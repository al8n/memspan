#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

// ── SSE2 / SSE4.2  (128-bit, __m128i) ───────────────────────────────────────

/// Returns an `__m128i` where each byte lane is `0xFF` if the corresponding
/// byte in `chunk` matches any needle, or `0x00` otherwise.
///
/// Iterates over needles with an OR-accumulator. Used for slices longer than
/// 8 elements where a const-unrolled tree is not available.
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "sse2")]
pub(in crate::needles) unsafe fn eq_any_mask_dynamic_sse2(
  chunk: __m128i,
  needles: &[u8],
) -> __m128i {
  let mut acc = _mm_setzero_si128();
  for &n in needles {
    acc = _mm_or_si128(acc, _mm_cmpeq_epi8(chunk, _mm_set1_epi8(n as i8)));
  }
  acc
}

/// Const-dispatch variant: unrolled balanced OR tree for 0–8 needles.
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "sse2")]
pub(in crate::needles) unsafe fn eq_any_mask_const_sse2<const N: usize>(
  chunk: __m128i,
  needles: [u8; N],
) -> __m128i {
  macro_rules! cmp {
    ($i:expr) => {
      _mm_cmpeq_epi8(chunk, _mm_set1_epi8(needles[$i] as i8))
    };
  }
  match N {
    0 => _mm_setzero_si128(),
    1 => cmp!(0),
    2 => _mm_or_si128(cmp!(0), cmp!(1)),
    3 => _mm_or_si128(_mm_or_si128(cmp!(0), cmp!(1)), cmp!(2)),
    4 => _mm_or_si128(
      _mm_or_si128(cmp!(0), cmp!(1)),
      _mm_or_si128(cmp!(2), cmp!(3)),
    ),
    5 => _mm_or_si128(
      _mm_or_si128(
        _mm_or_si128(cmp!(0), cmp!(1)),
        _mm_or_si128(cmp!(2), cmp!(3)),
      ),
      cmp!(4),
    ),
    6 => _mm_or_si128(
      _mm_or_si128(
        _mm_or_si128(cmp!(0), cmp!(1)),
        _mm_or_si128(cmp!(2), cmp!(3)),
      ),
      _mm_or_si128(cmp!(4), cmp!(5)),
    ),
    7 => _mm_or_si128(
      _mm_or_si128(
        _mm_or_si128(cmp!(0), cmp!(1)),
        _mm_or_si128(cmp!(2), cmp!(3)),
      ),
      _mm_or_si128(_mm_or_si128(cmp!(4), cmp!(5)), cmp!(6)),
    ),
    8 => _mm_or_si128(
      _mm_or_si128(
        _mm_or_si128(cmp!(0), cmp!(1)),
        _mm_or_si128(cmp!(2), cmp!(3)),
      ),
      _mm_or_si128(
        _mm_or_si128(cmp!(4), cmp!(5)),
        _mm_or_si128(cmp!(6), cmp!(7)),
      ),
    ),
    _ => eq_any_mask_dynamic_sse2(chunk, &needles),
  }
}

// ── AVX2  (256-bit, __m256i) ─────────────────────────────────────────────────

/// Returns a `__m256i` where each byte lane is `0xFF` if the byte in `chunk`
/// matches any needle.
#[cfg(target_arch = "x86_64")]
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
pub(in crate::needles) unsafe fn eq_any_mask_dynamic_avx2(
  chunk: __m256i,
  needles: &[u8],
) -> __m256i {
  let mut acc = _mm256_setzero_si256();
  for &n in needles {
    acc = _mm256_or_si256(acc, _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(n as i8)));
  }
  acc
}

