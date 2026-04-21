use memspan::skip;

#[test]
fn skip_while_handles_single_needle() {
  assert_eq!(skip::skip_while(b"aaaab", b'a'), 4);
  assert_eq!(skip::skip_while(b"baaaa", b'a'), 0);
  assert_eq!(skip::skip_while(b"aaaaa", b'a'), 5);
}

#[test]
fn skip_while_handles_fixed_needles_across_chunk_boundaries() {
  let needles = [b' ', b'\t', b'\r', b'\n', b','];

  for len in [
    1usize, 7, 15, 16, 17, 23, 31, 32, 33, 47, 48, 63, 64, 65, 127, 128, 129, 255, 256,
  ] {
    let mut input = vec![b' '; len];
    input[len - 1] = b'a';

    assert_eq!(skip::skip_while(&input, needles), len - 1, "len={len}");

    input[len - 1] = b'\n';
    assert_eq!(skip::skip_while(&input, needles), len, "len={len}");
  }
}

#[test]
fn skip_while_handles_dynamic_needles() {
  let needles: &[u8] = b"0123456789ABCDEF";

  for len in [1usize, 7, 15, 16, 17, 31, 32, 33, 64, 65, 128] {
    let mut input = vec![b'0'; len];
    input[len - 1] = b'x';

    assert_eq!(
      skip::skip_while(input.as_slice(), needles),
      len - 1,
      "len={len}"
    );

    input[len - 1] = b'F';
    assert_eq!(
      skip::skip_while(input.as_slice(), needles),
      len,
      "len={len}"
    );
  }
}

#[test]
fn skip_while_empty_needles_matches_empty_prefix() {
  assert_eq!(skip::skip_while(b"abcdef", []), 0);

  let empty: &[u8] = &[];
  assert_eq!(skip::skip_while(b"abcdef", empty), 0);
}

/// Exhaustive: every miss position in lengths spanning the probe boundary,
/// the SIMD loop, and the overlap-tail region.
#[test]
fn skip_while_simd_path_miss_inside_each_chunk() {
  let needles = [b' ', b'\t', b'\r', b'\n', b','];

  for len in 16usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![b' '; len];
      input[miss_pos] = b'a';
      assert_eq!(
        skip::skip_while(&input, needles),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
    // No miss anywhere -> full prefix.
    let input = vec![b' '; len];
    assert_eq!(
      skip::skip_while(&input, needles),
      len,
      "len={len}, all-match"
    );
  }
}

/// SIMD path with the dynamic-slice (>8) needle dispatch.
#[test]
fn skip_while_simd_path_dynamic_miss_inside_each_chunk() {
  let needles: &[u8] = b"0123456789ABCDEF";

  for len in [16usize, 17, 31, 32, 33, 47, 48, 63, 64, 65, 80] {
    for miss_pos in 0..len {
      let mut input = vec![b'A'; len];
      input[miss_pos] = b'z';
      assert_eq!(
        skip::skip_while(input.as_slice(), needles),
        miss_pos,
        "len={len}, miss_pos={miss_pos}"
      );
    }
  }
}
