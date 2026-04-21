use core::arch::aarch64::*;

use crate::Needles;

const NEON_CHUNK_SIZE: usize = 16;

/// Pack a 16-byte byte-mask (`0xFF`/`0x00` per lane) into a `u64` where each
/// 4-bit nibble represents one lane. The first matching lane is then at bit
/// position `bits.trailing_zeros() & !3`, i.e. lane index `tz / 4`.
///
/// This is the simdjson-style `shrn`-trick: a single narrow shift replaces the
/// `vand`+`vaddv` reduction.
#[doc(hidden)]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn nibble_mask(cmp: uint8x16_t) -> u64 {
  let narrowed = unsafe { vshrn_n_u16::<4>(vreinterpretq_u16_u8(cmp)) };
  unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(narrowed)) }
}

/// Test whether each byte in `chunk` lies in `[lo, hi]`.
///
/// Uses the unsigned-subtract trick: `chunk - lo` shifts the in-range bytes to
/// `0..=hi-lo` and underflows out-of-range bytes to large values (≥ 0x80
/// thanks to two's-complement wrap on `u8`), so a single `<` against
/// `hi - lo + 1` does both bounds at once. That's two dataflow ops per chunk
/// (`vsubq` + `vcltq`) instead of the three a `vcgeq` + `vcleq` + `vandq`
/// triplet would need; the constants are loop-hoisted by `inline(always)`.
///
/// The full-range case `lo = 0x00, hi = 0xFF` is special-cased: the normal
/// formula would compute `bound = 0xFF + 1 = 0`, making `vcltq_u8(x, 0)`
/// always false. Instead we return an all-ones mask directly.
#[doc(hidden)]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn range_mask(chunk: uint8x16_t, lo: u8, hi: u8) -> uint8x16_t {
  let width = hi.wrapping_sub(lo);
  if width == 0xFF {
    return unsafe { vdupq_n_u8(0xFF) };
  }
  let shifted = unsafe { vsubq_u8(chunk, vdupq_n_u8(lo)) };
  let bound = unsafe { vdupq_n_u8(width.wrapping_add(1)) };
  unsafe { vcltq_u8(shifted, bound) }
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn binary_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, b'0', b'1')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn octal_digit_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, b'0', b'7')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn digit_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, b'0', b'9')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn hex_digit_mask(chunk: uint8x16_t) -> uint8x16_t {
  let digit = digit_mask(chunk);
  let lower = unsafe { vorrq_u8(chunk, vdupq_n_u8(0x20)) };
  let alpha = range_mask(lower, b'a', b'f');
  unsafe { vorrq_u8(digit, alpha) }
}

/// Whitespace: `' '`, `'\t'`, `'\n'`, `'\r'`. Four direct equality probes
/// reduced via a balanced OR tree (≈ 4 cycles latency on a 4-wide pipeline).
#[cfg_attr(not(tarpaulin), inline(always))]
fn whitespace_mask(chunk: uint8x16_t) -> uint8x16_t {
  let space = unsafe { vceqq_u8(chunk, vdupq_n_u8(b' ')) };
  let tab = unsafe { vceqq_u8(chunk, vdupq_n_u8(b'\t')) };
  let nl = unsafe { vceqq_u8(chunk, vdupq_n_u8(b'\n')) };
  let cr = unsafe { vceqq_u8(chunk, vdupq_n_u8(b'\r')) };
  unsafe { vorrq_u8(vorrq_u8(space, tab), vorrq_u8(nl, cr)) }
}

/// `[a-zA-Z]` via the OR-with-0x20 case-fold trick: 3 ops/chunk.
#[cfg_attr(not(tarpaulin), inline(always))]
fn alpha_mask(chunk: uint8x16_t) -> uint8x16_t {
  let lower = unsafe { vorrq_u8(chunk, vdupq_n_u8(0x20)) };
  range_mask(lower, b'a', b'z')
}

/// `[a-zA-Z0-9]` — composes `alpha_mask` and `digit_mask`. The two range
/// chains run independently and merge with a final `vorrq` (≈ 6 ops/chunk).
#[cfg_attr(not(tarpaulin), inline(always))]
fn alphanumeric_mask(chunk: uint8x16_t) -> uint8x16_t {
  let alpha = alpha_mask(chunk);
  let digit = digit_mask(chunk);
  unsafe { vorrq_u8(alpha, digit) }
}

/// `[a-zA-Z_]` — alpha plus a single `_` equality.
#[cfg_attr(not(tarpaulin), inline(always))]
fn ident_start_mask(chunk: uint8x16_t) -> uint8x16_t {
  let alpha = alpha_mask(chunk);
  let underscore = unsafe { vceqq_u8(chunk, vdupq_n_u8(b'_')) };
  unsafe { vorrq_u8(alpha, underscore) }
}

