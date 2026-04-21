use lexsimd::skip;

// ---- skip_binary -----------------------------------------------------------

#[test]
fn skip_binary_basic() {
  assert_eq!(skip::skip_binary(b""), 0);
  assert_eq!(skip::skip_binary(b"0"), 1);
  assert_eq!(skip::skip_binary(b"1"), 1);
  assert_eq!(skip::skip_binary(b"01010101"), 8);
  assert_eq!(skip::skip_binary(b"012"), 2); // '2' not binary
  assert_eq!(skip::skip_binary(b"a01"), 0);
}

#[test]
fn skip_binary_chunk_boundaries_exhaustive() {
  for len in 1usize..=80 {
    // miss at each position
    for miss_pos in 0..len {
      let mut input = vec![b'0'; len];
      input[miss_pos] = b'2';
      assert_eq!(
        skip::skip_binary(&input),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
    // all-match
    let input = vec![b'1'; len];
    assert_eq!(skip::skip_binary(&input), len, "len={len}, all-match");
  }
}

// ---- skip_octal_digits ----------------------------------------------------

#[test]
fn skip_octal_basic() {
  assert_eq!(skip::skip_octal_digits(b""), 0);
  assert_eq!(skip::skip_octal_digits(b"01234567"), 8);
  assert_eq!(skip::skip_octal_digits(b"0128"), 3); // '8' at idx 3 not octal
  assert_eq!(skip::skip_octal_digits(b"017a"), 3); // 'a' at idx 3
  assert_eq!(skip::skip_octal_digits(b"79a"), 1); // '9' at idx 1 not octal
}

#[test]
fn skip_octal_chunk_boundaries_exhaustive() {
  for len in 1usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![b'7'; len];
      input[miss_pos] = b'8';
      assert_eq!(
        skip::skip_octal_digits(&input),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
    let input = vec![b'5'; len];
    assert_eq!(skip::skip_octal_digits(&input), len, "len={len}, all-match");
  }
}

// ---- skip_digits ----------------------------------------------------------

#[test]
fn skip_digits_basic() {
  assert_eq!(skip::skip_digits(b""), 0);
  assert_eq!(skip::skip_digits(b"0123456789"), 10);
  assert_eq!(skip::skip_digits(b"123abc"), 3);
  assert_eq!(skip::skip_digits(b"a123"), 0);
}

#[test]
fn skip_digits_chunk_boundaries_exhaustive() {
  for len in 1usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![b'5'; len];
      input[miss_pos] = b'a';
      assert_eq!(
        skip::skip_digits(&input),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
    let input = vec![b'9'; len];
    assert_eq!(skip::skip_digits(&input), len, "len={len}, all-match");
  }
}

// ---- skip_hex_digits ------------------------------------------------------

#[test]
fn skip_hex_digits_basic() {
  assert_eq!(skip::skip_hex_digits(b""), 0);
  assert_eq!(skip::skip_hex_digits(b"deadBEEF"), 8);
  assert_eq!(skip::skip_hex_digits(b"0123456789abcdefABCDEF"), 22);
  assert_eq!(skip::skip_hex_digits(b"ffg"), 2); // 'g' is not hex
  assert_eq!(skip::skip_hex_digits(b"FFG"), 2); // 'G' is not hex
  assert_eq!(skip::skip_hex_digits(b"123:"), 3); // ':' (0x3a) not hex
  assert_eq!(skip::skip_hex_digits(b"@"), 0); // '@' (0x40 → 0x60 after |0x20) not hex
  assert_eq!(skip::skip_hex_digits(b"`"), 0); // '`' (0x60) not hex
  assert_eq!(skip::skip_hex_digits(b"/"), 0); // '/' (0x2f) just below '0'
}

/// The case-fold trick (`byte | 0x20`) is what makes `[A-F]` map to `[a-f]`.
/// Make sure it doesn't accidentally accept the shifted forms of nearby
/// punctuation.
#[test]
fn skip_hex_digits_case_fold_boundary_chars() {
  // For every byte, decide if it should be hex-digit-able and check.
  for b in 0u8..=255u8 {
    let expected_hex = b.is_ascii_hexdigit();
    let input = [b];
    let result = skip::skip_hex_digits(&input);
    let want = if expected_hex { 1 } else { 0 };
    assert_eq!(
      result, want,
      "byte 0x{b:02x} ({:?}) — expected hex={expected_hex}",
      b as char
    );
  }
}

#[test]
fn skip_hex_digits_chunk_boundaries_exhaustive() {
  for len in 1usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[miss_pos] = b'g';
      assert_eq!(
        skip::skip_hex_digits(&input),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
    let input = vec![b'F'; len];
    assert_eq!(skip::skip_hex_digits(&input), len, "len={len}, all-match");
  }
}

/// Mixed case at every position to exercise the OR-with-0x20 inside the SIMD
/// hex-digit mask.
#[test]
fn skip_hex_digits_mixed_case_long() {
  let input = b"0123456789aBcDeFAbCdEf0123456789ABCDEFabcdef0123456789FAFaFAFa";
  assert_eq!(skip::skip_hex_digits(input), input.len());
}