/// Const-dispatch variant for AVX2: unrolled balanced OR tree for 0–8 needles.
#[cfg(target_arch = "x86_64")]
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx2")]
pub(in crate::needles) unsafe fn eq_any_mask_const_avx2<const N: usize>(
  chunk: __m256i,
  needles: [u8; N],
) -> __m256i {
  macro_rules! cmp {
    ($i:expr) => {
      _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(needles[$i] as i8))
    };
  }
  match N {
    0 => _mm256_setzero_si256(),
    1 => cmp!(0),
    2 => _mm256_or_si256(cmp!(0), cmp!(1)),
    3 => _mm256_or_si256(_mm256_or_si256(cmp!(0), cmp!(1)), cmp!(2)),
    4 => _mm256_or_si256(
      _mm256_or_si256(cmp!(0), cmp!(1)),
      _mm256_or_si256(cmp!(2), cmp!(3)),
    ),
    5 => _mm256_or_si256(
      _mm256_or_si256(
        _mm256_or_si256(cmp!(0), cmp!(1)),
        _mm256_or_si256(cmp!(2), cmp!(3)),
      ),
      cmp!(4),
    ),
    6 => _mm256_or_si256(
      _mm256_or_si256(
        _mm256_or_si256(cmp!(0), cmp!(1)),
        _mm256_or_si256(cmp!(2), cmp!(3)),
      ),
      _mm256_or_si256(cmp!(4), cmp!(5)),
    ),
    7 => _mm256_or_si256(
      _mm256_or_si256(
        _mm256_or_si256(cmp!(0), cmp!(1)),
        _mm256_or_si256(cmp!(2), cmp!(3)),
      ),
      _mm256_or_si256(_mm256_or_si256(cmp!(4), cmp!(5)), cmp!(6)),
    ),
    8 => _mm256_or_si256(
      _mm256_or_si256(
        _mm256_or_si256(cmp!(0), cmp!(1)),
        _mm256_or_si256(cmp!(2), cmp!(3)),
      ),
      _mm256_or_si256(
        _mm256_or_si256(cmp!(4), cmp!(5)),
        _mm256_or_si256(cmp!(6), cmp!(7)),
      ),
    ),
    _ => eq_any_mask_dynamic_avx2(chunk, &needles),
  }
}

// ── AVX-512BW  (512-bit, __m512i → u64 mask) ─────────────────────────────────

/// Returns a `u64` bitmask where bit `i` is set if byte lane `i` of `chunk`
/// matches any needle. Uses `_mm512_cmpeq_epi8_mask` which returns the mask
/// directly — no `movemask` conversion needed.
#[cfg(target_arch = "x86_64")]
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx512bw")]
pub(in crate::needles) unsafe fn eq_any_mask_dynamic_avx512(chunk: __m512i, needles: &[u8]) -> u64 {
  let mut acc: u64 = 0;
  for &n in needles {
    acc |= _mm512_cmpeq_epi8_mask(chunk, _mm512_set1_epi8(n as i8));
  }
  acc
}

/// Const-dispatch variant for AVX-512BW: unrolled balanced OR for 0–8 needles.
#[cfg(target_arch = "x86_64")]
#[cfg_attr(not(tarpaulin), inline)]
#[target_feature(enable = "avx512bw")]
pub(in crate::needles) unsafe fn eq_any_mask_const_avx512<const N: usize>(
  chunk: __m512i,
  needles: [u8; N],
) -> u64 {
  macro_rules! cmp {
    ($i:expr) => {
      _mm512_cmpeq_epi8_mask(chunk, _mm512_set1_epi8(needles[$i] as i8))
    };
  }
  match N {
    0 => 0u64,
    1 => cmp!(0),
    2 => cmp!(0) | cmp!(1),
    3 => cmp!(0) | cmp!(1) | cmp!(2),
    4 => (cmp!(0) | cmp!(1)) | (cmp!(2) | cmp!(3)),
    5 => (cmp!(0) | cmp!(1) | cmp!(2) | cmp!(3)) | cmp!(4),
    6 => (cmp!(0) | cmp!(1) | cmp!(2) | cmp!(3)) | (cmp!(4) | cmp!(5)),
    7 => (cmp!(0) | cmp!(1) | cmp!(2) | cmp!(3)) | (cmp!(4) | cmp!(5) | cmp!(6)),
    8 => (cmp!(0) | cmp!(1) | cmp!(2) | cmp!(3)) | (cmp!(4) | cmp!(5) | cmp!(6) | cmp!(7)),
    _ => eq_any_mask_dynamic_avx512(chunk, &needles),
  }
}