/// `[a-zA-Z0-9_]` — alphanumeric plus underscore. The heaviest mask in the
/// family (≈ 8 ops/chunk) but still ~4× cheaper than the equivalent
/// 63-needle `skip_while` slice.
#[cfg_attr(not(tarpaulin), inline(always))]
fn ident_mask(chunk: uint8x16_t) -> uint8x16_t {
  let alphanum = alphanumeric_mask(chunk);
  let underscore = unsafe { vceqq_u8(chunk, vdupq_n_u8(b'_')) };
  unsafe { vorrq_u8(alphanum, underscore) }
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn lower_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, b'a', b'z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn upper_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, b'A', b'Z')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, 0x00, 0x7F)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn non_ascii_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, 0x80, 0xFF)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_graphic_mask(chunk: uint8x16_t) -> uint8x16_t {
  range_mask(chunk, 0x21, 0x7E)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn ascii_control_mask(chunk: uint8x16_t) -> uint8x16_t {
  let ctrl = range_mask(chunk, 0x00, 0x1F);
  let del = unsafe { vceqq_u8(chunk, vdupq_n_u8(0x7F)) };
  unsafe { vorrq_u8(ctrl, del) }
}

macro_rules! skip_ascii_class {
  ($name:ident, $prefix_len:ident, $mask:ident) => {
    #[cfg_attr(not(tarpaulin), inline(always))]
    #[cfg(target_feature = "neon")]
    pub(super) fn $name(input: &[u8]) -> usize {
      let len = input.len();

      // Precondition normally enforced by the dispatcher; kept defensive so
      // the NEON helper is safe to call directly from tests or future code.
      if len < NEON_CHUNK_SIZE {
        return super::$prefix_len(input);
      }

      let ptr = input.as_ptr();

      // Most lexer numeric tokens are short. Probe one chunk scalar first so
      // a 1–15 byte number pays only a cheap early-exit loop, not a SIMD load
      // plus mask extraction.
      let first_chunk_len = super::$prefix_len(&input[..NEON_CHUNK_SIZE]);
      if first_chunk_len != NEON_CHUNK_SIZE {
        return first_chunk_len;
      }

      let mut cur = NEON_CHUNK_SIZE;

      // 2× unrolled main loop: AND both 16-byte match masks; if the AND is
      // all-ones both chunks are clean. One vget_lane_u64 covers 32 bytes,
      // halving the SIMD→GPR transfer cost on the hot all-match path.
      while cur + 2 * NEON_CHUNK_SIZE <= len {
        let c0 = unsafe { vld1q_u8(ptr.add(cur)) };
        let c1 = unsafe { vld1q_u8(ptr.add(cur + NEON_CHUNK_SIZE)) };
        let m0 = $mask(c0);
        let m1 = $mask(c1);
        let miss_bits = !nibble_mask(unsafe { vandq_u8(m0, m1) });
        if miss_bits != 0 {
          let mb0 = !nibble_mask(m0);
          if mb0 != 0 {
            return cur + (mb0.trailing_zeros() / 4) as usize;
          }
          let mb1 = !nibble_mask(m1);
          return cur + NEON_CHUNK_SIZE + (mb1.trailing_zeros() / 4) as usize;
        }
        cur += 2 * NEON_CHUNK_SIZE;
      }

      while cur + NEON_CHUNK_SIZE <= len {
        let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
        let cmp = $mask(chunk);
        let miss_bits = !nibble_mask(cmp);
        if miss_bits != 0 {
          return cur + (miss_bits.trailing_zeros() / 4) as usize;
        }
        cur += NEON_CHUNK_SIZE;
      }

      if cur == len {
        return len;
      }

      let overlap_start = len - NEON_CHUNK_SIZE;
      let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
      let cmp = $mask(chunk);

      let already_scanned_lanes = cur - overlap_start;
      let lane_mask = (!0u64) << (already_scanned_lanes * 4);
      let miss_bits = !nibble_mask(cmp) & lane_mask;

      if miss_bits != 0 {
        overlap_start + (miss_bits.trailing_zeros() / 4) as usize
      } else {
        len
      }
    }
  };
}

skip_ascii_class!(skip_binary, prefix_len_binary, binary_mask);
skip_ascii_class!(skip_digits, prefix_len_digits, digit_mask);
skip_ascii_class!(skip_hex_digits, prefix_len_hex_digits, hex_digit_mask);
skip_ascii_class!(skip_octal_digits, prefix_len_octal_digits, octal_digit_mask);
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

#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_feature = "neon")]
pub(super) fn count_matches<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  let len = input.len();
  if len < NEON_CHUNK_SIZE {
    return input
      .iter()
      .filter(|&&b| needles.tail_find(core::slice::from_ref(&b)).is_some())
      .count();
  }

  let ptr = input.as_ptr();
  let mut count = 0usize;
  let mut cur = 0;

  while cur + 2 * NEON_CHUNK_SIZE <= len {
    let c0 = unsafe { vld1q_u8(ptr.add(cur)) };
    let c1 = unsafe { vld1q_u8(ptr.add(cur + NEON_CHUNK_SIZE)) };
    let m0 = needles.eq_any_mask_neon(c0);
    let m1 = needles.eq_any_mask_neon(c1);
    count += (nibble_mask(m0).count_ones() / 4) as usize;
    count += (nibble_mask(m1).count_ones() / 4) as usize;
    cur += 2 * NEON_CHUNK_SIZE;
  }

  while cur + NEON_CHUNK_SIZE <= len {
    let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
    let cmp = needles.eq_any_mask_neon(chunk);
    count += (nibble_mask(cmp).count_ones() / 4) as usize;
    cur += NEON_CHUNK_SIZE;
  }

  if cur < len {
    let overlap_start = len - NEON_CHUNK_SIZE;
    let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
    let cmp = needles.eq_any_mask_neon(chunk);
    let already = cur - overlap_start;
    let lane_mask = (!0u64) << (already * 4);
    count += (nibble_mask(cmp) & lane_mask).count_ones() as usize / 4;
  }

  count
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_feature = "neon")]
pub(super) fn find_last<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  let len = input.len();
  if len < NEON_CHUNK_SIZE {
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

  while cur + 2 * NEON_CHUNK_SIZE <= len {
    let c0 = unsafe { vld1q_u8(ptr.add(cur)) };
    let c1 = unsafe { vld1q_u8(ptr.add(cur + NEON_CHUNK_SIZE)) };
    let b0 = nibble_mask(needles.eq_any_mask_neon(c0));
    let b1 = nibble_mask(needles.eq_any_mask_neon(c1));
    if b0 != 0 {
      last = Some(cur + (15 - b0.leading_zeros() / 4) as usize);
    }
    if b1 != 0 {
      last = Some(cur + NEON_CHUNK_SIZE + (15 - b1.leading_zeros() / 4) as usize);
    }
    cur += 2 * NEON_CHUNK_SIZE;
  }

  while cur + NEON_CHUNK_SIZE <= len {
    let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
    let bits = nibble_mask(needles.eq_any_mask_neon(chunk));
    if bits != 0 {
      last = Some(cur + (15 - bits.leading_zeros() / 4) as usize);
    }
    cur += NEON_CHUNK_SIZE;
  }

  if cur < len {
    let overlap_start = len - NEON_CHUNK_SIZE;
    let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
    let already = cur - overlap_start;
    let lane_mask = (!0u64) << (already * 4);
    let bits = nibble_mask(needles.eq_any_mask_neon(chunk)) & lane_mask;
    if bits != 0 {
      last = Some(overlap_start + (15 - bits.leading_zeros() / 4) as usize);
    }
  }

  last
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_feature = "neon")]
pub(super) fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  let len = input.len();

  // Precondition normally enforced by the dispatcher; kept as a defensive
  // fallback so this function is also safe to call directly.
  if len < NEON_CHUNK_SIZE {
    return needles.tail_find(input);
  }

  let ptr = input.as_ptr();

  // Scalar probe of the first chunk. On dense-hit lexer workloads (whitespace,
  // quotes, separators every few bytes) the very first chunk contains the hit
  // at a low offset, and a per-byte early-exit loop beats the full-chunk SIMD
  // load + extract sequence: ~5–6 byte iterations is cheaper than `vld +
  // 5×vceqq + 4×vorrq + vshrn + vget + ctz + return`, and we skip the SIMD
  // register-setup the function would do up front. For hit-poor inputs the
  // probe misses and we fall through to the SIMD loop, having already
  // covered the first 16 bytes — no duplicate work.
  if let Some(hit) = needles.tail_find(&input[..NEON_CHUNK_SIZE]) {
    return Some(hit);
  }

  let mut cur: usize = NEON_CHUNK_SIZE;

  // 2× unrolled main loop: OR both 16-byte hit masks; one vget_lane_u64
  // covers 32 bytes, halving the SIMD→GPR transfer cost on the no-hit path.
  while cur + 2 * NEON_CHUNK_SIZE <= len {
    let c0 = unsafe { vld1q_u8(ptr.add(cur)) };
    let c1 = unsafe { vld1q_u8(ptr.add(cur + NEON_CHUNK_SIZE)) };
    let m0 = needles.eq_any_mask_neon(c0);
    let m1 = needles.eq_any_mask_neon(c1);
    let combined = nibble_mask(unsafe { vorrq_u8(m0, m1) });
    if combined != 0 {
      let b0 = nibble_mask(m0);
      if b0 != 0 {
        return Some(cur + (b0.trailing_zeros() / 4) as usize);
      }
      let b1 = nibble_mask(m1);
      return Some(cur + NEON_CHUNK_SIZE + (b1.trailing_zeros() / 4) as usize);
    }
    cur += 2 * NEON_CHUNK_SIZE;
  }

  while cur + NEON_CHUNK_SIZE <= len {
    let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
    let cmp = needles.eq_any_mask_neon(chunk);
    let bits = nibble_mask(cmp);
    if bits != 0 {
      return Some(cur + (bits.trailing_zeros() / 4) as usize);
    }
    cur += NEON_CHUNK_SIZE;
  }

  if cur == len {
    return None;
  }

  // Tail: overlap with the last NEON chunk so we never read out-of-bounds
  // and never need a scratch buffer. Mask off lanes the main loop already
  // covered.
  let overlap_start = len - NEON_CHUNK_SIZE;
  let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
  let cmp = needles.eq_any_mask_neon(chunk);

  let already_scanned_lanes = cur - overlap_start;
  let lane_mask = (!0u64) << (already_scanned_lanes * 4);
  let bits = nibble_mask(cmp) & lane_mask;

  if bits != 0 {
    Some(overlap_start + (bits.trailing_zeros() / 4) as usize)
  } else {
    None
  }
}

