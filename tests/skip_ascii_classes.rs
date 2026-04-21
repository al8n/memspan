use memspan::skip;

fn assert_prefix_boundaries(scanner: fn(&[u8]) -> usize, fill: u8, miss: u8) {
  for len in 0usize..=96 {
    let input = vec![fill; len];
    assert_eq!(scanner(&input), len, "len={len}, all-match");

    for miss_pos in 0..len {
      let mut input = vec![fill; len];
      input[miss_pos] = miss;

      assert_eq!(scanner(&input), miss_pos, "len={len}, miss_pos={miss_pos}");
    }
  }
}

#[test]
fn skip_binary_handles_ascii_binary_prefix() {
  assert_eq!(skip::skip_binary(b""), 0);
  assert_eq!(skip::skip_binary(b"0101012"), 6);
  assert_eq!(skip::skip_binary(b"2"), 0);

  assert_prefix_boundaries(skip::skip_binary, b'1', b'2');
}

#[test]
fn skip_digits_handles_ascii_decimal_prefix() {
  assert_eq!(skip::skip_digits(b""), 0);
  assert_eq!(skip::skip_digits(b"0123456789abc"), 10);
  assert_eq!(skip::skip_digits(b"abc"), 0);

  assert_prefix_boundaries(skip::skip_digits, b'9', b'a');
}

#[test]
fn skip_hex_digits_handles_ascii_hex_prefix() {
  assert_eq!(skip::skip_hex_digits(b""), 0);
  assert_eq!(skip::skip_hex_digits(b"0123456789abcdefABCDEFg"), 22);
  assert_eq!(skip::skip_hex_digits(b"g"), 0);

  assert_prefix_boundaries(skip::skip_hex_digits, b'F', b'g');
}

#[test]
fn skip_octal_digits_handles_ascii_octal_prefix() {
  assert_eq!(skip::skip_octal_digits(b""), 0);
  assert_eq!(skip::skip_octal_digits(b"0123456789"), 8);
  assert_eq!(skip::skip_octal_digits(b"8"), 0);

  assert_prefix_boundaries(skip::skip_octal_digits, b'7', b'8');
}

#[test]
fn skip_lower_basic() {
  assert_eq!(skip::skip_lower(b""), 0);
  assert_eq!(skip::skip_lower(b"abcxyz"), 6);
  assert_eq!(skip::skip_lower(b"abcXYZ"), 3);
  assert_eq!(skip::skip_lower(b"Abc"), 0);
  assert_eq!(skip::skip_lower(b"a1b"), 1);
}

#[test]
fn skip_lower_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if b.is_ascii_lowercase() { 1 } else { 0 };
    assert_eq!(
      skip::skip_lower(&[b]),
      expected,
      "byte 0x{b:02x} ({:?})",
      b as char
    );
  }
}

#[test]
fn skip_lower_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_lower, b'a', b'A');
}

#[test]
fn skip_upper_basic() {
  assert_eq!(skip::skip_upper(b""), 0);
  assert_eq!(skip::skip_upper(b"ABCXYZ"), 6);
  assert_eq!(skip::skip_upper(b"ABCabc"), 3);
  assert_eq!(skip::skip_upper(b"aBC"), 0);
}

#[test]
fn skip_upper_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if b.is_ascii_uppercase() { 1 } else { 0 };
    assert_eq!(
      skip::skip_upper(&[b]),
      expected,
      "byte 0x{b:02x} ({:?})",
      b as char
    );
  }
}

#[test]
fn skip_upper_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_upper, b'Z', b'a');
}

#[test]
fn skip_ascii_basic() {
  assert_eq!(skip::skip_ascii(b""), 0);
  assert_eq!(skip::skip_ascii(b"hello\x7F"), 6);
  assert_eq!(skip::skip_ascii(b"\x80abc"), 0);
  assert_eq!(skip::skip_ascii(b"abc\x80"), 3);
}

#[test]
fn skip_ascii_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if b.is_ascii() { 1 } else { 0 };
    assert_eq!(skip::skip_ascii(&[b]), expected, "byte 0x{b:02x}");
  }
}

#[test]
fn skip_ascii_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_ascii, b'z', 0x80);
}

#[test]
fn skip_non_ascii_basic() {
  assert_eq!(skip::skip_non_ascii(b""), 0);
  assert_eq!(skip::skip_non_ascii(b"hello"), 0);
  assert_eq!(skip::skip_non_ascii(&[0x80u8, 0xC0, 0xFF, b'a']), 3);
  assert_eq!(skip::skip_non_ascii(&[0x80u8, 0xFF]), 2);
}

