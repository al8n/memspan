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
#[inline]
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
#[inline]
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

/// Returns an `__m128i` where each byte lane is `0xFF` if the corresponding
/// byte in `chunk` is not ASCII (`0x80..=0xFF`), or `0x00` otherwise.
///
/// The non-ASCII bytes are exactly the lanes that are negative as `i8`, so one
/// signed compare against zero (`pcmpgtb`) is the whole test.
#[inline]
#[target_feature(enable = "sse2")]
pub(in crate::needles) unsafe fn high_bit_mask_sse2(chunk: __m128i) -> __m128i {
  _mm_cmplt_epi8(chunk, _mm_setzero_si128())
}

// ── AVX2  (256-bit, __m256i) ─────────────────────────────────────────────────

/// Returns a `__m256i` where each byte lane is `0xFF` if the byte in `chunk`
/// matches any needle.
#[cfg(target_arch = "x86_64")]
#[inline]
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
#[inline]
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

/// Returns a `__m256i` where each byte lane is `0xFF` if the byte in `chunk` is
/// not ASCII (`0x80..=0xFF`): the AVX2 form of [`high_bit_mask_sse2`].
#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::needles) unsafe fn high_bit_mask_avx2(chunk: __m256i) -> __m256i {
  _mm256_cmpgt_epi8(_mm256_setzero_si256(), chunk)
}

// ── AVX-512BW  (512-bit, __m512i → u64 mask) ─────────────────────────────────

/// Returns a `u64` bitmask where bit `i` is set if byte lane `i` of `chunk`
/// matches any needle. Uses `_mm512_cmpeq_epi8_mask` which returns the mask
/// directly — no `movemask` conversion needed.
#[cfg(target_arch = "x86_64")]
#[inline]
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
#[inline]
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

/// Returns a `u64` bitmask where bit `i` is set if byte lane `i` of `chunk` is
/// not ASCII (`0x80..=0xFF`). `_mm512_movepi8_mask` copies each byte's high bit
/// straight into the mask (`vpmovb2m`), so no compare is needed.
#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx512bw")]
pub(in crate::needles) unsafe fn high_bit_mask_avx512(chunk: __m512i) -> u64 {
  _mm512_movepi8_mask(chunk)
}
