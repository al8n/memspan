use skipchr::skip;

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
