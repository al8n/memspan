/// SIMD and vendor intrinsics module.
pub mod arch;

mod sealed {
  pub trait Sealed {}

  impl<T> Sealed for &T where T: Sealed + ?Sized {}
  impl Sealed for u8 {}
  impl Sealed for [u8] {}
  impl<const N: usize> Sealed for [u8; N] {}
}

macro_rules! tail_find_fixed {
  ($name:ident, $($needle:ident),+) => {
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn $name(tail: &[u8], $($needle: u8),+) -> Option<usize> {
      for (idx, &byte) in tail.iter().enumerate() {
        if false $(|| byte == $needle)+ {
          return Some(idx);
        }
      }

      None
    }
  };
}

tail_find_fixed!(tail_find1, a);
tail_find_fixed!(tail_find2, a, b);
tail_find_fixed!(tail_find3, a, b, c);
tail_find_fixed!(tail_find4, a, b, c, d);
tail_find_fixed!(tail_find5, a, b, c, d, e);
tail_find_fixed!(tail_find6, a, b, c, d, e, f);
tail_find_fixed!(tail_find7, a, b, c, d, e, f, g);
tail_find_fixed!(tail_find8, a, b, c, d, e, f, g, h);

macro_rules! prefix_len_fixed {
  ($name:ident, $($needle:ident),+) => {
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn $name(input: &[u8], $($needle: u8),+) -> usize {
      for (idx, &byte) in input.iter().enumerate() {
        if !(false $(|| byte == $needle)+) {
          return idx;
        }
      }

      input.len()
    }
  };
}

prefix_len_fixed!(prefix_len1, a);
prefix_len_fixed!(prefix_len2, a, b);
prefix_len_fixed!(prefix_len3, a, b, c);
prefix_len_fixed!(prefix_len4, a, b, c, d);
prefix_len_fixed!(prefix_len5, a, b, c, d, e);
prefix_len_fixed!(prefix_len6, a, b, c, d, e, f);
prefix_len_fixed!(prefix_len7, a, b, c, d, e, f, g);
prefix_len_fixed!(prefix_len8, a, b, c, d, e, f, g, h);

/// Needle types for SIMD-accelerated searching.
pub trait Needles: sealed::Sealed {
  /// Number of needles in this set.
  ///
  /// The dispatcher uses this to pick between the scalar path (for inputs
  /// shorter than one SIMD chunk) and the wider SIMD loop.
  fn needle_count(&self) -> usize;

  /// Whether the needles are empty (e.g., an empty slice).
  #[inline(always)]
  fn is_empty(&self) -> bool {
    self.needle_count() == 0
  }

  /// Find the first occurrence of any of the needles in `tail`, returning
  /// the index (relative to the start of `tail`) if found.
  ///
  /// This is the scalar primitive that backs [`crate::skip::skip_until`]'s
  /// fallback path. Uses an unrolled per-byte loop that LLVM auto-vectorizes
  /// well for 1–8 needles; falls back to a per-byte `contains` check beyond 8.
  ///
  /// Calling this directly bypasses the per-call dispatcher overhead in
  /// [`crate::skip::skip_until`] (one length check + one runtime feature
  /// check). That overhead is invisible on long inputs but measurable on tight
  /// loops over very short slices (≲32 bytes). Prefer the dispatcher for
  /// general use; reach for this when you have known-short inputs and the
  /// extra few cycles per call matter.
  fn tail_find(&self, tail: &[u8]) -> Option<usize>;

  /// Returns the length of the longest prefix where every byte matches one of
  /// the needles.
  ///
  /// This is the scalar primitive that backs [`crate::skip::skip_while`]'s
  /// fallback path. For 1 needle and for 2–8 needle sets it uses tight,
  /// LLVM-auto-vectorized per-byte loops; for >8 needles it falls back to a
  /// per-byte `contains` check.
  ///
  /// Calling this directly bypasses the per-call dispatcher overhead in
  /// [`crate::skip::skip_while`]. That overhead is structural — the
  /// dispatcher's body holds both a scalar branch and a NEON branch, which
  /// LLVM doesn't fully inline at the call site — and it is the floor on
  /// throughput for inputs in the 16–32 byte range. If you call `skip_while`
  /// in a hot loop where every input is known to be short (e.g. you're
  /// already chunking elsewhere), calling `prefix_len` directly here recovers
  /// roughly 1.7× throughput at len=16. Prefer the dispatcher for general use.
  fn prefix_len(&self, input: &[u8]) -> usize;

