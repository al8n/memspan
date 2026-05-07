//! Targeted tests filling coverage gaps that the topical test files miss.
//!
//! Three classes of gap addressed here:
//!
//! 1. **Needle counts that aren't otherwise exercised**: the `&[u8]` and
//!    `[u8; N]` impls dispatch on needle count via per-arm `match`. The
//!    existing topical tests cover N ∈ {1, 3, 4, 6, 7, 8} but skip 2 and 5
//!    for slices — and for fixed arrays they skip 2 and 5 entirely in the
//!    long-input (SIMD) paths. Each missing arm corresponds to a distinct
//!    `eq_any_mask_const_{sse2,avx2,avx512}` instantiation and a distinct
//!    `prefix_len{N}` / `tail_find{N}` scalar function.
//!
//! 2. **Inputs long enough to enter the unrolled 2×CHUNK loop**: the
//!    AVX-512 path has CHUNK=64 with a 64-byte probe, so the unrolled
//!    loop is only entered at len ≥ 192. The AVX2 path has CHUNK=32
//!    (probe 32, 2×CHUNK 64), so the unrolled loop is only entered at
//!    len ≥ 96. Existing ASCII-class tests cap at len=96 (just barely
//!    misses AVX-512 unrolled).
//!
//! 3. **Empty / dynamic slice arms** at long lengths — covers the
//!    `[] => …` empty arm and the `_ => eq_any_mask_dynamic_*` fallback
//!    arm in each slice impl.

use memspan::skip;

// Lengths picked to traverse every chunk-loop branch on every backend:
// - SSE  CHUNK=16 → unrolled loop at len ≥ 48; tail-overlap at len % 16 != 0
// - AVX2 CHUNK=32 → unrolled loop at len ≥ 96; tail-overlap at len % 32 != 0
// - AVX-512 CHUNK=64 → unrolled loop at len ≥ 192; tail-overlap at len % 64 != 0
const LONG_LENS: &[usize] = &[
  16, 17, 31, 32, 33, 47, 48, 63, 64, 65, 95, 96, 97, 127, 128, 129, 191, 192, 193, 255, 256, 257,
  319, 320, 384, 385,
];

fn scalar_count(input: &[u8], needles: &[u8]) -> usize {
  input.iter().filter(|&&b| needles.contains(&b)).count()
}

fn scalar_find_last(input: &[u8], needles: &[u8]) -> Option<usize> {
  input
    .iter()
    .enumerate()
    .rev()
    .find(|&(_, &b)| needles.contains(&b))
    .map(|(i, _)| i)
}

fn scalar_skip_until(input: &[u8], needles: &[u8]) -> Option<usize> {
  input.iter().position(|b| needles.contains(b))
}

fn scalar_skip_while(input: &[u8], needles: &[u8]) -> usize {
  input
    .iter()
    .position(|b| !needles.contains(b))
    .unwrap_or(input.len())
}

// ── Slice needle: count = 2 ──────────────────────────────────────────────────
// Fills the `[u8]::{tail_find, prefix_len, eq_any_mask_*}` `[a, b]` arms
// across every SIMD tier.

#[test]
fn slice_n2_skip_until_long() {
  let n: &[u8] = b"XY";
  for &len in LONG_LENS {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'Y';
    assert_eq!(
      skip::skip_until(input.as_slice(), n),
      Some(len - 1),
      "len={len}"
    );
    let miss = vec![b'a'; len];
    assert_eq!(skip::skip_until(miss.as_slice(), n), None, "len={len}");
  }
}

#[test]
fn slice_n2_skip_while_long() {
  let n: &[u8] = b"AB";
  for &len in LONG_LENS {
    let pattern: Vec<u8> = (0..len)
      .map(|i| if i % 2 == 0 { b'A' } else { b'B' })
      .collect();
    assert_eq!(
      skip::skip_while(pattern.as_slice(), n),
      len,
      "len={len}, all-match"
    );
    let mut miss = pattern.clone();
    miss[len - 1] = b'z';
    assert_eq!(
      skip::skip_while(miss.as_slice(), n),
      len - 1,
      "len={len}, miss tail"
    );
  }
}

#[test]
fn slice_n2_count_and_find_last_long() {
  let n: &[u8] = b"ae";
  for &len in LONG_LENS {
    let input: Vec<u8> = (0..len)
      .map(|i| match i % 5 {
        0 => b'a',
        1 => b'e',
        _ => b'z',
      })
      .collect();
    assert_eq!(
      skip::count_matches(input.as_slice(), n),
      scalar_count(&input, n),
      "len={len}"
    );
    assert_eq!(
      skip::find_last(input.as_slice(), n),
      scalar_find_last(&input, n),
      "len={len}"
    );
  }
}

