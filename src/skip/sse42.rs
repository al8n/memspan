//! SSE4.2 (128-bit) implementations of `skip_until`, `skip_while`, and the
//! specialized ASCII-class scanners.
//!
//! All public functions in this module are `unsafe` because they require the
//! `sse4.2` target feature (which implies SSE2). The dispatcher in
//! `skip/mod.rs` gates every call behind a runtime `sse42_available()` check.
//!
//! **Position extraction.** Unlike the NEON path (which uses the
//! `vshrn`/nibble-mask trick), x86's `_mm_movemask_epi8` extracts one bit per
//! lane into a 16-bit integer, so the first matching lane is simply
//! `bits.trailing_zeros()` — no division by 4.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::Needles;

const CHUNK: usize = 16;

/// Tests whether each byte of `chunk` lies in `[lo, hi]` (inclusive, unsigned).
///
/// Algorithm (3 ops vs NEON's 2):
/// 1. `x = chunk - lo` (wrapping mod 256 — same bit pattern as unsigned wrap)
/// 2. `min(x, hi-lo)` clamps in-range values to themselves, out-of-range to `hi-lo`
/// 3. `cmpeq(x, min)` is 0xFF only where `x ≤ hi-lo`, i.e. byte ∈ [lo, hi]
#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn range_mask(chunk: __m128i, lo: u8, hi: u8) -> __m128i {
  let x = _mm_sub_epi8(chunk, _mm_set1_epi8(lo as i8));
  let limit = _mm_set1_epi8(hi.wrapping_sub(lo) as i8);
  _mm_cmpeq_epi8(x, _mm_min_epu8(x, limit))
}