  /// Returns a NEON byte mask where matching lanes are `0xFF` and non-matching
  /// lanes are `0x00`.
  #[cfg(target_arch = "aarch64")]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t;

  /// Returns an SSE2 byte mask (`__m128i`) where matching lanes are `0xFF` and
  /// non-matching lanes are `0x00`.
  #[cfg(target_arch = "x86")]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i;

  /// Returns an SSE2 byte mask (`__m128i`) where matching lanes are `0xFF` and
  /// non-matching lanes are `0x00`.
  #[cfg(target_arch = "x86_64")]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i;

  /// Returns an AVX2 byte mask (`__m256i`) where matching lanes are `0xFF` and
  /// non-matching lanes are `0x00`.
  #[cfg(target_arch = "x86_64")]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i;

  /// Returns a 64-bit AVX-512BW bitmask where bit `i` is set if byte lane `i`
  /// of `chunk` matches any needle. Unlike the SSE/AVX variants this is a
  /// direct `__mmask64` rather than a byte-per-lane vector.
  #[cfg(target_arch = "x86_64")]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64;

  /// Returns a WASM SIMD128 byte mask (`v128`) where matching lanes are `0xFF`
  /// and non-matching lanes are `0x00`.
  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128;
}

impl<T> Needles for &T
where
  T: Needles + ?Sized,
{
  #[inline(always)]
  fn needle_count(&self) -> usize {
    (**self).needle_count()
  }

  #[inline(always)]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    (**self).tail_find(tail)
  }

  #[inline(always)]
  fn prefix_len(&self, input: &[u8]) -> usize {
    (**self).prefix_len(input)
  }

  #[cfg(target_arch = "aarch64")]
  #[inline(always)]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    (**self).eq_any_mask_neon(chunk)
  }

  #[cfg(target_arch = "x86")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i {
    (**self).eq_any_mask_sse2(chunk)
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i {
    (**self).eq_any_mask_sse2(chunk)
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i {
    (**self).eq_any_mask_avx2(chunk)
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64 {
    (**self).eq_any_mask_avx512(chunk)
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[inline(always)]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    (**self).eq_any_mask_simd128(chunk)
  }
}

impl Needles for u8 {
  #[inline(always)]
  fn needle_count(&self) -> usize {
    1
  }

  #[inline(always)]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    tail_find1(tail, *self)
  }

  #[inline(always)]
  fn prefix_len(&self, input: &[u8]) -> usize {
    prefix_len1(input, *self)
  }

  #[cfg(target_arch = "aarch64")]
  #[inline(always)]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    let target = unsafe { core::arch::aarch64::vdupq_n_u8(*self) };
    unsafe { core::arch::aarch64::vceqq_u8(chunk, target) }
  }

  #[cfg(target_arch = "x86")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i {
    unsafe {
      use core::arch::x86::*;
      _mm_cmpeq_epi8(chunk, _mm_set1_epi8(*self as i8))
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i {
    unsafe {
      use core::arch::x86_64::*;
      _mm_cmpeq_epi8(chunk, _mm_set1_epi8(*self as i8))
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i {
    unsafe {
      use core::arch::x86_64::*;
      _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(*self as i8))
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64 {
    unsafe {
      use core::arch::x86_64::*;
      _mm512_cmpeq_epi8_mask(chunk, _mm512_set1_epi8(*self as i8))
    }
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[inline(always)]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    use core::arch::wasm32::*;
    i8x16_eq(chunk, i8x16_splat(*self as i8))
  }
}

impl Needles for [u8] {
  #[inline(always)]
  fn needle_count(&self) -> usize {
    self.len()
  }

  #[inline(always)]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    match self {
      [] => None,
      [a] => tail_find1(tail, *a),
      [a, b] => tail_find2(tail, *a, *b),
      [a, b, c] => tail_find3(tail, *a, *b, *c),
      [a, b, c, d] => tail_find4(tail, *a, *b, *c, *d),
      [a, b, c, d, e] => tail_find5(tail, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => tail_find6(tail, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => tail_find7(tail, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => tail_find8(tail, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => tail.iter().position(|byte| self.contains(byte)),
    }
  }

  #[inline(always)]
  fn prefix_len(&self, input: &[u8]) -> usize {
    match self {
      [] => 0,
      [a] => prefix_len1(input, *a),
      [a, b] => prefix_len2(input, *a, *b),
      [a, b, c] => prefix_len3(input, *a, *b, *c),
      [a, b, c, d] => prefix_len4(input, *a, *b, *c, *d),
      [a, b, c, d, e] => prefix_len5(input, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => prefix_len6(input, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => prefix_len7(input, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => prefix_len8(input, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => input
        .iter()
        .position(|byte| !self.contains(byte))
        .unwrap_or(input.len()),
    }
  }

  #[cfg(target_arch = "aarch64")]
  #[inline(always)]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    match self {
      [] => arch::aarch64::eq_any_mask_const(chunk, []),
      [a] => arch::aarch64::eq_any_mask_const(chunk, [*a]),
      [a, b] => arch::aarch64::eq_any_mask_const(chunk, [*a, *b]),
      [a, b, c] => arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c]),
      [a, b, c, d] => arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c, *d]),
      [a, b, c, d, e] => arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c, *d, *e]),
      [a, b, c, d, e, f] => arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c, *d, *e, *f]),
      [a, b, c, d, e, f, g] => {
        arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c, *d, *e, *f, *g])
      }
      [a, b, c, d, e, f, g, h] => {
        arch::aarch64::eq_any_mask_const(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      }
      _ => arch::aarch64::eq_any_mask_dynamic(chunk, self),
    }
  }

  #[cfg(target_arch = "x86")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i {
    match self {
      [] => unsafe { core::arch::x86::_mm_setzero_si128() },
      [a] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a]) },
      [a, b] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b]) },
      [a, b, c] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c]) },
      [a, b, c, d] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d]) },
      [a, b, c, d, e] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e]) },
      [a, b, c, d, e, f] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f])
      },
      [a, b, c, d, e, f, g] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f, *g])
      },
      [a, b, c, d, e, f, g, h] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      },
      _ => unsafe { arch::x86::eq_any_mask_dynamic_sse2(chunk, self) },
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i {
    match self {
      [] => unsafe { core::arch::x86_64::_mm_setzero_si128() },
      [a] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a]) },
      [a, b] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b]) },
      [a, b, c] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c]) },
      [a, b, c, d] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d]) },
      [a, b, c, d, e] => unsafe { arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e]) },
      [a, b, c, d, e, f] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f])
      },
      [a, b, c, d, e, f, g] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f, *g])
      },
      [a, b, c, d, e, f, g, h] => unsafe {
        arch::x86::eq_any_mask_const_sse2(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      },
      _ => unsafe { arch::x86::eq_any_mask_dynamic_sse2(chunk, self) },
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i {
    match self {
      [] => unsafe { core::arch::x86_64::_mm256_setzero_si256() },
      [a] => unsafe { arch::x86::eq_any_mask_const_avx2(chunk, [*a]) },
      [a, b] => unsafe { arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b]) },
      [a, b, c] => unsafe { arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c]) },
      [a, b, c, d] => unsafe { arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c, *d]) },
      [a, b, c, d, e] => unsafe { arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c, *d, *e]) },
      [a, b, c, d, e, f] => unsafe {
        arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c, *d, *e, *f])
      },
      [a, b, c, d, e, f, g] => unsafe {
        arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c, *d, *e, *f, *g])
      },
      [a, b, c, d, e, f, g, h] => unsafe {
        arch::x86::eq_any_mask_const_avx2(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      },
      _ => unsafe { arch::x86::eq_any_mask_dynamic_avx2(chunk, self) },
    }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64 {
    match self {
      [] => 0u64,
      [a] => unsafe { arch::x86::eq_any_mask_const_avx512(chunk, [*a]) },
      [a, b] => unsafe { arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b]) },
      [a, b, c] => unsafe { arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c]) },
      [a, b, c, d] => unsafe { arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c, *d]) },
      [a, b, c, d, e] => unsafe {
        arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c, *d, *e])
      },
      [a, b, c, d, e, f] => unsafe {
        arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c, *d, *e, *f])
      },
      [a, b, c, d, e, f, g] => unsafe {
        arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c, *d, *e, *f, *g])
      },
      [a, b, c, d, e, f, g, h] => unsafe {
        arch::x86::eq_any_mask_const_avx512(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      },
      _ => unsafe { arch::x86::eq_any_mask_dynamic_avx512(chunk, self) },
    }
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[inline(always)]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    match self {
      [] => arch::wasm32::eq_any_mask_const_simd128(chunk, []),
      [a] => arch::wasm32::eq_any_mask_const_simd128(chunk, [*a]),
      [a, b] => arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b]),
      [a, b, c] => arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c]),
      [a, b, c, d] => arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c, *d]),
      [a, b, c, d, e] => arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c, *d, *e]),
      [a, b, c, d, e, f] => {
        arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c, *d, *e, *f])
      }
      [a, b, c, d, e, f, g] => {
        arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c, *d, *e, *f, *g])
      }
      [a, b, c, d, e, f, g, h] => {
        arch::wasm32::eq_any_mask_const_simd128(chunk, [*a, *b, *c, *d, *e, *f, *g, *h])
      }
      _ => arch::wasm32::eq_any_mask_dynamic_simd128(chunk, self),
    }
  }
}

