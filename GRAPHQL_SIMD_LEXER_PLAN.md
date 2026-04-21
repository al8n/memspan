# SIMD-Accelerated GraphQL Lexer Plan

This document summarizes a simdjson-inspired optimization strategy for a
GraphQL lexer. The goal is not to SIMD-optimize every token parser directly.
Instead, the lexer should use SIMD to scan large blocks, discover important
byte positions, and let the normal parser emit tokens one by one.

## Core Idea

A traditional lexer often advances byte by byte:

```text
read byte
branch
skip whitespace
branch
try punctuation
branch
try identifier
branch
try number
...
```

This creates many tiny branches and does not give SIMD enough work to amortize
setup cost. For short tokens such as `id`, `123`, or `on`, calling a SIMD
scanner per token can be slower than scalar code.

The simdjson-style approach is different:

```text
Stage 1:
  scan a large block of input
  produce bitmasks for interesting byte classes
  extract token landmarks into a small reusable buffer

Stage 2:
  consume landmarks
  emit exactly one token at a time
  delegate complex tokens to specialized parsers
```

The external lexer API can still be:

```rust
fn next_token(&mut self) -> Option<Result<Token<'src>, TokenError>>;
```

Only the internal navigation becomes block based.

## What Stage 1 Should Find

For GraphQL, Stage 1 should identify landmarks rather than fully parsed tokens.

Useful byte classes:

```text
ignored:
  space, tab, comma, carriage return, line feed, UTF-8 BOM

punctuation:
  ! $ & ( ) : = @ [ ] { | }

special punctuation:
  .        // spread operator or spread-related error

slow-path entrances:
  "        // inline string or block string
  #        // comment
  - 0-9    // number
  A-Z a-z _ // GraphQL Name

invalid:
  any non-ignored byte that cannot start a valid token
```

GraphQL comments and ignored tokens matter:

```text
Ignored = whitespace, comma, BOM, line terminator, comment
Comment = # followed by any bytes until line terminator
```

The scanner must not skip invalid bytes. Invalid positions should become
landmarks too, otherwise the lexer may accidentally jump over errors.

## No Bitmap Allocation

The "bitmap" in this design is not a heap allocation. It is usually one local
integer per byte class:

```rust
struct GraphqlMasks {
  ignored: u64,
  punct: u64,
  dot: u64,
  quote: u64,
  comment: u64,
  name_start: u64,
  name_continue: u64,
  number_start: u64,
  invalid: u64,
}
```

Each bit represents one byte in the current block:

```text
bit 0  -> block[0]
bit 1  -> block[1]
...
bit 63 -> block[63]
```

The masks live in registers or on the stack. They are immediately consumed to
produce positions.

The only persistent storage should be a reusable position buffer:

```rust
pub struct LandmarkBatch<const N: usize> {
  positions: [u32; N],
  len: usize,
  read: usize,
}
```

This avoids per-block allocation. If a full-file index is desired later, it can
use a `Vec<u32>` that is allocated once and reused with `clear()`.

## Landmark Extraction

Stage 1 combines the masks into one set of interesting positions:

```rust
let landmarks =
    masks.punct
  | masks.dot
  | masks.quote
  | masks.comment
  | masks.name_token_start
  | masks.number_start
  | masks.invalid;
```

Then it extracts positions with trailing-zero iteration:

```rust
let mut bits = landmarks;

while bits != 0 {
  let lane = bits.trailing_zeros() as usize;
  batch.push(block_base + lane);
  bits &= bits - 1;
}
```

This is the key performance shape:

```text
one SIMD block classification
one or more u64 masks
cheap bit iteration
small reused position buffer
```

## Identifier Start and Span

GraphQL Name has two related but different classes:

```text
NameStart    = [_A-Za-z]
NameContinue = [_0-9A-Za-z]
Name         = NameStart NameContinue*
```

`name_start` is for discovering token starts. `name_continue` is for discovering
token boundaries.

The correct start mask is not just `name_start`, because every letter inside a
name is also a valid start character. The start mask should also require that
the previous byte was not a name continuation:

```rust
let prev_name_continue = (name_continue << 1) | prev_block_ended_with_name_continue_bit;
let name_token_start = name_start & !prev_name_continue;
```

Once Stage 2 lands on a name start, the span is found by scanning until the
first non-`name_continue` byte:

```rust
fn find_name_end_in_block(
  block_base: usize,
  pos: usize,
  name_continue: u64,
) -> Option<usize> {
  let lane = pos - block_base;
  let remaining = 64 - lane;
  let valid = if remaining == 64 {
    u64::MAX
  } else {
    (1u64 << remaining) - 1
  };

  let cont_from_here = (name_continue >> lane) & valid;
  let stop_bits = !cont_from_here & valid;

  if stop_bits == 0 {
    None
  } else {
    Some(pos + stop_bits.trailing_zeros() as usize)
  }
}
```

If the name continues to the end of the current block, the lexer refills the
next block and repeats. The tail can fall back to scalar scanning.

## Contextual Keywords

GraphQL does not have many global strict keywords in the lexer sense. Most
keywords are contextual names:

```text
query
mutation
subscription
fragment
on
schema
scalar
type
interface
union
enum
input
extend
implements
directive
repeatable
```

The value keywords are special in value context:

```text
true
false
null
```

However, the lexer should usually emit a `Name` token and let the parser decide
what the name means in context. For example, a field can be named `query` or
`type`.

Recommended Stage 2 classification:

```rust
pub enum GraphqlKeyword {
  Query,
  Mutation,
  Subscription,
  Fragment,
  On,
  True,
  False,
  Null,
  Schema,
  Scalar,
  Type,
  Interface,
  Union,
  Enum,
  Input,
  Extend,
  Implements,
  Directive,
  Repeatable,
}

#[inline(always)]
pub fn classify_graphql_name(name: &[u8]) -> Option<GraphqlKeyword> {
  match name.len() {
    2 => match name {
      b"on" => Some(GraphqlKeyword::On),
      _ => None,
    },
    4 => match name {
      b"true" => Some(GraphqlKeyword::True),
      b"null" => Some(GraphqlKeyword::Null),
      b"type" => Some(GraphqlKeyword::Type),
      b"enum" => Some(GraphqlKeyword::Enum),
      _ => None,
    },
    5 => match name {
      b"query" => Some(GraphqlKeyword::Query),
      b"false" => Some(GraphqlKeyword::False),
      b"union" => Some(GraphqlKeyword::Union),
      b"input" => Some(GraphqlKeyword::Input),
      _ => None,
    },
    6 => match name {
      b"schema" => Some(GraphqlKeyword::Schema),
      b"scalar" => Some(GraphqlKeyword::Scalar),
      b"extend" => Some(GraphqlKeyword::Extend),
      _ => None,
    },
    8 => match name {
      b"mutation" => Some(GraphqlKeyword::Mutation),
      b"fragment" => Some(GraphqlKeyword::Fragment),
      _ => None,
    },
    9 => match name {
      b"interface" => Some(GraphqlKeyword::Interface),
      b"directive" => Some(GraphqlKeyword::Directive),
      b"repeatable" => Some(GraphqlKeyword::Repeatable),
      _ => None,
    },
    10 => match name {
      b"implements" => Some(GraphqlKeyword::Implements),
      _ => None,
    },
    12 => match name {
      b"subscription" => Some(GraphqlKeyword::Subscription),
      _ => None,
    },
    _ => None,
  }
}
```

This length-first scalar classifier is usually better than building exact
keyword masks in Stage 1. Stage 1 should find name starts. Stage 2 should
classify names only after their spans are known.

## Main Loop Shape

The main loop should consume landmarks but still emit one token per call:

