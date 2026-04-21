//! AVX-512BW (512-bit) implementations of `skip_until`, `skip_while`, and the
//! specialized ASCII-class scanners.
//!
//! AVX-512BW is special: comparison intrinsics like `_mm512_cmpeq_epi8_mask`
//! and `_mm512_cmple_epu8_mask` return a `__mmask64` (u64) **directly** — no
//! `movemask` conversion. Position extraction is therefore simply
//! `bits.trailing_zeros()`.
//!
//! Chunk size is 64 bytes; the 2× unrolled loop covers 128 bytes per iteration.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::Needles;

const CHUNK: usize = 64;

/// Range check [lo, hi] for AVX-512BW (2 ops, same cost as NEON).
///
/// `_mm512_sub_epi8` does wrapping u8 subtraction (identical bit pattern to
/// unsigned). `_mm512_cmple_epu8_mask` then does an unsigned ≤ comparison and
/// returns a `u64` mask directly — no further conversion needed.
#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
pub(crate) unsafe fn range_mask(chunk: __m512i, lo: u8, hi: u8) -> u64 {
  let x = _mm512_sub_epi8(chunk, _mm512_set1_epi8(lo as i8));
  _mm512_cmple_epu8_mask(x, _mm512_set1_epi8(hi.wrapping_sub(lo) as i8))
}