impl<const N: usize> Needles for [u8; N] {
  #[inline(always)]
  fn needle_count(&self) -> usize {
    N
  }

  #[inline(always)]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    match self.as_slice() {
      [] => None,
      [a] => tail_find1(tail, *a),
      [a, b] => tail_find2(tail, *a, *b),
      [a, b, c] => tail_find3(tail, *a, *b, *c),
      [a, b, c, d] => tail_find4(tail, *a, *b, *c, *d),
      [a, b, c, d, e] => tail_find5(tail, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => tail_find6(tail, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => tail_find7(tail, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => tail_find8(tail, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => tail.iter().position(|byte| self.contains(byte)),
    }
  }

  #[inline(always)]
  fn prefix_len(&self, input: &[u8]) -> usize {
    match self.as_slice() {
      [] => 0,
      [a] => prefix_len1(input, *a),
      [a, b] => prefix_len2(input, *a, *b),
      [a, b, c] => prefix_len3(input, *a, *b, *c),
      [a, b, c, d] => prefix_len4(input, *a, *b, *c, *d),
      [a, b, c, d, e] => prefix_len5(input, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => prefix_len6(input, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => prefix_len7(input, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => prefix_len8(input, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => input
        .iter()
        .position(|byte| !self.contains(byte))
        .unwrap_or(input.len()),
    }
  }

  #[cfg(target_arch = "aarch64")]
  #[inline(always)]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    arch::aarch64::eq_any_mask_const(chunk, *self)
  }

  #[cfg(target_arch = "x86")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i {
    unsafe { arch::x86::eq_any_mask_const_sse2(chunk, *self) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i {
    unsafe { arch::x86::eq_any_mask_const_sse2(chunk, *self) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i {
    unsafe { arch::x86::eq_any_mask_const_avx2(chunk, *self) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64 {
    unsafe { arch::x86::eq_any_mask_const_avx512(chunk, *self) }
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[inline(always)]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    arch::wasm32::eq_any_mask_const_simd128(chunk, *self)
  }
}

/// A needle set that also matches every non-ASCII byte (`0x80..=0xFF`).
///
/// This backs [`crate::skip::skip_until_or_non_ascii`]. Every backend's
/// `skip_until` reaches its needles only through [`Needles::tail_find`] and the
/// `eq_any_mask_*` methods, so wrapping the caller's set here reuses the
/// dispatcher, the scalar probe, the vector loops and the overlap tails
/// unchanged; each mask is the inner mask OR'ed with a high-bit mask.
pub(crate) struct OrNonAscii<Nd>(pub(crate) Nd);

impl<Nd: Needles> sealed::Sealed for OrNonAscii<Nd> {}

impl<Nd: Needles> Needles for OrNonAscii<Nd> {
  #[inline(always)]
  fn needle_count(&self) -> usize {
    // Only zero-ness changes an answer. `skip_until`, `skip_while` and
    // `find_last` in `src/skip/mod.rs` read this count (the first and last
    // return `None` at zero; `skip_while` sends a count of one or less to the
    // scalar `prefix_len`, which returns what the vector path would), and the
    // default `Needles::is_empty` compares it with zero. The `+ 1` counts the
    // non-ASCII class, so the count is never zero and an empty inner set still
    // reaches the non-ASCII search instead of returning `None` up front.
    self.0.needle_count() + 1
  }

  #[inline(always)]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    // Exact composition, inner needles first: a non-ASCII byte before the
    // first needle ends the ASCII prefix of `tail[..limit]` early and is the
    // answer; otherwise nothing before that needle is non-ASCII, so the answer
    // is the needle, or `None` when there is neither.
    let len = tail.len();
    let hit = self.0.tail_find(tail);
    let limit = hit.unwrap_or(len);
    let end = crate::skip::prefix_len_ascii(&tail[..limit]);
    if end < limit { Some(end) } else { hit }
  }

  #[inline(always)]
  fn prefix_len(&self, input: &[u8]) -> usize {
    // Terminates: every iteration that does not return advances `i` by at least one.
    let mut i = 0;
    loop {
      let a = self.0.prefix_len(&input[i..]);
      i += a;
      let b = crate::skip::prefix_len_non_ascii(&input[i..]);
      i += b;
      if a == 0 && b == 0 {
        return i;
      }
    }
  }

  #[cfg(target_arch = "aarch64")]
  #[inline(always)]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    let needles = self.0.eq_any_mask_neon(chunk);
    unsafe { core::arch::aarch64::vorrq_u8(needles, arch::aarch64::high_bit_mask(chunk)) }
  }

  #[cfg(target_arch = "x86")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86::__m128i) -> core::arch::x86::__m128i {
    let needles = self.0.eq_any_mask_sse2(chunk);
    unsafe { core::arch::x86::_mm_or_si128(needles, arch::x86::high_bit_mask_sse2(chunk)) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_sse2(&self, chunk: core::arch::x86_64::__m128i) -> core::arch::x86_64::__m128i {
    let needles = self.0.eq_any_mask_sse2(chunk);
    unsafe { core::arch::x86_64::_mm_or_si128(needles, arch::x86::high_bit_mask_sse2(chunk)) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx2(&self, chunk: core::arch::x86_64::__m256i) -> core::arch::x86_64::__m256i {
    let needles = self.0.eq_any_mask_avx2(chunk);
    unsafe { core::arch::x86_64::_mm256_or_si256(needles, arch::x86::high_bit_mask_avx2(chunk)) }
  }

  #[cfg(target_arch = "x86_64")]
  #[inline(always)]
  fn eq_any_mask_avx512(&self, chunk: core::arch::x86_64::__m512i) -> u64 {
    self.0.eq_any_mask_avx512(chunk) | unsafe { arch::x86::high_bit_mask_avx512(chunk) }
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[inline(always)]
  fn eq_any_mask_simd128(&self, chunk: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    let needles = self.0.eq_any_mask_simd128(chunk);
    core::arch::wasm32::v128_or(needles, arch::wasm32::high_bit_mask(chunk))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Holds `OrNonAscii`'s scalar methods to the definition on every input of up
  /// to four bytes over one needle, one other ASCII byte and both ends of the
  /// non-ASCII range. `prefix_len` is on no public path, so nothing else checks
  /// it.
  fn check_scalar_methods<Nd: Needles>(needles: Nd, members: &[u8]) {
    const ALPHABET: [u8; 4] = [b'"', b'a', 0x80, 0xFF];
    let adapter = OrNonAscii(needles);
    let is_member = |b: &u8| *b >= 0x80 || members.contains(b);
    let mut buf = [0u8; 4];
    for len in 0..=buf.len() {
      for code in 0..ALPHABET.len().pow(len as u32) {
        let mut rest = code;
        for slot in &mut buf[..len] {
          *slot = ALPHABET[rest % ALPHABET.len()];
          rest /= ALPHABET.len();
        }
        let input = &buf[..len];
        assert_eq!(
          adapter.tail_find(input),
          input.iter().position(is_member),
          "tail_find({input:?}) with needles {members:?}"
        );
        assert_eq!(
          adapter.prefix_len(input),
          input.iter().position(|b| !is_member(b)).unwrap_or(len),
          "prefix_len({input:?}) with needles {members:?}"
        );
      }
    }
  }

  #[test]
  fn or_non_ascii_scalar_methods_match_the_definition() {
    check_scalar_methods([0u8; 0], &[]);
    check_scalar_methods(b'"', b"\"");
    check_scalar_methods(*b"\"\xC3", b"\"\xC3");
    let nine: &[u8] = b"()[]{}<>\"";
    check_scalar_methods(nine, nine);
  }
}