#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_feature = "neon")]
pub(super) fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  let len = input.len();

  // Precondition normally enforced by the dispatcher; kept as a defensive
  // fallback so this function is also safe to call directly.
  if len < NEON_CHUNK_SIZE {
    return needles.prefix_len(input);
  }

  let ptr = input.as_ptr();

  // Scalar probe of the first chunk. Symmetric to `skip_until`: on dense-miss
  // workloads (typical lexer whitespace runs of 1–4 bytes) the very first
  // chunk holds the non-match at a low offset, and a per-byte early-exit
  // beats the SIMD `vld + eq + vshrn + vget + ctz + return` sequence — and
  // skips the SIMD register-setup. For long-run inputs the probe scans the
  // full 16 bytes scalar, then we fall through into the SIMD loop having
  // already covered them.
  let first_chunk_len = needles.prefix_len(&input[..NEON_CHUNK_SIZE]);
  if first_chunk_len != NEON_CHUNK_SIZE {
    return first_chunk_len;
  }

  let mut cur: usize = NEON_CHUNK_SIZE;

  // 2× unrolled: AND both match masks; if the AND is all-ones both chunks are
  // clean. One vget_lane_u64 covers 32 bytes on the hot all-match path.
  while cur + 2 * NEON_CHUNK_SIZE <= len {
    let c0 = unsafe { vld1q_u8(ptr.add(cur)) };
    let c1 = unsafe { vld1q_u8(ptr.add(cur + NEON_CHUNK_SIZE)) };
    let m0 = needles.eq_any_mask_neon(c0);
    let m1 = needles.eq_any_mask_neon(c1);
    let miss_bits = !nibble_mask(unsafe { vandq_u8(m0, m1) });
    if miss_bits != 0 {
      let mb0 = !nibble_mask(m0);
      if mb0 != 0 {
        return cur + (mb0.trailing_zeros() / 4) as usize;
      }
      let mb1 = !nibble_mask(m1);
      return cur + NEON_CHUNK_SIZE + (mb1.trailing_zeros() / 4) as usize;
    }
    cur += 2 * NEON_CHUNK_SIZE;
  }

  while cur + NEON_CHUNK_SIZE <= len {
    let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
    let cmp = needles.eq_any_mask_neon(chunk);
    let miss_bits = !nibble_mask(cmp);
    if miss_bits != 0 {
      return cur + (miss_bits.trailing_zeros() / 4) as usize;
    }
    cur += NEON_CHUNK_SIZE;
  }

  if cur == len {
    return len;
  }

  // Tail: overlap with the last NEON chunk so we never read out-of-bounds.
  let overlap_start = len - NEON_CHUNK_SIZE;
  let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
  let cmp = needles.eq_any_mask_neon(chunk);

  let already_scanned_lanes = cur - overlap_start;
  let lane_mask = (!0u64) << (already_scanned_lanes * 4);
  let miss_bits = !nibble_mask(cmp) & lane_mask;

  if miss_bits != 0 {
    overlap_start + (miss_bits.trailing_zeros() / 4) as usize
  } else {
    len
  }
}