#[test]
fn skip_non_ascii_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if !b.is_ascii() { 1 } else { 0 };
    assert_eq!(skip::skip_non_ascii(&[b]), expected, "byte 0x{b:02x}");
  }
}

#[test]
fn skip_non_ascii_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_non_ascii, 0xC0, b'a');
}

#[test]
fn skip_ascii_graphic_basic() {
  assert_eq!(skip::skip_ascii_graphic(b""), 0);
  assert_eq!(skip::skip_ascii_graphic(b"hello!"), 6);
  assert_eq!(skip::skip_ascii_graphic(b"abc "), 3); // space (0x20) is not graphic
  assert_eq!(skip::skip_ascii_graphic(b"\x1fABC"), 0); // control char
  assert_eq!(skip::skip_ascii_graphic(b"~\x7f"), 1); // DEL (0x7F) is not graphic
}

#[test]
fn skip_ascii_graphic_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if b.is_ascii_graphic() { 1 } else { 0 };
    assert_eq!(
      skip::skip_ascii_graphic(&[b]),
      expected,
      "byte 0x{b:02x} ({:?})",
      b as char
    );
  }
}

#[test]
fn skip_ascii_graphic_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_ascii_graphic, b'!', b' ');
}

#[test]
fn skip_ascii_control_basic() {
  assert_eq!(skip::skip_ascii_control(b""), 0);
  assert_eq!(skip::skip_ascii_control(b"\x00\x1f\x7f"), 3);
  assert_eq!(skip::skip_ascii_control(b"\t\n\r"), 3); // tab, newline, CR are control
  assert_eq!(skip::skip_ascii_control(b" abc"), 0); // space (0x20) is not control
  assert_eq!(skip::skip_ascii_control(b"\x1fA"), 1);
}

#[test]
fn skip_ascii_control_every_byte() {
  for b in 0u8..=255u8 {
    let expected = if b.is_ascii_control() { 1 } else { 0 };
    assert_eq!(skip::skip_ascii_control(&[b]), expected, "byte 0x{b:02x}");
  }
}

#[test]
fn skip_ascii_control_chunk_boundaries() {
  assert_prefix_boundaries(skip::skip_ascii_control, b'\x01', b' ');
}

fn scalar_skip_until_newline(input: &[u8]) -> usize {
  input
    .iter()
    .position(|&b| b == b'\n')
    .unwrap_or(input.len())
}

#[test]
fn skip_until_newline_basic() {
  assert_eq!(skip::skip_until_newline(b""), 0);
  assert_eq!(skip::skip_until_newline(b"\n"), 0);
  assert_eq!(skip::skip_until_newline(b"hello\nworld"), 5);
  assert_eq!(skip::skip_until_newline(b"no newline here"), 15);
  assert_eq!(skip::skip_until_newline(b"abc\n\n"), 3);
}

#[test]
fn skip_until_newline_newline_each_position() {
  for len in 1usize..=80 {
    for nl_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[nl_pos] = b'\n';
      assert_eq!(
        skip::skip_until_newline(&input),
        nl_pos,
        "len={len}, nl_pos={nl_pos}"
      );
    }
    let no_nl = vec![b'a'; len];
    assert_eq!(
      skip::skip_until_newline(&no_nl),
      scalar_skip_until_newline(&no_nl),
      "len={len} (no newline)"
    );
  }
}

#[test]
fn skip_until_newline_chunk_boundaries() {
  for len in [15usize, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
    let no_nl = vec![b'x'; len];
    assert_eq!(
      skip::skip_until_newline(&no_nl),
      len,
      "len={len} (no newline)"
    );

    let mut with_nl = vec![b'x'; len];
    with_nl[len - 1] = b'\n';
    assert_eq!(
      skip::skip_until_newline(&with_nl),
      len - 1,
      "len={len} (newline at tail)"
    );
  }
}

#[test]
fn contains_any_basic() {
  assert!(!skip::contains_any(b"", b'a'));
  assert!(skip::contains_any(b"hello", b'e'));
  assert!(!skip::contains_any(b"hello", b'z'));
  assert!(skip::contains_any(b"abc", [b'x', b'b']));
  assert!(!skip::contains_any(b"abc", [b'x', b'y']));
}

#[test]
fn contains_any_empty_needles() {
  let empty: &[u8] = &[];
  assert!(!skip::contains_any(b"hello", empty));
}

#[test]
fn contains_any_hit_each_position() {
  for len in 1usize..=48 {
    for hit_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[hit_pos] = b'Z';
      assert!(
        skip::contains_any(&input, b'Z'),
        "len={len}, hit_pos={hit_pos}"
      );
      assert!(
        !skip::contains_any(&input, b'Y'),
        "len={len}, hit_pos={hit_pos}"
      );
    }
  }
}