// ── per-class mask functions ─────────────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn binary_mask(c: __m128i) -> __m128i {
  range_mask(c, b'0', b'1')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn octal_digit_mask(c: __m128i) -> __m128i {
  range_mask(c, b'0', b'7')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn digit_mask(c: __m128i) -> __m128i {
  range_mask(c, b'0', b'9')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn hex_digit_mask(c: __m128i) -> __m128i {
  let digit = digit_mask(c);
  let lower = _mm_or_si128(c, _mm_set1_epi8(0x20u8 as i8));
  let alpha = range_mask(lower, b'a', b'f');
  _mm_or_si128(digit, alpha)
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse2")]
unsafe fn whitespace_mask(c: __m128i) -> __m128i {
  let sp = _mm_cmpeq_epi8(c, _mm_set1_epi8(b' ' as i8));
  let tab = _mm_cmpeq_epi8(c, _mm_set1_epi8(b'\t' as i8));
  let nl = _mm_cmpeq_epi8(c, _mm_set1_epi8(b'\n' as i8));
  let cr = _mm_cmpeq_epi8(c, _mm_set1_epi8(b'\r' as i8));
  _mm_or_si128(_mm_or_si128(sp, tab), _mm_or_si128(nl, cr))
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn alpha_mask(c: __m128i) -> __m128i {
  let lower = _mm_or_si128(c, _mm_set1_epi8(0x20u8 as i8));
  range_mask(lower, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn alphanumeric_mask(c: __m128i) -> __m128i {
  _mm_or_si128(alpha_mask(c), digit_mask(c))
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn ident_start_mask(c: __m128i) -> __m128i {
  _mm_or_si128(alpha_mask(c), _mm_cmpeq_epi8(c, _mm_set1_epi8(b'_' as i8)))
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.1")]
unsafe fn ident_mask(c: __m128i) -> __m128i {
  _mm_or_si128(
    alphanumeric_mask(c),
    _mm_cmpeq_epi8(c, _mm_set1_epi8(b'_' as i8)),
  )
}

/// Extracts a 16-bit match bitmask (1 bit per lane, bit 0 = lane 0).
#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse2")]
unsafe fn movemask(m: __m128i) -> u32 {
  _mm_movemask_epi8(m) as u32
}

macro_rules! skip_ascii_class {
  ($name:ident, $prefix_len:ident, $mask:ident, $feature:literal) => {
    #[cfg_attr(not(tarpaulin), inline(always))]
    #[target_feature(enable = $feature)]
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

      // 2× unrolled: AND both match masks; all-ones ⟹ both chunks clean.
      while cur + 2 * CHUNK <= len {
        let c0 = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
        let c1 = _mm_loadu_si128(ptr.add(cur + CHUNK) as *const __m128i);
        let m0 = $mask(c0);
        let m1 = $mask(c1);
        let combined = movemask(_mm_and_si128(m0, m1));
        if combined != 0xFFFF {
          let b0 = movemask(m0);
          if b0 != 0xFFFF {
            return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
          }
          let b1 = movemask(m1);
          return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
        }
        cur += 2 * CHUNK;
      }

      while cur + CHUNK <= len {
        let chunk = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
        let bits = movemask($mask(chunk));
        if bits != 0xFFFF {
          return cur + ((!bits) & 0xFFFF).trailing_zeros() as usize;
        }
        cur += CHUNK;
      }

      if cur == len {
        return len;
      }

      let overlap_start = len - CHUNK;
      let chunk = _mm_loadu_si128(ptr.add(overlap_start) as *const __m128i);
      let bits = movemask($mask(chunk));
      let already = cur - overlap_start;
      let scan_mask = (!0u32) << already;
      let non_match = (!bits) & scan_mask & 0xFFFF;
      if non_match != 0 {
        overlap_start + non_match.trailing_zeros() as usize
      } else {
        len
      }
    }
  };
}

skip_ascii_class!(skip_binary, prefix_len_binary, binary_mask, "sse4.1");
skip_ascii_class!(
  skip_octal_digits,
  prefix_len_octal_digits,
  octal_digit_mask,
  "sse4.1"
);
skip_ascii_class!(skip_digits, prefix_len_digits, digit_mask, "sse4.1");
skip_ascii_class!(
  skip_hex_digits,
  prefix_len_hex_digits,
  hex_digit_mask,
  "sse4.1"
);
skip_ascii_class!(
  skip_whitespace,
  prefix_len_whitespace,
  whitespace_mask,
  "sse2"
);
skip_ascii_class!(skip_alpha, prefix_len_alpha, alpha_mask, "sse4.1");
skip_ascii_class!(
  skip_alphanumeric,
  prefix_len_alphanumeric,
  alphanumeric_mask,
  "sse4.1"
);
skip_ascii_class!(
  skip_ident_start,
  prefix_len_ident_start,
  ident_start_mask,
  "sse4.1"
);
skip_ascii_class!(skip_ident, prefix_len_ident, ident_mask, "sse4.1");

// ── generic skip_until / skip_while ─────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.2")]
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
    let c0 = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
    let c1 = _mm_loadu_si128(ptr.add(cur + CHUNK) as *const __m128i);
    let m0 = needles.eq_any_mask_sse2(c0);
    let m1 = needles.eq_any_mask_sse2(c1);
    let combined = movemask(_mm_or_si128(m0, m1));
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
    let chunk = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
    let bits = movemask(needles.eq_any_mask_sse2(chunk));
    if bits != 0 {
      return Some(cur + bits.trailing_zeros() as usize);
    }
    cur += CHUNK;
  }

  if cur == len {
    return None;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm_loadu_si128(ptr.add(overlap_start) as *const __m128i);
  let bits = movemask(needles.eq_any_mask_sse2(chunk));
  let already = cur - overlap_start;
  let scan_mask = (!0u32) << already;
  let hit_bits = bits & scan_mask & 0xFFFF;
  if hit_bits != 0 {
    Some(overlap_start + hit_bits.trailing_zeros() as usize)
  } else {
    None
  }
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[target_feature(enable = "sse4.2")]
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
    let c0 = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
    let c1 = _mm_loadu_si128(ptr.add(cur + CHUNK) as *const __m128i);
    let m0 = needles.eq_any_mask_sse2(c0);
    let m1 = needles.eq_any_mask_sse2(c1);
    let combined = movemask(_mm_and_si128(m0, m1));
    if combined != 0xFFFF {
      let b0 = movemask(m0);
      if b0 != 0xFFFF {
        return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
      }
      let b1 = movemask(m1);
      return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = _mm_loadu_si128(ptr.add(cur) as *const __m128i);
    let bits = movemask(needles.eq_any_mask_sse2(chunk));
    if bits != 0xFFFF {
      return cur + ((!bits) & 0xFFFF).trailing_zeros() as usize;
    }
    cur += CHUNK;
  }

  if cur == len {
    return len;
  }

  let overlap_start = len - CHUNK;
  let chunk = _mm_loadu_si128(ptr.add(overlap_start) as *const __m128i);
  let bits = movemask(needles.eq_any_mask_sse2(chunk));
  let already = cur - overlap_start;
  let scan_mask = (!0u32) << already;
  let non_match = (!bits) & scan_mask & 0xFFFF;
  if non_match != 0 {
    overlap_start + non_match.trailing_zeros() as usize
  } else {
    len
  }
}
