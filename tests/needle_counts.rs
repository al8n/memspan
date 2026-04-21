// Exercises every needle-count arm in the Needles trait impls for both
// fixed-size arrays ([u8; N]) and runtime slices (&[u8]).
//
// On aarch64 the NEON path is triggered for skip_until when len >= 16 and
// for skip_while when count > 1 and len >= 32.  Short inputs (< 16) exercise
// the scalar tail_find / prefix_len arms.

#![allow(warnings)]

use memspan::skip;

// ── helpers ──────────────────────────────────────────────────────────────────

fn build(len: usize, hit_byte: u8) -> Vec<u8> {
  let mut v = vec![b'z'; len];
  v[len - 1] = hit_byte;
  v
}

// ── [u8; N] fixed-array needle tests ─────────────────────────────────────────
// Each call with len >= 16 reaches the NEON path and exercises
// [u8; N]::eq_any_mask_neon → aarch64::eq_any_mask_const(N).
// Each call with len < 16 reaches [u8; N]::tail_find / prefix_len.

#[test]
fn fixed_array_n1_skip_until() {
  let n = [b'X'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'X');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

#[test]
fn fixed_array_n3_skip_until() {
  let n = [b'X', b'Y', b'Z'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'Z');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

#[test]
fn fixed_array_n4_skip_until() {
  let n = [b'A', b'B', b'C', b'D'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'D');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

#[test]
fn fixed_array_n6_skip_until() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'F');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

#[test]
fn fixed_array_n7_skip_until() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F', b'G'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'G');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

#[test]
fn fixed_array_n8_skip_until() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H'];
  for len in [1usize, 7, 15, 16, 17, 32, 64, 128] {
    let input = build(len, b'H');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
}

// prefix_len arms for [u8; N] — skip_while with count > 1 and any len
// (scalar for len < 32, NEON for len >= 32).
#[test]
fn fixed_array_prefix_len_n1() {
  let n = [b'A'];
  assert_eq!(skip::skip_while(b"AAAB", n), 3);
  assert_eq!(skip::skip_while(b"BAAA", n), 0);
}

#[test]
fn fixed_array_prefix_len_n3() {
  let n = [b'A', b'B', b'C'];
  assert_eq!(skip::skip_while(b"ABCABCX", n), 6);
  assert_eq!(skip::skip_while(b"XABC", n), 0);
}

#[test]
fn fixed_array_prefix_len_n4() {
  let n = [b'A', b'B', b'C', b'D'];
  assert_eq!(skip::skip_while(b"ABCDABCDX", n), 8);
}

#[test]
fn fixed_array_prefix_len_n6() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F'];
  assert_eq!(skip::skip_while(b"ABCDEFX", n), 6);
}

#[test]
fn fixed_array_prefix_len_n7() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F', b'G'];
  assert_eq!(skip::skip_while(b"ABCDEFGX", n), 7);
}

#[test]
fn fixed_array_prefix_len_n8() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H'];
  assert_eq!(skip::skip_while(b"ABCDEFGHX", n), 8);
}

// ── &[u8] slice needle tests ──────────────────────────────────────────────────
// Long inputs (>= 16) reach [u8]::eq_any_mask_neon; short inputs reach
// [u8]::tail_find / prefix_len.  Both paths have per-arm matches in needles.rs.

