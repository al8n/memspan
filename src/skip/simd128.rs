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
#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn range_mask(chunk: v128, lo: u8, hi: u8) -> v128 {
  let x = i8x16_sub(chunk, i8x16_splat(lo as i8));
  u8x16_lt(x, u8x16_splat(hi.wrapping_sub(lo).wrapping_add(1)))
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
