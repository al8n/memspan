//! WebAssembly SIMD128 (128-bit) implementations of `skip_until`, `skip_while`,
//! and the specialized ASCII-class scanners.
//!
//! WASM SIMD128 is structurally similar to SSE2: 16-byte chunks, per-lane
//! 0xFF/0x00 match masks, and `i8x16_bitmask` for position extraction (returns
//! a u32 with one bit per lane, matching `_mm_movemask_epi8`). WASM SIMD
//! intrinsics are **safe** (no `unsafe` blocks required).
//!
//! The range check uses `i8x16_sub` (wrapping) + `u8x16_lt` (unsigned <),
//! matching NEON's 2-op cost.

use core::arch::wasm32::*;

use crate::Needles;

const CHUNK: usize = 16;

/// Tests whether each byte of `chunk` lies in `[lo, hi]` (2 ops, like NEON).
///
/// The full-range case `lo = 0x00, hi = 0xFF` is special-cased: the bound
/// would wrap to 0 and `u8x16_lt(x, 0)` would always be false.
#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn range_mask(chunk: v128, lo: u8, hi: u8) -> v128 {
  let width = hi.wrapping_sub(lo);
  if width == 0xFF {
    return u8x16_splat(0xFF);
  }
  let x = i8x16_sub(chunk, i8x16_splat(lo as i8));
  u8x16_lt(x, u8x16_splat(width.wrapping_add(1)))
}