```rust
impl<'src> GraphqlLexer<'src> {
  pub fn next_token(&mut self) -> Option<Result<Token<'src>, TokenError>> {
    loop {
      let pos = self.next_landmark()?;
      self.cursor = pos;

      let byte = self.bytes[pos];

      return Some(match byte {
        b'!' => self.emit_bang(pos),
        b'$' => self.emit_dollar(pos),
        b'&' => self.emit_ampersand(pos),
        b'(' => self.emit_lparen(pos),
        b')' => self.emit_rparen(pos),
        b':' => self.emit_colon(pos),
        b'=' => self.emit_equal(pos),
        b'@' => self.emit_at(pos),
        b'[' => self.emit_lbracket(pos),
        b']' => self.emit_rbracket(pos),
        b'{' => self.emit_lbrace(pos),
        b'}' => self.emit_rbrace(pos),
        b'|' => self.emit_pipe(pos),

        b'.' => self.lex_spread_or_error(pos),

        b'#' => {
          self.skip_comment(pos);
          continue;
        }

        b'"' => self.lex_string(pos),

        b'-' | b'0'..=b'9' => self.lex_number(pos),

        b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.lex_name(pos),

        0xEF if self.bytes[pos..].starts_with(b"\xEF\xBB\xBF") => {
          self.cursor = pos + 3;
          continue;
        }

        _ => self.error_unexpected_byte(pos),
      });
    }
  }
}
```

The important property is that the lexer does not return a batch of tokens.
It only uses a batch of byte positions internally.

## Integration With Logos

The safest migration path is incremental:

1. Keep the existing Logos lexer as the correctness reference.
2. Add a SIMD landmark scanner that can find punctuation, comments, strings,
   names, numbers, and invalid byte entrances.
3. Implement fast direct emission for punctuation.
4. Implement name scanning manually with `name_continue` masks.
5. Keep string parsing on the existing slow path first.
6. Keep number parsing on the existing slow path first, because GraphQL number
   errors are strict and subtle.
7. After the landmark pipeline is correct, translate number parsing into a
   hand-written parser and benchmark again.

The current Logos number rules contain many important error cases:

```text
leading zero
missing integer part
missing fraction digits
missing exponent digits
unexpected suffix
unexpected plus or minus token
```

Do not replace them with a fast scanner until the exact same errors can be
reproduced.

## Where SIMD Helps Most

SIMD is most useful for:

```text
finding punctuation across large input blocks
finding name starts
finding quote and comment entrances
skipping long ignored regions
detecting invalid bytes early
reducing byte-by-byte lexer branching
```

SIMD is less useful for:

```text
parsing very short identifiers
parsing very short numbers
classifying contextual keywords
one-call-per-token skip helpers on tiny slices
```

This matches the benchmark behavior observed for `skip_while`: scalar code
often wins for very short runs, while SIMD wins once work is large enough to
amortize setup and dispatch cost.

## Correctness Pitfalls

Important details to preserve:

```text
comma is ignored in GraphQL
BOM is ignored only as UTF-8 bytes EF BB BF
comments end at CR or LF
spread is exactly "..."
"." and ".." should produce spread-related errors
block string starts with triple quote
inline string starts with single quote
invalid bytes must become landmarks
names use NameStart for starts and NameContinue for spans
number suffix errors must not be hidden by landmark skipping
recursion depth updates still happen for braces, brackets, and parentheses
```

## Recommended Architecture

The long-term architecture should look like this:

```text
GraphqlLexer
  owns cursor state
  owns recursion limiter
  owns one reusable LandmarkBatch
  owns current block masks or scanner state

GraphqlScanner
  performs SIMD block classification
  produces GraphqlMasks
  extracts landmarks into LandmarkBatch

Slow paths
  string parser
  block string parser
  number parser
  error handlers

Parser
  receives one token at a time
  interprets contextual keywords by grammar position
```

This design keeps the public lexer simple while letting SIMD optimize the part
that matters most: navigation through the input.

## Summary

The GraphQL lexer should not try to SIMD-parse every token. It should use SIMD
to build a cheap, allocation-free stream of landmarks:

```text
punctuation
quote
comment
name start
number start
dot
invalid byte
```

Then Stage 2 emits tokens one at a time. Simple punctuation can be emitted
directly. Names can be scanned with `name_continue` masks and classified by
length. Strings and numbers should initially reuse the existing robust slow
paths.

This applies the simdjson idea to GraphQL: SIMD does not replace the parser.
SIMD helps the parser avoid looking at unimportant bytes.
