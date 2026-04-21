//! AVX2 (256-bit) implementations of `skip_until`, `skip_while`, and the
#![allow(unsafe_op_in_unsafe_fn)]
//! specialized ASCII-class scanners.
//!
//! Chunk size doubles to 32 bytes vs SSE4.2; the 2× unrolled main loop
//! therefore covers 64 bytes per iteration. `_mm256_movemask_epi8` returns
//! a 32-bit integer, so `trailing_zeros()` gives the position directly.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::Needles;

const CHUNK: usize = 32;

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn range_mask(chunk: __m256i, lo: u8, hi: u8) -> __m256i {
  let x = _mm256_sub_epi8(chunk, _mm256_set1_epi8(lo as i8));
  let limit = _mm256_set1_epi8(hi.wrapping_sub(lo) as i8);
  _mm256_cmpeq_epi8(x, _mm256_min_epu8(x, limit))
}

// ── per-class mask functions ─────────────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn binary_mask(c: __m256i) -> __m256i {
  range_mask(c, b'0', b'1')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn octal_digit_mask(c: __m256i) -> __m256i {
  range_mask(c, b'0', b'7')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn digit_mask(c: __m256i) -> __m256i {
  range_mask(c, b'0', b'9')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn hex_digit_mask(c: __m256i) -> __m256i {
  let digit = digit_mask(c);
  let lower = _mm256_or_si256(c, _mm256_set1_epi8(0x20u8 as i8));
  let alpha = range_mask(lower, b'a', b'f');
  _mm256_or_si256(digit, alpha)
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn whitespace_mask(c: __m256i) -> __m256i {
  let sp = _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b' ' as i8));
  let tab = _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b'\t' as i8));
  let nl = _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b'\n' as i8));
  let cr = _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b'\r' as i8));
  _mm256_or_si256(_mm256_or_si256(sp, tab), _mm256_or_si256(nl, cr))
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn alpha_mask(c: __m256i) -> __m256i {
  let lower = _mm256_or_si256(c, _mm256_set1_epi8(0x20u8 as i8));
  range_mask(lower, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn alphanumeric_mask(c: __m256i) -> __m256i {
  _mm256_or_si256(alpha_mask(c), digit_mask(c))
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn ident_start_mask(c: __m256i) -> __m256i {
  _mm256_or_si256(
    alpha_mask(c),
    _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b'_' as i8)),
  )
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn ident_mask(c: __m256i) -> __m256i {
  _mm256_or_si256(
    alphanumeric_mask(c),
    _mm256_cmpeq_epi8(c, _mm256_set1_epi8(b'_' as i8)),
  )
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn lower_mask(c: __m256i) -> __m256i {
  range_mask(c, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn upper_mask(c: __m256i) -> __m256i {
  range_mask(c, b'A', b'Z')
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn ascii_mask(c: __m256i) -> __m256i {
  range_mask(c, 0x00, 0x7F)
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn non_ascii_mask(c: __m256i) -> __m256i {
  range_mask(c, 0x80, 0xFF)
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn ascii_graphic_mask(c: __m256i) -> __m256i {
  range_mask(c, 0x21, 0x7E)
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn ascii_control_mask(c: __m256i) -> __m256i {
  let ctrl = range_mask(c, 0x00, 0x1F);
  let del = _mm256_cmpeq_epi8(c, _mm256_set1_epi8(0x7F_u8 as i8));
  _mm256_or_si256(ctrl, del)
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
unsafe fn movemask(m: __m256i) -> u32 {
  _mm256_movemask_epi8(m) as u32
}

// ── skip_ascii_class macro ───────────────────────────────────────────────────

macro_rules! skip_ascii_class {
  ($name:ident, $prefix_len:ident, $mask:ident) => {
    #[cfg_attr(not(tarpaulin), inline)]
    #[target_feature(enable = "avx2")]
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
        let c0 = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
        let c1 = _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i);
        let m0 = $mask(c0);
        let m1 = $mask(c1);
        let combined = movemask(_mm256_and_si256(m0, m1));
        if combined != !0u32 {
          let b0 = movemask(m0);
          if b0 != !0u32 {
            return cur + (!b0).trailing_zeros() as usize;
          }
          let b1 = movemask(m1);
          return cur + CHUNK + (!b1).trailing_zeros() as usize;
        }
        cur += 2 * CHUNK;
      }

      while cur + CHUNK <= len {
        let chunk = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
        let bits = movemask($mask(chunk));
        if bits != !0u32 {
          return cur + (!bits).trailing_zeros() as usize;
        }
        cur += CHUNK;
      }

      if cur == len {
        return len;
      }

      let overlap_start = len - CHUNK;
      let chunk = _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i);
      let bits = movemask($mask(chunk));
      let already = cur - overlap_start;
      let scan_mask = (!0u32) << already;
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

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
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
    let c0 = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let c1 = _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i);
    let m0 = movemask(needles.eq_any_mask_avx2(c0));
    let m1 = movemask(needles.eq_any_mask_avx2(c1));
    count += m0.count_ones() as usize;
    count += m1.count_ones() as usize;
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    count += bits.count_ones() as usize;
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    let already = cur - overlap_start;
    let scan_mask = (!0u32) << already;
    count += (bits & scan_mask).count_ones() as usize;
  }

  count
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
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
    let c0 = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let c1 = _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i);
    let b0 = movemask(needles.eq_any_mask_avx2(c0));
    let b1 = movemask(needles.eq_any_mask_avx2(c1));
    if b0 != 0 {
      last = Some(cur + (31 - b0.leading_zeros()) as usize);
    }
    if b1 != 0 {
      last = Some(cur + CHUNK + (31 - b1.leading_zeros()) as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    if bits != 0 {
      last = Some(cur + (31 - bits.leading_zeros()) as usize);
    }
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    let already = cur - overlap_start;
    let scan_mask = (!0u32) << already;
    let hit_bits = bits & scan_mask;
    if hit_bits != 0 {
      last = Some(overlap_start + (31 - hit_bits.leading_zeros()) as usize);
    }
  }

  last
}

// ── generic skip_until / skip_while ─────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
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
    let c0 = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let c1 = _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i);
    let m0 = needles.eq_any_mask_avx2(c0);
    let m1 = needles.eq_any_mask_avx2(c1);
    let combined = movemask(_mm256_or_si256(m0, m1));
    if combined != 0 {
      let b0 = movemask(m0);
      if b0 != 0 {
        return Some(cur + b0.trailing_zeros() as usize);
      }
      let b1 = movemask(m1);
      return Some(cur + CHUNK + b1.trailing_zeros() as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    if bits != 0 {
      return Some(cur + bits.trailing_zeros() as usize);
    }
    cur += CHUNK;
  }

  if cur == len {
    return None;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i);
  let bits = movemask(needles.eq_any_mask_avx2(chunk));
  let already = cur - overlap_start;
  let scan_mask = (!0u32) << already;
  let hit_bits = bits & scan_mask;
  if hit_bits != 0 {
    Some(overlap_start + hit_bits.trailing_zeros() as usize)
  } else {
    None
  }
}

#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
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
    let c0 = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let c1 = _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i);
    let m0 = needles.eq_any_mask_avx2(c0);
    let m1 = needles.eq_any_mask_avx2(c1);
    let combined = movemask(_mm256_and_si256(m0, m1));
    if combined != !0u32 {
      let b0 = movemask(m0);
      if b0 != !0u32 {
        return cur + (!b0).trailing_zeros() as usize;
      }
      let b1 = movemask(m1);
      return cur + CHUNK + (!b1).trailing_zeros() as usize;
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm256_loadu_si256(ptr.add(cur) as *const __m256i);
    let bits = movemask(needles.eq_any_mask_avx2(chunk));
    if bits != !0u32 {
      return cur + (!bits).trailing_zeros() as usize;
    }
    cur += CHUNK;
  }

  if cur == len {
    return len;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i);
  let bits = movemask(needles.eq_any_mask_avx2(chunk));
  let already = cur - overlap_start;
  let scan_mask = (!0u32) << already;
  let non_match = (!bits) & scan_mask;
  if non_match != 0 {
    overlap_start + non_match.trailing_zeros() as usize
  } else {
    len
  }
}