// ── Slice needle: counts 1, 3, 6, 7, 8 at lengths past the probe chunk ──────
// The existing `slice_n{1,3,6,7,8}_skip_until` tests cap at len=64, which on
// the AVX-512 path is fully consumed by the 64-byte probe (`tail_find`),
// never invoking `eq_any_mask_avx512`. These tests add lengths past the
// probe so each per-arity slice dispatch arm in
// `[u8]::eq_any_mask_{sse2,avx2,avx512}` actually runs.
//
// Inputs are built with the hit at `len-1` so `tail_find` on the first chunk
// returns `None`, forcing the SIMD main loop to load and mask later chunks.

fn slice_post_probe<F>(n: &[u8], hit: u8, f: F)
where
  F: Fn(&[u8], &[u8], usize),
{
  for &len in LONG_LENS {
    if len < 65 {
      // Need at least one byte past the AVX-512 probe (CHUNK=64) so the
      // SIMD main loop is entered. SSE/AVX2 are already exercised by the
      // existing topical slice tests at len ∈ {16, 32, 64}.
      continue;
    }
    let mut input = vec![b'A'; len];
    input[len - 1] = hit;
    f(input.as_slice(), n, len);
  }
}

#[test]
fn slice_n1_post_probe() {
  slice_post_probe(b"X", b'X', |input, n, len| {
    assert_eq!(skip::skip_until(input, n), Some(len - 1), "len={len}");
  });
}

#[test]
fn slice_n3_post_probe() {
  slice_post_probe(b"XYZ", b'Z', |input, n, len| {
    assert_eq!(skip::skip_until(input, n), Some(len - 1), "len={len}");
  });
}

#[test]
fn slice_n6_post_probe() {
  slice_post_probe(b"123456", b'6', |input, n, len| {
    assert_eq!(skip::skip_until(input, n), Some(len - 1), "len={len}");
  });
}

#[test]
fn slice_n7_post_probe() {
  slice_post_probe(b"1234567", b'7', |input, n, len| {
    assert_eq!(skip::skip_until(input, n), Some(len - 1), "len={len}");
  });
}

#[test]
fn slice_n8_post_probe() {
  slice_post_probe(b"12345678", b'8', |input, n, len| {
    assert_eq!(skip::skip_until(input, n), Some(len - 1), "len={len}");
  });
}

// Same family, but for skip_while → exercises the slice impl's
// `eq_any_mask_*` arms via the all-match SIMD scan path.

fn slice_skip_while_post_probe(n: &[u8]) {
  for &len in LONG_LENS {
    if len < 65 {
      continue;
    }
    let pattern: Vec<u8> = (0..len).map(|i| n[i % n.len()]).collect();
    assert_eq!(
      skip::skip_while(pattern.as_slice(), n),
      len,
      "len={len}, all-match"
    );
    let mut miss = pattern.clone();
    miss[len - 1] = b'!';
    assert_eq!(
      skip::skip_while(miss.as_slice(), n),
      len - 1,
      "len={len}, miss tail"
    );
  }
}

#[test]
fn slice_skip_while_n3_post_probe() {
  slice_skip_while_post_probe(b"abc");
}

#[test]
fn slice_skip_while_n6_post_probe() {
  slice_skip_while_post_probe(b"abcdef");
}

#[test]
fn slice_skip_while_n7_post_probe() {
  slice_skip_while_post_probe(b"abcdefg");
}

#[test]
fn slice_skip_while_n8_post_probe() {
  slice_skip_while_post_probe(b"abcdefgh");
}

// ── Slice needle: count = 5 ──────────────────────────────────────────────────
// Fills the `[u8]::{tail_find, prefix_len, eq_any_mask_*}` `[a, b, c, d, e]`
// arms and indirectly `prefix_len5` / `tail_find5` (the only fixed-N scalar
// helpers in the 1..=8 family that no other slice test reaches).

#[test]
fn slice_n5_skip_until_long() {
  let n: &[u8] = b"12345";
  for &len in LONG_LENS {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'5';
    assert_eq!(
      skip::skip_until(input.as_slice(), n),
      Some(len - 1),
      "len={len}"
    );
    let miss = vec![b'a'; len];
    assert_eq!(skip::skip_until(miss.as_slice(), n), None, "len={len}");
  }
}

