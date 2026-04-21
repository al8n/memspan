//! Coverage for the lexer-oriented `skip_*` family
//! (`skip_whitespace`, `skip_alpha`, `skip_alphanumeric`,
//! `skip_ident_start`, `skip_ident`).
//!
//! Every test pairs a basic-correctness check with an exhaustive byte-table
//! sweep so the SIMD mask is verified against the reference predicate over
//! the entire `u8` space, plus an exhaustive miss-position scan over lengths
//! that span the probe boundary, the SIMD loop, and the overlap tail.

use skipchr::skip;

// ---- helpers --------------------------------------------------------------

fn ref_is_alpha(b: u8) -> bool {
  let lower = b | 0x20;
  lower.is_ascii_lowercase()
}
fn ref_is_digit(b: u8) -> bool {
  b.is_ascii_digit()
}

fn assert_byte_table<F, P>(skip_fn: F, pred: P, name: &str)
where
  F: Fn(&[u8]) -> usize,
  P: Fn(u8) -> bool,
{
  for b in 0u8..=255 {
    let input = [b];
    let got = skip_fn(&input);
    let want = if pred(b) { 1 } else { 0 };
    assert_eq!(
      got,
      want,
      "{name}: byte 0x{b:02x} ({:?}) — pred says match={}",
      b as char,
      pred(b)
    );
  }
}

fn assert_miss_position_exhaustive<F>(skip_fn: F, fill: u8, miss: u8, name: &str)
where
  F: Fn(&[u8]) -> usize,
{
  for len in 1usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![fill; len];
      input[miss_pos] = miss;
      assert_eq!(
        skip_fn(&input),
        miss_pos,
        "{name}: len={len}, miss_pos={miss_pos}"
      );
    }
    let input = vec![fill; len];
    assert_eq!(skip_fn(&input), len, "{name}: len={len}, all-match");
  }
}

// ---- skip_whitespace -----------------------------------------------------

#[test]
fn skip_whitespace_basic() {
  assert_eq!(skip::skip_whitespace(b""), 0);
  assert_eq!(skip::skip_whitespace(b"   "), 3);
  assert_eq!(skip::skip_whitespace(b" \t\r\n  x"), 6);
  assert_eq!(skip::skip_whitespace(b"a   "), 0);
  // Vertical tab (0x0B) and form feed (0x0C) are NOT in our set.
  assert_eq!(skip::skip_whitespace(b" \x0B"), 1);
  assert_eq!(skip::skip_whitespace(b" \x0C"), 1);
}

#[test]
fn skip_whitespace_byte_table() {
  assert_byte_table(
    skip::skip_whitespace,
    |b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'),
    "skip_whitespace",
  );
}

#[test]
fn skip_whitespace_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip::skip_whitespace, b' ', b'a', "skip_whitespace");
}

// ---- skip_alpha -----------------------------------------------------------

#[test]
fn skip_alpha_basic() {
  assert_eq!(skip::skip_alpha(b""), 0);
  assert_eq!(skip::skip_alpha(b"abcXYZ"), 6);
  assert_eq!(skip::skip_alpha(b"abc1"), 3);
  assert_eq!(skip::skip_alpha(b"_abc"), 0); // '_' not in alpha
  // Punctuation that case-folds into nearby ranges must NOT match:
  assert_eq!(skip::skip_alpha(b"@"), 0); // 0x40 → 0x60 (still < 'a')
  assert_eq!(skip::skip_alpha(b"`"), 0); // 0x60 just below 'a'
  assert_eq!(skip::skip_alpha(b"["), 0); // 0x5B → 0x7B (just above 'z')
  assert_eq!(skip::skip_alpha(b"{"), 0); // 0x7B just above 'z'
}

#[test]
fn skip_alpha_byte_table() {
  assert_byte_table(skip::skip_alpha, ref_is_alpha, "skip_alpha");
}

#[test]
fn skip_alpha_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip::skip_alpha, b'A', b'1', "skip_alpha");
  assert_miss_position_exhaustive(skip::skip_alpha, b'z', b'_', "skip_alpha");
}

// ---- skip_alphanumeric ---------------------------------------------------

#[test]
fn skip_alphanumeric_basic() {
  assert_eq!(skip::skip_alphanumeric(b""), 0);
  assert_eq!(skip::skip_alphanumeric(b"abc123XYZ"), 9);
  assert_eq!(skip::skip_alphanumeric(b"abc_"), 3); // '_' not in set
  assert_eq!(skip::skip_alphanumeric(b"-abc"), 0);
}

#[test]
fn skip_alphanumeric_byte_table() {
  assert_byte_table(
    skip::skip_alphanumeric,
    |b| ref_is_alpha(b) || ref_is_digit(b),
    "skip_alphanumeric",
  );
}

#[test]
fn skip_alphanumeric_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip::skip_alphanumeric, b'a', b'-', "skip_alphanumeric");
}

// ---- skip_ident_start ----------------------------------------------------

#[test]
fn skip_ident_start_basic() {
  assert_eq!(skip::skip_ident_start(b""), 0);
  assert_eq!(skip::skip_ident_start(b"a"), 1);
  assert_eq!(skip::skip_ident_start(b"_"), 1);
  assert_eq!(skip::skip_ident_start(b"_foo"), 4);
  assert_eq!(skip::skip_ident_start(b"foo_"), 4);
  assert_eq!(skip::skip_ident_start(b"1abc"), 0); // digit not allowed at start
  assert_eq!(skip::skip_ident_start(b"ab1"), 2);
}

#[test]
fn skip_ident_start_byte_table() {
  assert_byte_table(
    skip::skip_ident_start,
    |b| ref_is_alpha(b) || b == b'_',
    "skip_ident_start",
  );
}

#[test]
fn skip_ident_start_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip::skip_ident_start, b'a', b'1', "skip_ident_start");
  assert_miss_position_exhaustive(skip::skip_ident_start, b'_', b'-', "skip_ident_start");
}

// ---- skip_ident ----------------------------------------------------------

#[test]
fn skip_ident_basic() {
  assert_eq!(skip::skip_ident(b""), 0);
  assert_eq!(skip::skip_ident(b"my_var_42"), 9);
  assert_eq!(skip::skip_ident(b"_foo123"), 7);
  assert_eq!(skip::skip_ident(b"foo-bar"), 3);
  assert_eq!(skip::skip_ident(b"hello world"), 5);
}

#[test]
fn skip_ident_byte_table() {
  assert_byte_table(
    skip::skip_ident,
    |b| ref_is_alpha(b) || ref_is_digit(b) || b == b'_',
    "skip_ident",
  );
}

#[test]
fn skip_ident_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip::skip_ident, b'a', b'-', "skip_ident");
  assert_miss_position_exhaustive(skip::skip_ident, b'9', b' ', "skip_ident");
}

/// Realistic: `skip_ident_start` then `skip_ident` together should consume a
/// full identifier, regardless of how it straddles chunk boundaries.
#[test]
fn skip_ident_start_then_continuation_is_full_identifier() {
  for ident in [
    "x".to_string(),
    "abc".to_string(),
    "_foo".to_string(),
    "my_long_variable_name_42".to_string(),
    "X".repeat(80),
    format!("_{}", "a".repeat(64)),
  ] {
    let mut input = ident.as_bytes().to_vec();
    input.push(b' ');
    let head = skip::skip_ident_start(&input);
    assert!(head > 0, "ident={:?} — must consume at least 1 byte", ident);
    let total = head + skip::skip_ident(&input[head..]);
    assert_eq!(
      total,
      ident.len(),
      "ident={ident:?}: head={head}, total={total}"
    );
  }
}