// ── per-class mask functions ─────────────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
fn binary_mask(c: v128) -> v128 {
  range_mask(c, b'0', b'1')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn octal_digit_mask(c: v128) -> v128 {
  range_mask(c, b'0', b'7')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn digit_mask(c: v128) -> v128 {
  range_mask(c, b'0', b'9')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn hex_digit_mask(c: v128) -> v128 {
  let digit = digit_mask(c);
  let lower = v128_or(c, u8x16_splat(0x20));
  let alpha = range_mask(lower, b'a', b'f');
  v128_or(digit, alpha)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn whitespace_mask(c: v128) -> v128 {
  let sp = i8x16_eq(c, i8x16_splat(b' ' as i8));
  let tab = i8x16_eq(c, i8x16_splat(b'\t' as i8));
  let nl = i8x16_eq(c, i8x16_splat(b'\n' as i8));
  let cr = i8x16_eq(c, i8x16_splat(b'\r' as i8));
  v128_or(v128_or(sp, tab), v128_or(nl, cr))
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn alpha_mask(c: v128) -> v128 {
  let lower = v128_or(c, u8x16_splat(0x20));
  range_mask(lower, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn alphanumeric_mask(c: v128) -> v128 {
  v128_or(alpha_mask(c), digit_mask(c))
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ident_start_mask(c: v128) -> v128 {
  v128_or(alpha_mask(c), i8x16_eq(c, i8x16_splat(b'_' as i8)))
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ident_mask(c: v128) -> v128 {
  v128_or(alphanumeric_mask(c), i8x16_eq(c, i8x16_splat(b'_' as i8)))
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn lower_mask(c: v128) -> v128 {
  range_mask(c, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn upper_mask(c: v128) -> v128 {
  range_mask(c, b'A', b'Z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_mask(c: v128) -> v128 {
  range_mask(c, 0x00, 0x7F)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn non_ascii_mask(c: v128) -> v128 {
  range_mask(c, 0x80, 0xFF)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_graphic_mask(c: v128) -> v128 {
  range_mask(c, 0x21, 0x7E)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_control_mask(c: v128) -> v128 {
  let ctrl = range_mask(c, 0x00, 0x1F);
  let del = i8x16_eq(c, i8x16_splat(0x7F_u8 as i8));
  v128_or(ctrl, del)
}

/// Extracts a 16-bit bitmask (1 bit per lane) from a 0xFF/0x00 mask vector.
#[cfg_attr(not(tarpaulin), inline(always))]
fn bitmask(m: v128) -> u32 {
  i8x16_bitmask(m) as u32
}

// ── skip_ascii_class macro ───────────────────────────────────────────────────

macro_rules! skip_ascii_class {
  ($name:ident, $prefix_len:ident, $mask:ident) => {
    #[cfg_attr(not(tarpaulin), inline(always))]
    pub(super) fn $name(input: &[u8]) -> usize {
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
        let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
        let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
        let m0 = $mask(c0);
        let m1 = $mask(c1);
        let combined = bitmask(v128_and(m0, m1));
        if combined != 0xFFFF {
          let b0 = bitmask(m0);
          if b0 != 0xFFFF {
            return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
          }
          let b1 = bitmask(m1);
          return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
        }
        cur += 2 * CHUNK;
      }

      while cur + CHUNK <= len {
        let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
        let bits = bitmask($mask(chunk));
        if bits != 0xFFFF {
          return cur + ((!bits) & 0xFFFF).trailing_zeros() as usize;
        }
        cur += CHUNK;
      }

      if cur == len {
        return len;
      }

      let overlap_start = len - CHUNK;
      let chunk = unsafe { v128_load(ptr.add(overlap_start) as *const v128) };
      let bits = bitmask($mask(chunk));
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
pub(super) fn count_matches<Nd>(input: &[u8], needles: Nd) -> usize
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
    let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
    let m0 = bitmask(needles.eq_any_mask_simd128(c0));
    let m1 = bitmask(needles.eq_any_mask_simd128(c1));
    count += (m0 & 0xFFFF).count_ones() as usize;
    count += (m1 & 0xFFFF).count_ones() as usize;
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk));
    count += (bits & 0xFFFF).count_ones() as usize;
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = unsafe { v128_load(ptr.add(overlap_start) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk));
    let already = cur - overlap_start;
    let scan_mask = ((!0u32) << already) & 0xFFFF;
    count += (bits & scan_mask).count_ones() as usize;
  }

  count
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(super) fn find_last<Nd>(input: &[u8], needles: Nd) -> Option<usize>
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
    let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
    let b0 = bitmask(needles.eq_any_mask_simd128(c0)) & 0xFFFF;
    let b1 = bitmask(needles.eq_any_mask_simd128(c1)) & 0xFFFF;
    if b0 != 0 {
      last = Some(cur + (31 - b0.leading_zeros()) as usize);
    }
    if b1 != 0 {
      last = Some(cur + CHUNK + (31 - b1.leading_zeros()) as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk)) & 0xFFFF;
    if bits != 0 {
      last = Some(cur + (31 - bits.leading_zeros()) as usize);
    }
    cur += CHUNK;
  }

  if cur < len {
    let overlap_start = len - CHUNK;
    let chunk = unsafe { v128_load(ptr.add(overlap_start) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk));
    let already = cur - overlap_start;
    let scan_mask = ((!0u32) << already) & 0xFFFF;
    let hit_bits = bits & scan_mask;
    if hit_bits != 0 {
      last = Some(overlap_start + (31 - hit_bits.leading_zeros()) as usize);
    }
  }

  last
}

// ── generic skip_until / skip_while ─────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
pub(super) fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
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
    let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
    let m0 = needles.eq_any_mask_simd128(c0);
    let m1 = needles.eq_any_mask_simd128(c1);
    let combined = bitmask(v128_or(m0, m1));
    if combined != 0 {
      let b0 = bitmask(m0);
      if b0 != 0 {
        return Some(cur + b0.trailing_zeros() as usize);
      }
      let b1 = bitmask(m1);
      return Some(cur + CHUNK + b1.trailing_zeros() as usize);
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk));
    if bits != 0 {
      return Some(cur + bits.trailing_zeros() as usize);
    }
    cur += CHUNK;
  }

  if cur == len {
    return None;
  }

  let overlap_start = len - CHUNK;
  let chunk = unsafe { v128_load(ptr.add(overlap_start) as *const v128) };
  let bits = bitmask(needles.eq_any_mask_simd128(chunk));
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
pub(super) fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
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
    let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
    let m0 = needles.eq_any_mask_simd128(c0);
    let m1 = needles.eq_any_mask_simd128(c1);
    let combined = bitmask(v128_and(m0, m1));
    if combined != 0xFFFF {
      let b0 = bitmask(m0);
      if b0 != 0xFFFF {
        return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
      }
      let b1 = bitmask(m1);
      return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
    }
    cur += 2 * CHUNK;
  }

  while cur + CHUNK <= len {
    let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
    let bits = bitmask(needles.eq_any_mask_simd128(chunk));
    if bits != 0xFFFF {
      return cur + ((!bits) & 0xFFFF).trailing_zeros() as usize;
    }
    cur += CHUNK;
  }

  if cur == len {
    return len;
  }

  let overlap_start = len - CHUNK;
  let chunk = unsafe { v128_load(ptr.add(overlap_start) as *const v128) };
  let bits = bitmask(needles.eq_any_mask_simd128(chunk));
  let already = cur - overlap_start;
  let scan_mask = (!0u32) << already;
  let non_match = (!bits) & scan_mask & 0xFFFF;
  if non_match != 0 {
    overlap_start + non_match.trailing_zeros() as usize
  } else {
    len
  }
}