// ── per-class mask functions (return u64) ────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn binary_mask(c: __m512i) -> u64 {
  range_mask(c, b'0', b'1')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn octal_digit_mask(c: __m512i) -> u64 {
  range_mask(c, b'0', b'7')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn digit_mask(c: __m512i) -> u64 {
  range_mask(c, b'0', b'9')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn hex_digit_mask(c: __m512i) -> u64 {
  let digit = digit_mask(c);
  let lower = _mm512_or_si512(c, _mm512_set1_epi8(0x20u8 as i8));
  let alpha = range_mask(lower, b'a', b'f');
  digit | alpha
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn whitespace_mask(c: __m512i) -> u64 {
  let sp = _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b' ' as i8));
  let tab = _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b'\t' as i8));
  let nl = _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b'\n' as i8));
  let cr = _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b'\r' as i8));
  sp | tab | nl | cr
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn alpha_mask(c: __m512i) -> u64 {
  let lower = _mm512_or_si512(c, _mm512_set1_epi8(0x20u8 as i8));
  range_mask(lower, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn alphanumeric_mask(c: __m512i) -> u64 {
  alpha_mask(c) | digit_mask(c)
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn ident_start_mask(c: __m512i) -> u64 {
  alpha_mask(c) | _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b'_' as i8))
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn ident_mask(c: __m512i) -> u64 {
  alphanumeric_mask(c) | _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(b'_' as i8))
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn lower_mask(c: __m512i) -> u64 {
  range_mask(c, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn upper_mask(c: __m512i) -> u64 {
  range_mask(c, b'A', b'Z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn ascii_mask(c: __m512i) -> u64 {
  range_mask(c, 0x00, 0x7F)
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn non_ascii_mask(c: __m512i) -> u64 {
  range_mask(c, 0x80, 0xFF)
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn ascii_graphic_mask(c: __m512i) -> u64 {
  range_mask(c, 0x21, 0x7E)
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
unsafe fn ascii_control_mask(c: __m512i) -> u64 {
  let ctrl = range_mask(c, 0x00, 0x1F);
  let del = _mm512_cmpeq_epi8_mask(c, _mm512_set1_epi8(0x7F_u8 as i8));
  ctrl | del
}

// ── skip_ascii_class macro ───────────────────────────────────────────────────

macro_rules! skip_ascii_class {
  ($name:ident, $prefix_len:ident, $mask:ident) => {
    #[cfg_attr(not(tarpaulin), inline(always))]
    #[target_feature(enable = "avx512bw")]
    pub(super) unsafe fn $name(input: &[u8]) -> usize {
      let len = input.len();
      if len < CHUNK {
        return super::$prefix_len(input);
      }

      let ptr = input.as_ptr();

      let first = super::$prefix_len(&input[..CHUNK]);
      if first != CHUNK {
        return first;
      }

      let mut cur = CHUNK;

      while cur + 2 * CHUNK <= len {
        let c0 = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
        let c1 = _mm512_loadu_si512(ptr.add(cur + CHUNK).cast::<i32>());
        let m0 = $mask(c0);
        let m1 = $mask(c1);
        // All-ones means all match; any zero means a non-match in m0|m1 position.
        // For skip_while: miss iff NOT all-ones. Use AND: zero bit = non-match.
        let combined = m0 & m1;
        if combined != !0u64 {
          if m0 != !0u64 {
            return cur + (!m0).trailing_zeros() as usize;
          }
          return cur + CHUNK + (!m1).trailing_zeros() as usize;
        }
        cur += 2 * CHUNK;
      }

      while cur + CHUNK <= len {
        let chunk = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
        let bits = $mask(chunk);
        if bits != !0u64 {
          return cur + (!bits).trailing_zeros() as usize;
        }
        cur += CHUNK;
      }

      if cur == len {
        return len;
      }

      let overlap_start = len - CHUNK;
      let chunk = _mm512_loadu_si512(ptr.add(overlap_start).cast::<i32>());
      let bits = $mask(chunk);
      let already = cur - overlap_start;
      let scan_mask = (!0u64) << already;
      let non_match = (!bits) & scan_mask;
      if non_match != 0 {
        overlap_start + non_match.trailing_zeros() as usize
      } else {
        len
      }
    }
  };
}

skip_ascii_class!(skip_binary, prefix_len_binary, binary_mask);
skip_ascii_class!(skip_octal_digits, prefix_len_octal_digits, octal_digit_mask);
skip_ascii_class!(skip_digits, prefix_len_digits, digit_mask);
skip_ascii_class!(skip_hex_digits, prefix_len_hex_digits, hex_digit_mask);
skip_ascii_class!(skip_whitespace, prefix_len_whitespace, whitespace_mask);
skip_ascii_class!(skip_alpha, prefix_len_alpha, alpha_mask);
skip_ascii_class!(
  skip_alphanumeric,
  prefix_len_alphanumeric,
  alphanumeric_mask
);
skip_ascii_class!(skip_ident_start, prefix_len_ident_start, ident_start_mask);
skip_ascii_class!(skip_ident, prefix_len_ident, ident_mask);
skip_ascii_class!(skip_lower, prefix_len_lower, lower_mask);
skip_ascii_class!(skip_upper, prefix_len_upper, upper_mask);
skip_ascii_class!(skip_ascii, prefix_len_ascii, ascii_mask);
skip_ascii_class!(skip_non_ascii, prefix_len_non_ascii, non_ascii_mask);
skip_ascii_class!(
  skip_ascii_graphic,
  prefix_len_ascii_graphic,
  ascii_graphic_mask
);
skip_ascii_class!(
  skip_ascii_control,
  prefix_len_ascii_control,
  ascii_control_mask
);

// ── count_matches / find_last ────────────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
pub(super) unsafe fn count_matches<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  let len = input.len();
  if len < CHUNK {
    return input
      .iter()
      .filter(|&&b| needles.tail_find(core::slice::from_ref(&b)).is_some())
      .count();
  }

  let ptr = input.as_ptr();
  let mut count = 0usize;
  let mut cur = 0;

  while cur + 2 * CHUNK <= len {
    let c0 = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let c1 = _mm512_loadu_si512(ptr.add(cur + CHUNK).cast::<i32>());
    let m0 = needles.eq_any_mask_avx512(c0);
    let m1 = needles.eq_any_mask_avx512(c1);
    count += m0.count_ones() as usize;
    count += m1.count_ones() as usize;
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    count += bits.count_ones() as usize;
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = _mm512_loadu_si512(ptr.add(overlap_start).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    let already = cur - overlap_start;
    let scan_mask = (!0u64) << already;
    count += (bits & scan_mask).count_ones() as usize;
  }

  count
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
pub(super) unsafe fn find_last<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  let len = input.len();
  if len < CHUNK {
    let mut last = None;
    for (i, &b) in input.iter().enumerate() {
      if needles.tail_find(core::slice::from_ref(&b)).is_some() {
        last = Some(i);
      }
    }
    return last;
  }

  let ptr = input.as_ptr();
  let mut last: Option<usize> = None;
  let mut cur = 0;

  while cur + 2 * CHUNK <= len {
    let c0 = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let c1 = _mm512_loadu_si512(ptr.add(cur + CHUNK).cast::<i32>());
    let b0 = needles.eq_any_mask_avx512(c0);
    let b1 = needles.eq_any_mask_avx512(c1);
    if b0 != 0 {
      last = Some(cur + (63 - b0.leading_zeros()) as usize);
    }
    if b1 != 0 {
      last = Some(cur + CHUNK + (63 - b1.leading_zeros()) as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    if bits != 0 {
      last = Some(cur + (63 - bits.leading_zeros()) as usize);
    }
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = _mm512_loadu_si512(ptr.add(overlap_start).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    let already = cur - overlap_start;
    let scan_mask = (!0u64) << already;
    let hit_bits = bits & scan_mask;
    if hit_bits != 0 {
      last = Some(overlap_start + (63 - hit_bits.leading_zeros()) as usize);
    }
  }

  last
}

// ── generic skip_until / skip_while ─────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
pub(super) unsafe fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  let len = input.len();
  if len < CHUNK {
    return needles.tail_find(input);
  }

  let ptr = input.as_ptr();

  if let Some(hit) = needles.tail_find(&input[..CHUNK]) {
    return Some(hit);
  }

  let mut cur = CHUNK;

  while cur + 2 * CHUNK <= len {
    let c0 = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let c1 = _mm512_loadu_si512(ptr.add(cur + CHUNK).cast::<i32>());
    let m0 = needles.eq_any_mask_avx512(c0);
    let m1 = needles.eq_any_mask_avx512(c1);
    let combined = m0 | m1;
    if combined != 0 {
      if m0 != 0 {
        return Some(cur + m0.trailing_zeros() as usize);
      }
      return Some(cur + CHUNK + m1.trailing_zeros() as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    if bits != 0 {
      return Some(cur + bits.trailing_zeros() as usize);
    }
    cur += CHUNK;
  }

  if cur == len {
    return None;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm512_loadu_si512(ptr.add(overlap_start).cast::<i32>());
  let bits = needles.eq_any_mask_avx512(chunk);
  let already = cur - overlap_start;
  let scan_mask = (!0u64) << already;
  let hit_bits = bits & scan_mask;
  if hit_bits != 0 {
    Some(overlap_start + hit_bits.trailing_zeros() as usize)
  } else {
    None
  }
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "avx512bw")]
pub(super) unsafe fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  let len = input.len();
  if len < CHUNK {
    return needles.prefix_len(input);
  }

  let ptr = input.as_ptr();

  let first = needles.prefix_len(&input[..CHUNK]);
  if first != CHUNK {
    return first;
  }

  let mut cur = CHUNK;

  while cur + 2 * CHUNK <= len {
    let c0 = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let c1 = _mm512_loadu_si512(ptr.add(cur + CHUNK).cast::<i32>());
    let m0 = needles.eq_any_mask_avx512(c0);
    let m1 = needles.eq_any_mask_avx512(c1);
    let combined = m0 & m1;
    if combined != !0u64 {
      if m0 != !0u64 {
        return cur + (!m0).trailing_zeros() as usize;
      }
      return cur + CHUNK + (!m1).trailing_zeros() as usize;
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm512_loadu_si512(ptr.add(cur).cast::<i32>());
    let bits = needles.eq_any_mask_avx512(chunk);
    if bits != !0u64 {
      return cur + (!bits).trailing_zeros() as usize;
    }
    cur += CHUNK;
  }

  if cur == len {
    return len;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm512_loadu_si512(ptr.add(overlap_start).cast::<i32>());
  let bits = needles.eq_any_mask_avx512(chunk);
  let already = cur - overlap_start;
  let scan_mask = (!0u64) << already;
  let non_match = (!bits) & scan_mask;
  if non_match != 0 {
    overlap_start + non_match.trailing_zeros() as usize
  } else {
    len
  }
}