#[test]
fn slice_n5_skip_while_long() {
  let n: &[u8] = b"aeiou";
  for &len in LONG_LENS {
    let pattern: Vec<u8> = (0..len).map(|i| n[i % 5]).collect();
    assert_eq!(
      skip::skip_while(pattern.as_slice(), n),
      len,
      "len={len}, all-match"
    );
    for &miss_pos in &[0usize, len / 2, len - 1] {
      let mut miss = pattern.clone();
      miss[miss_pos] = b'Z';
      assert_eq!(
        skip::skip_while(miss.as_slice(), n),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
  }
}

#[test]
fn slice_n5_count_and_find_last_long() {
  let n: &[u8] = b"aeiou";
  for &len in LONG_LENS {
    let input: Vec<u8> = (0..len)
      .map(|i| match i % 7 {
        0 => b'a',
        1 => b'e',
        2 => b'i',
        3 => b'o',
        4 => b'u',
        _ => b'z',
      })
      .collect();
    assert_eq!(
      skip::count_matches(input.as_slice(), n),
      scalar_count(&input, n),
      "len={len}"
    );
    assert_eq!(
      skip::find_last(input.as_slice(), n),
      scalar_find_last(&input, n),
      "len={len}"
    );
  }
}

// ── Fixed arrays [u8; 2] and [u8; 5] at SIMD-loop lengths ────────────────────
// Fills the `eq_any_mask_const_{sse2,avx2,avx512}` N=2 and N=5 arms
// (existing tests reach N ∈ {1, 3, 4, 6, 7, 8, 9} for fixed arrays).

#[test]
fn fixed_array_n2_long() {
  let n = [b'X', b'Y'];
  for &len in LONG_LENS {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'Y';
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'a'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
    let pattern: Vec<u8> = (0..len)
      .map(|i| if i % 2 == 0 { b'X' } else { b'Y' })
      .collect();
    assert_eq!(skip::skip_while(&pattern, n), len, "len={len}, all-match");
    assert_eq!(skip::count_matches(&pattern, n), len, "len={len}, count");
    assert_eq!(
      skip::find_last(&pattern, n),
      Some(len - 1),
      "len={len}, find_last"
    );
  }
}

#[test]
fn fixed_array_n5_long() {
  let n = [b'1', b'2', b'3', b'4', b'5'];
  for &len in LONG_LENS {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'5';
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'a'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
    let pattern: Vec<u8> = (0..len).map(|i| n[i % 5]).collect();
    assert_eq!(skip::skip_while(&pattern, n), len, "len={len}, all-match");
    assert_eq!(skip::count_matches(&pattern, n), len, "len={len}, count");
    assert_eq!(
      skip::find_last(&pattern, n),
      Some(len - 1),
      "len={len}, find_last"
    );
  }
}

// ── Built-in ASCII classes at AVX-512-unrolled-loop lengths ──────────────────
// The existing `tests/skip_ascii_classes.rs` boundary helper caps at len=96,
// which only just enters the AVX2 unrolled loop and never enters the AVX-512
// unrolled loop (needs len ≥ 192). These tests close that gap with miss
// positions inside each chunk of the unrolled loop on every tier.

fn assert_class_long<F>(scan: F, fill: u8, miss: u8, name: &str)
where
  F: Fn(&[u8]) -> usize,
{
  for &len in LONG_LENS {
    if len < 96 {
      continue;
    }
    // All-match.
    let input = vec![fill; len];
    assert_eq!(scan(&input), len, "{name}: len={len}, all-match");
    // Miss at every position spanning the AVX-512 unrolled loop region
    // (probe + first-unrolled-chunk + second-unrolled-chunk + tail).
    // Sampling positions instead of exhaustive keeps the test fast.
    for &miss_pos in &[
      0usize,
      15,
      31,
      63,  // boundary of AVX-512 probe chunk
      64,  // first byte after probe
      95,  // first byte of AVX2 unrolled second chunk
      127, // last byte of AVX-512 first unrolled chunk
      128, // first byte of AVX-512 second unrolled chunk
      191, // last byte of AVX-512 unrolled iteration
      len / 2,
      len - 65,
      len - 33,
      len - 1,
    ] {
      if miss_pos >= len {
        continue;
      }
      let mut input = vec![fill; len];
      input[miss_pos] = miss;
      assert_eq!(
        scan(&input),
        miss_pos,
        "{name}: len={len}, miss_pos={miss_pos}"
      );
    }
  }
}

#[test]
fn ascii_classes_long_inputs() {
  assert_class_long(skip::skip_binary, b'1', b'2', "skip_binary");
  assert_class_long(skip::skip_octal_digits, b'7', b'8', "skip_octal_digits");
  assert_class_long(skip::skip_digits, b'9', b'a', "skip_digits");
  assert_class_long(skip::skip_hex_digits, b'F', b'g', "skip_hex_digits");
  assert_class_long(skip::skip_whitespace, b' ', b'a', "skip_whitespace");
  assert_class_long(skip::skip_alpha, b'a', b'1', "skip_alpha");
  assert_class_long(skip::skip_alphanumeric, b'A', b'!', "skip_alphanumeric");
  assert_class_long(skip::skip_ident_start, b'_', b'1', "skip_ident_start");
  assert_class_long(skip::skip_ident, b'_', b'!', "skip_ident");
  assert_class_long(skip::skip_lower, b'a', b'A', "skip_lower");
  assert_class_long(skip::skip_upper, b'Z', b'z', "skip_upper");
  assert_class_long(skip::skip_ascii, b'A', 0x80, "skip_ascii");
  assert_class_long(skip::skip_non_ascii, 0x80, b'A', "skip_non_ascii");
  assert_class_long(skip::skip_ascii_graphic, b'a', b' ', "skip_ascii_graphic");
  assert_class_long(skip::skip_ascii_control, 0x01, b' ', "skip_ascii_control");
}

// ── Generic skip_until / skip_while at AVX-512-unrolled lengths ──────────────
// Same gap as above but for the needle-driven generic SIMD functions
// (sse42::{skip_until,skip_while,count_matches,find_last} and the AVX2 /
// AVX-512 analogues). Exercises miss positions specifically inside each
// unrolled-loop chunk so both `b0 != …` and `b1 != …` branches fire.

#[test]
fn skip_until_unrolled_loop_branches() {
  // 4-needle (covers the [a,b,c,d] arms broadly); 2 and 5 are covered above.
  let n_arr = [b'!', b'@', b'#', b'$'];
  let n_slice: &[u8] = b"!@#$";
  for &len in LONG_LENS {
    if len < 96 {
      continue;
    }
    for &hit_pos in &[64usize, 95, 127, 128, 191, len - 65, len - 32, len - 1] {
      if hit_pos >= len {
        continue;
      }
      let mut input = vec![b'a'; len];
      input[hit_pos] = b'#';
      assert_eq!(
        skip::skip_until(&input, n_arr),
        Some(hit_pos),
        "arr: len={len}, hit_pos={hit_pos}"
      );
      assert_eq!(
        skip::skip_until(input.as_slice(), n_slice),
        Some(hit_pos),
        "slice: len={len}, hit_pos={hit_pos}"
      );
    }
  }
}

#[test]
fn skip_while_unrolled_loop_branches() {
  let n_arr = [b' ', b'\t', b'\r', b'\n', b','];
  let n_slice: &[u8] = b" \t\r\n,";
  for &len in LONG_LENS {
    if len < 96 {
      continue;
    }
    for &miss_pos in &[64usize, 95, 127, 128, 191, len - 65, len - 32, len - 1] {
      if miss_pos >= len {
        continue;
      }
      let mut input = vec![b' '; len];
      input[miss_pos] = b'a';
      assert_eq!(
        skip::skip_while(&input, n_arr),
        miss_pos,
        "arr: len={len}, miss_pos={miss_pos}"
      );
      assert_eq!(
        skip::skip_while(input.as_slice(), n_slice),
        miss_pos,
        "slice: len={len}, miss_pos={miss_pos}"
      );
    }
  }
}

#[test]
fn count_and_find_last_unrolled_loop_long() {
  let n_arr = [b'a', b'e', b'i', b'o', b'u'];
  let n_slice: &[u8] = b"aeiou";
  for &len in LONG_LENS {
    if len < 96 {
      continue;
    }
    let input: Vec<u8> = (0..len)
      .map(|i| match i % 7 {
        0 => b'a',
        1 => b'e',
        2 => b'i',
        3 => b'o',
        4 => b'u',
        _ => b'z',
      })
      .collect();
    assert_eq!(
      skip::count_matches(&input, n_arr),
      scalar_count(&input, n_slice),
      "arr count len={len}"
    );
    assert_eq!(
      skip::count_matches(input.as_slice(), n_slice),
      scalar_count(&input, n_slice),
      "slice count len={len}"
    );
    assert_eq!(
      skip::find_last(&input, n_arr),
      scalar_find_last(&input, n_slice),
      "arr find_last len={len}"
    );
    assert_eq!(
      skip::find_last(input.as_slice(), n_slice),
      scalar_find_last(&input, n_slice),
      "slice find_last len={len}"
    );
  }
}

// ── Empty needle slice at SIMD lengths ───────────────────────────────────────
// Exercises the `[] => …` arm in each slice impl of `eq_any_mask_*`.
// `skip_until` and `skip_while` short-circuit on `needle_count() == 0`
// before reaching the SIMD layer, so we go through `count_matches` and
// `find_last` (which don't have that early-return) to actually instantiate
// the empty arm.

#[test]
fn empty_slice_count_and_find_last_long() {
  let empty: &[u8] = &[];
  for &len in LONG_LENS {
    let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_add(1)).collect();
    assert_eq!(skip::count_matches(input.as_slice(), empty), 0, "len={len}");
    assert_eq!(skip::find_last(input.as_slice(), empty), None, "len={len}");
  }
}

// ── Dynamic (>8 needles) slice at SIMD lengths ───────────────────────────────
// Exercises the `_ => eq_any_mask_dynamic_*` fallback arm in each slice impl.

#[test]
fn dynamic_slice_long_inputs() {
  let n: &[u8] = b"0123456789abcdef"; // 16 needles → dynamic arm
  for &len in LONG_LENS {
    let input: Vec<u8> = (0..len).map(|i| n[i % n.len()]).collect();
    assert_eq!(skip::skip_while(input.as_slice(), n), len, "len={len}");
    assert_eq!(
      skip::count_matches(input.as_slice(), n),
      scalar_count(&input, n),
      "len={len}"
    );
    assert_eq!(
      skip::find_last(input.as_slice(), n),
      scalar_find_last(&input, n),
      "len={len}"
    );

    let mut input = vec![b'!'; len];
    input[len - 1] = b'f';
    assert_eq!(
      skip::skip_until(input.as_slice(), n),
      Some(len - 1),
      "len={len}"
    );
    assert_eq!(
      skip::skip_until(input.as_slice(), n),
      scalar_skip_until(&input, n),
      "len={len}"
    );
    let pattern: Vec<u8> = (0..len).map(|i| n[i % n.len()]).collect();
    assert_eq!(
      skip::skip_while(pattern.as_slice(), n),
      scalar_skip_while(&pattern, n),
      "len={len}"
    );
  }
}

// ── &T delegation at long lengths ────────────────────────────────────────────
// Exercises the blanket `Needles for &T` impl across every SIMD tier — the
// delegation forwards `needle_count`, `tail_find`, `prefix_len`, and every
// `eq_any_mask_*` to `(**self)`. Existing `ref_delegation_*` tests only
// reach the scalar/short paths.

#[test]
fn ref_delegation_long() {
  let n: &[u8] = b"abcde";
  let rn: &&[u8] = &&n;
  for &len in LONG_LENS {
    let input: Vec<u8> = (0..len).map(|i| n[i % n.len()]).collect();
    assert_eq!(skip::skip_while(input.as_slice(), rn), len, "len={len}");
    assert_eq!(skip::count_matches(input.as_slice(), rn), len, "len={len}");
    assert_eq!(
      skip::find_last(input.as_slice(), rn),
      Some(len - 1),
      "len={len}"
    );
    let mut needle_input = vec![b'!'; len];
    needle_input[len - 1] = b'a';
    assert_eq!(
      skip::skip_until(needle_input.as_slice(), rn),
      Some(len - 1),
      "len={len}"
    );
  }
}

// ── is_empty default-method body on Needles trait ────────────────────────────
// `Needles::is_empty` is a default trait method that delegates to
// `needle_count()`. None of the topical tests call it directly, so the
// default body itself shows as uncovered.

#[test]
fn is_empty_default_method() {
  use memspan::Needles;

  fn check<N: Needles>(n: N, want_empty: bool) {
    assert_eq!(n.is_empty(), want_empty);
  }

  check(b'X', false);
  check([b'a', b'b'], false);
  check::<[u8; 0]>([], true);
  let empty_slice: &[u8] = &[];
  check(empty_slice, true);
  let nonempty_slice: &[u8] = b"hello";
  check(nonempty_slice, false);
}
