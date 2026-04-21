use memspan::skip;

#[test]
fn skip_until_handles_chunk_boundaries_for_single_needle() {
  for len in [1usize, 7, 15, 16, 17] {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'X';

    assert_eq!(skip::skip_until(&input, b'X'), Some(len - 1), "len={len}");
    assert_eq!(skip::skip_until(&input, b'Z'), None, "len={len}");
  }
}

#[test]
fn skip_until_handles_chunk_boundaries_for_fixed_needles() {
  for len in [1usize, 7, 15, 16, 17] {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'Y';

    assert_eq!(
      skip::skip_until(&input, [b'X', b'Y']),
      Some(len - 1),
      "len={len}"
    );
    assert_eq!(skip::skip_until(&input, [b'X', b'Z']), None, "len={len}");
    assert_eq!(skip::skip_until(&input, b"XY"), Some(len - 1), "len={len}");
  }
}

#[test]
fn skip_until_handles_chunk_boundaries_for_dynamic_needles() {
  for len in [1usize, 7, 15, 16, 17] {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'3';

    let hit: &[u8] = b"123";
    let miss: &[u8] = b"124";

    assert_eq!(skip::skip_until(&input, hit), Some(len - 1), "len={len}");
    assert_eq!(skip::skip_until(&input, miss), None, "len={len}");
  }
}

#[test]
fn skip_until_returns_none_for_empty_needles() {
  let input = b"abcdef";

  assert_eq!(skip::skip_until(input, []), None);

  let empty: &[u8] = &[];
  assert_eq!(skip::skip_until(input, empty), None);
}

/// Exercises the NEON SIMD path (≥4 needles) across the chunk-size boundary
/// and the overlap-tail logic.
#[test]
fn skip_until_simd_path_boundary_lengths_5_needles() {
  let needles = [b'1', b'2', b'3', b'4', b'5'];
  let miss = [b'1', b'2', b'3', b'4', b'6'];

  for len in [
    1usize, 7, 15, 16, 17, 23, 31, 32, 33, 47, 48, 63, 64, 65, 127, 128, 129, 255, 256,
  ] {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'5';

    assert_eq!(
      skip::skip_until(&input, needles),
      Some(len - 1),
      "len={len}"
    );
    assert_eq!(skip::skip_until(&input, miss), None, "len={len}");
  }
}

/// SIMD path: match somewhere in the middle, including positions that fall in
/// the overlap-tail region.
#[test]
fn skip_until_simd_path_match_inside_each_chunk() {
  let needles = [b'!', b'@', b'#', b'$', b'%'];

  for len in 16usize..=80 {
    for hit_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[hit_pos] = b'#';
      assert_eq!(
        skip::skip_until(&input, needles),
        Some(hit_pos),
        "len={len}, hit_pos={hit_pos}"
      );
    }
  }
}

/// Slice-needle SIMD path (>8 needles → dynamic NEON loop).
#[test]
fn skip_until_simd_path_dynamic_needles_large() {
  let needles: &[u8] = b"0123456789ABCDEF";
  for len in [16usize, 17, 31, 32, 33, 64, 65, 128] {
    let mut input = vec![b'a'; len];
    input[len - 1] = b'F';
    assert_eq!(
      skip::skip_until(input.as_slice(), needles),
      Some(len - 1),
      "len={len}"
    );

    let miss: Vec<u8> = vec![b'a'; len];
    assert_eq!(
      skip::skip_until(miss.as_slice(), needles),
      None,
      "len={len}"
    );
  }
}
