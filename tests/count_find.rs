use memspan::skip;

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

// ── count_matches ─────────────────────────────────────────────────────────────

#[test]
fn count_matches_empty_input() {
  assert_eq!(skip::count_matches(b"", b'a'), 0);
  assert_eq!(skip::count_matches(b"", [b'a', b'b']), 0);
}

#[test]
fn count_matches_empty_needles() {
  let empty: &[u8] = &[];
  assert_eq!(skip::count_matches(b"hello world", empty), 0);
}

#[test]
fn count_matches_all_match() {
  for len in [1usize, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
    let input = vec![b'a'; len];
    assert_eq!(skip::count_matches(&input, b'a'), len, "len={len}");
  }
}

#[test]
fn count_matches_no_match() {
  for len in [1usize, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128] {
    let input = vec![b'a'; len];
    assert_eq!(skip::count_matches(&input, b'z'), 0, "len={len}");
  }
}

#[test]
fn count_matches_single_hit_each_position() {
  for len in 1usize..=80 {
    for hit_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[hit_pos] = b'X';
      assert_eq!(
        skip::count_matches(&input, b'X'),
        1,
        "len={len}, hit_pos={hit_pos}"
      );
    }
  }
}

#[test]
fn count_matches_cross_check_scalar_single_needle() {
  let needle = b'e';
  for len in [0usize, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
    let input: Vec<u8> = (0..len)
      .map(|i| if i % 3 == 0 { b'e' } else { b'x' })
      .collect();
    let expected = scalar_count(&input, &[needle]);
    assert_eq!(skip::count_matches(&input, needle), expected, "len={len}");
  }
}

#[test]
fn count_matches_cross_check_scalar_multi_needle() {
  let needles: &[u8] = b"aeiou";
  for len in [
    0usize, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256,
  ] {
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
    let expected = scalar_count(&input, needles);
    assert_eq!(skip::count_matches(&input, needles), expected, "len={len}");
  }
}

#[test]
fn count_matches_duplicate_needles() {
  let input = b"aabbccaabb";
  assert_eq!(skip::count_matches(input, [b'a', b'a']), 4);
  assert_eq!(skip::count_matches(input, b'a'), 4);
}

#[test]
fn count_matches_chunk_boundary_all_hits_last() {
  for len in [15usize, 16, 17, 31, 32, 33, 63, 64, 65] {
    let mut input = vec![b'z'; len];
    input[len - 1] = b'X';
    assert_eq!(
      skip::count_matches(&input, b'X'),
      1,
      "len={len} (hit at tail)"
    );
    input[0] = b'X';
    assert_eq!(
      skip::count_matches(&input, b'X'),
      2,
      "len={len} (hit at head+tail)"
    );
  }
}

// ── find_last ─────────────────────────────────────────────────────────────────

#[test]
fn find_last_empty_input() {
  assert_eq!(skip::find_last(b"", b'a'), None);
  assert_eq!(skip::find_last(b"", [b'a', b'b']), None);
}

#[test]
fn find_last_empty_needles() {
  let empty: &[u8] = &[];
  assert_eq!(skip::find_last(b"hello world", empty), None);
}

#[test]
fn find_last_no_match() {
  for len in [1usize, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128] {
    let input = vec![b'a'; len];
    assert_eq!(skip::find_last(&input, b'z'), None, "len={len}");
  }
}

#[test]
fn find_last_single_match_each_position() {
  for len in 1usize..=80 {
    for hit_pos in 0..len {
      let mut input = vec![b'a'; len];
      input[hit_pos] = b'X';
      assert_eq!(
        skip::find_last(&input, b'X'),
        Some(hit_pos),
        "len={len}, hit_pos={hit_pos}"
      );
    }
  }
}

#[test]
fn find_last_returns_rightmost() {
  for len in [16usize, 32, 64, 128] {
    for last_pos in [0usize, 1, 7, len / 2, len - 2, len - 1] {
      let mut input = vec![b'a'; len];
      if last_pos >= 1 {
        input[0] = b'X';
      }
      input[last_pos] = b'X';
      assert_eq!(
        skip::find_last(&input, b'X'),
        Some(last_pos),
        "len={len}, last_pos={last_pos}"
      );
    }
  }
}

#[test]
fn find_last_cross_check_scalar_single_needle() {
  let needle = b'e';
  for len in [0usize, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
    let input: Vec<u8> = (0..len)
      .map(|i| if i % 4 == 0 { b'e' } else { b'x' })
      .collect();
    let expected = scalar_find_last(&input, &[needle]);
    assert_eq!(skip::find_last(&input, needle), expected, "len={len}");
  }
}

#[test]
fn find_last_cross_check_scalar_multi_needle() {
  let needles: &[u8] = b"aeiou";
  for len in [
    0usize, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256,
  ] {
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
    let expected = scalar_find_last(&input, needles);
    assert_eq!(skip::find_last(&input, needles), expected, "len={len}");
  }
}

#[test]
fn find_last_all_match() {
  for len in [1usize, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128] {
    let input = vec![b'a'; len];
    assert_eq!(skip::find_last(&input, b'a'), Some(len - 1), "len={len}");
  }
}

#[test]
fn find_last_chunk_boundary_hit_at_tail() {
  for len in [15usize, 16, 17, 31, 32, 33, 63, 64, 65] {
    let mut input = vec![b'z'; len];
    input[len - 1] = b'X';
    assert_eq!(skip::find_last(&input, b'X'), Some(len - 1), "len={len}");
  }
}