#[test]
fn slice_n1_skip_until() {
  let n: &[u8] = b"X";
  // short (scalar tail_find [a] arm)
  assert_eq!(skip::skip_until(b"zzX", n), Some(2));
  assert_eq!(skip::skip_until(b"zzz", n), None);
  // long (NEON eq_any_mask_neon [a] arm)
  for len in [16usize, 32, 64] {
    let input = build(len, b'X');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

#[test]
fn slice_n3_skip_until() {
  let n: &[u8] = b"XYZ";
  assert_eq!(skip::skip_until(b"zzZ", n), Some(2));
  assert_eq!(skip::skip_until(b"zzz", n), None);
  for len in [16usize, 32, 64] {
    let input = build(len, b'Z');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

#[test]
fn slice_n4_skip_until() {
  let n: &[u8] = b"ABCD";
  assert_eq!(skip::skip_until(b"zzD", n), Some(2));
  for len in [16usize, 32, 64] {
    let input = build(len, b'D');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

#[test]
fn slice_n6_skip_until() {
  let n: &[u8] = b"ABCDEF";
  // short → [u8]::tail_find [a,b,c,d,e,f] arm
  assert_eq!(skip::skip_until(b"zzF", n), Some(2));
  assert_eq!(skip::skip_until(b"zzz", n), None);
  // long → [u8]::eq_any_mask_neon [a,b,c,d,e,f] arm
  for len in [16usize, 32, 64] {
    let input = build(len, b'F');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

#[test]
fn slice_n7_skip_until() {
  let n: &[u8] = b"ABCDEFG";
  assert_eq!(skip::skip_until(b"zzG", n), Some(2));
  assert_eq!(skip::skip_until(b"zzz", n), None);
  for len in [16usize, 32, 64] {
    let input = build(len, b'G');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

#[test]
fn slice_n8_skip_until() {
  let n: &[u8] = b"ABCDEFGH";
  assert_eq!(skip::skip_until(b"zzH", n), Some(2));
  assert_eq!(skip::skip_until(b"zzz", n), None);
  for len in [16usize, 32, 64] {
    let input = build(len, b'H');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
  }
}

// prefix_len arms for &[u8] — via skip_while with short inputs (scalar path)
#[test]
fn slice_prefix_len_n1() {
  let n: &[u8] = b"A";
  assert_eq!(skip::skip_while(b"AAAB", n), 3);
  assert_eq!(skip::skip_while(b"BAAA", n), 0);
}

#[test]
fn slice_prefix_len_n3() {
  let n: &[u8] = b"ABC";
  assert_eq!(skip::skip_while(b"ABCABCX", n), 6);
}

#[test]
fn slice_prefix_len_n4() {
  let n: &[u8] = b"ABCD";
  assert_eq!(skip::skip_while(b"ABCDX", n), 4);
}

#[test]
fn slice_prefix_len_n6() {
  let n: &[u8] = b"ABCDEF";
  assert_eq!(skip::skip_while(b"ABCDEFX", n), 6);
}

#[test]
fn slice_prefix_len_n7() {
  let n: &[u8] = b"ABCDEFG";
  assert_eq!(skip::skip_while(b"ABCDEFGX", n), 7);
}

#[test]
fn slice_prefix_len_n8() {
  let n: &[u8] = b"ABCDEFGH";
  assert_eq!(skip::skip_while(b"ABCDEFGHX", n), 8);
}

// ── &T delegation (Needles for &T) ────────────────────────────────────────────
// Passing &&[u8] / &&u8 routes through the blanket &T impl.
#[test]
fn ref_delegation_skip_until() {
  let n: &[u8] = b"XY";
  let rn: &&[u8] = &&n;
  for len in [1usize, 16, 32] {
    let input = build(len, b'Y');
    assert_eq!(skip::skip_until(&input, rn), Some(len - 1), "len={len}");
  }
}

#[test]
fn ref_delegation_skip_while() {
  let n: &[u8] = b"AB";
  let rn: &&[u8] = &&n;
  assert_eq!(skip::skip_while(b"ABABX", rn), 4);
}

#[test]
fn ref_delegation_count_find() {
  let n: &[u8] = b"AB";
  let rn: &&[u8] = &&n;
  assert_eq!(skip::count_matches(b"ABCABC", rn), 4);
  assert_eq!(skip::find_last(b"ABCABC", rn), Some(4));
}

// ── N=0 via count_matches (no needle-count guard in the aarch64 dispatcher) ──
// This reaches eq_any_mask_neon with [u8; 0], covering the N=0 arm in
// aarch64::eq_any_mask_const that is otherwise unreachable via skip_until.
#[test]
fn count_matches_zero_needle_array() {
  let n: [u8; 0] = [];
  let input: Vec<u8> = vec![b'a'; 32];
  assert_eq!(skip::count_matches(&input, n), 0);
  assert_eq!(skip::find_last(&input, n), None);
}

// ── N>8 fixed array: covers the `_ =>` arm in eq_any_mask_const ─────────────
// Also covers the _ arms in [u8; N]::tail_find and prefix_len.
#[test]
fn fixed_array_n9_skip_until() {
  let n = [b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I'];
  for len in [1usize, 7, 15, 16, 17, 32, 64] {
    let input = build(len, b'I');
    assert_eq!(skip::skip_until(&input, n), Some(len - 1), "len={len}");
    let miss = vec![b'z'; len];
    assert_eq!(skip::skip_until(&miss, n), None, "len={len}");
  }
  assert_eq!(skip::skip_while(b"ABCDEFGHIX", n), 9);
}

// ── count_matches and find_last with various needle counts ────────────────────
#[test]
fn count_find_fixed_array_n3() {
  let n = [b'a', b'e', b'i'];
  let input: Vec<u8> = vec![b'a'; 32];
  assert_eq!(skip::count_matches(&input, n), 32);
  assert_eq!(skip::find_last(&input, n), Some(31));
}

#[test]
fn count_find_fixed_array_n6() {
  let n = [b'a', b'b', b'c', b'd', b'e', b'f'];
  let input: Vec<u8> = b"abcdef".iter().cycle().copied().take(32).collect();
  assert_eq!(skip::count_matches(&input, n), 32);
  assert_eq!(skip::find_last(&input, n), Some(31));
}

#[test]
fn count_find_fixed_array_n8() {
  let n = [b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8'];
  let input: Vec<u8> = b"12345678".iter().cycle().copied().take(64).collect();
  assert_eq!(skip::count_matches(&input, n), 64);
  assert_eq!(skip::find_last(&input, n), Some(63));
}
