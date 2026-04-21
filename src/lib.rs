#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]
#![deny(missing_docs)]

#[cfg(feature = "std")]
extern crate std;

pub use skip::*;

/// SIMD-accelerated skipping utilities for lexing.
pub mod skip;

/// Utilities for SIMD-accelerated lexing, including CPU feature detection.
pub mod utils;

mod needles;

pub use needles::Needles;

/// Internal items re-exported for use by the [`skip_class!`] macro. Not part
/// of the stable public API; treat anything inside this module as a private
/// implementation detail. The module exists so the macro-generated code can
/// reach the same `range_mask` / `nibble_mask` / dispatcher constants the
/// built-in `skip_*` fns use, without exposing them in the top-level surface.
#[doc(hidden)]
pub mod __macro {
  #[cfg(target_arch = "aarch64")]
  pub use crate::utils::neon_available;

  /// Width of the SIMD chunk processed per iteration.
  pub const NEON_CHUNK_SIZE: usize = 16;

  /// Inputs shorter than this go through the scalar fallback. Chosen
  /// empirically; matches the threshold used by [`crate::skip::skip_while`]
  /// and the built-in `skip_*` fns.
  pub const SCALAR_THRESHOLD: usize = 32;

  #[cfg(target_arch = "aarch64")]
  pub use crate::skip::neon::{nibble_mask, range_mask};

  #[cfg(target_arch = "aarch64")]
  pub use core::arch::aarch64::{uint8x16_t, vceqq_u8, vdupq_n_u8, vld1q_u8, vorrq_u8};

  // ── x86 / SSE4.1 ─────────────────────────────────────────────────────────

  /// Width of the SSE4.1 and WASM SIMD128 chunks.
  pub const SSE_CHUNK_SIZE: usize = 16;

  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  pub use crate::utils::sse42_available;

  #[cfg(target_arch = "x86")]
  pub use core::arch::x86::{
    __m128i, _mm_and_si128, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_min_epu8, _mm_movemask_epi8,
    _mm_or_si128, _mm_set1_epi8, _mm_setzero_si128, _mm_sub_epi8,
  };

  #[cfg(target_arch = "x86_64")]
  pub use core::arch::x86_64::{
    __m128i, __m256i, _mm_and_si128, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_min_epu8,
    _mm_movemask_epi8, _mm_or_si128, _mm_set1_epi8, _mm_setzero_si128, _mm_sub_epi8,
    _mm256_and_si256, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_min_epu8, _mm256_movemask_epi8,
    _mm256_or_si256, _mm256_set1_epi8, _mm256_setzero_si256, _mm256_sub_epi8,
  };

  // ── x86_64 / AVX2 ────────────────────────────────────────────────────────

  /// Width of the AVX2 chunk.
  pub const AVX2_CHUNK_SIZE: usize = 32;

  #[cfg(target_arch = "x86_64")]
  pub use crate::utils::avx2_available;

  // ── wasm32 / SIMD128 ─────────────────────────────────────────────────────

  #[cfg(target_arch = "wasm32")]
  pub use crate::utils::simd128_available;

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  pub use core::arch::wasm32::{
    i8x16_bitmask, i8x16_eq, i8x16_splat, i8x16_sub, u8x16_lt, u8x16_splat, v128, v128_and,
    v128_load, v128_or,
  };
}

/// Define a custom `skip_*` function for an ASCII byte class, generating the
/// same scalar fallback + SIMD loop the built-in [`skip::skip_digits`],
/// [`skip::skip_whitespace`], etc. use internally. On x86/x86_64 the
/// generated function dispatches through AVX2 (256-bit) → SSE4.1 (128-bit)
/// → scalar; on aarch64 through NEON; on wasm32 through SIMD128.
///
/// The generated function returns the length of the leading prefix where
/// every byte is in the user-defined class. Both the scalar predicate
/// (auto-vectorized by LLVM) and the NEON mask (range-check + equality
/// `vorrq` tree) are derived from the same byte/range list, so there is no
/// risk of the two paths drifting out of sync.
///
/// # Syntax
///
/// ```ignore
/// skipchr::skip_class! {
///     /// Doc comment forwarded to the generated fn.
///     pub fn skip_my_class(
///         bytes  = [b' ', b'\t'],          // optional
///         ranges = [b'a'..=b'z', 0x30..=0x39],  // optional
///     );
/// }
/// ```
///
/// At least one byte or one range must be provided (an empty class is a
/// no-op that always returns 0). The two sections are independent and may
/// be used together or separately, but `bytes` must come first when both
/// are given.
///
/// # Examples
///
/// A bytes-only class — whitespace plus a comma separator:
///
/// ```
/// skipchr::skip_class! {
///     pub fn skip_ws_and_comma(bytes = [b' ', b'\t', b'\r', b'\n', b',']);
/// }
///
/// assert_eq!(skip_ws_and_comma(b"   ,\t\nfoo"), 6);
/// assert_eq!(skip_ws_and_comma(b"foo"), 0);
/// ```
///
/// A range-only class — the leading run of lowercase ASCII letters:
///
/// ```
/// skipchr::skip_class! {
///     pub fn skip_lowercase(ranges = [b'a'..=b'z']);
/// }
///
/// assert_eq!(skip_lowercase(b"abcXYZ"), 3);
/// ```
///
/// A mixed class — alphanumeric plus a few punctuation bytes:
///
/// ```
/// skipchr::skip_class! {
///     pub fn skip_punct_ident(
///         bytes  = [b'_', b'-', b'!', b'?'],
///         ranges = [b'a'..=b'z', b'A'..=b'Z', b'0'..=b'9'],
///     );
/// }
///
/// assert_eq!(skip_punct_ident(b"hello-world! 42"), 12);
/// ```
///
/// # Multi-byte sequences
///
/// The macro is single-byte by design: each SIMD lane carries one byte and
/// answers a yes/no membership test. Multi-byte sequences (a UTF-8 BOM, a
/// keyword, a multi-char operator) require cross-lane state that doesn't fit
/// this loop shape. If you need them, layer a one-shot `slice::starts_with`
/// (or a separate scanner) at the position where this fn stops; that adds at
/// most a few cycles per occurrence and keeps the SIMD bulk fast.
///
/// # Performance
///
/// Generated fns track the built-in specializations: ~10–13× scalar at
/// peak throughput on inputs ≥ 256 bytes; tied with scalar (within wrapper
/// overhead) on inputs < 32 bytes. See the `skip_lexer_class` bench suite
/// for representative numbers across mask shapes.
#[macro_export]
macro_rules! skip_class {
  (
    $(#[$attr:meta])*
    $vis:vis fn $name:ident
    (
      $(bytes = [$($byte:expr),+ $(,)?] $(,)?)?
      $(ranges = [$($lo:literal ..= $hi:literal),+ $(,)?] $(,)?)?
    )
    ;
  ) => {
    $(#[$attr])*
    #[inline(always)]
    $vis fn $name(input: &[u8]) -> usize {
      #[inline(always)]
      fn predicate(b: u8) -> bool {
        // The `let _` pacifies the unused-variable warning when neither
        // bytes nor ranges are provided (degenerate empty-class case).
        let _ = b;
        false
        $( $( || b == $byte )+ )?
        $( $( || ::core::matches!(b, $lo..=$hi) )+ )?
      }

      #[inline(always)]
      fn prefix_len_scalar(input: &[u8]) -> usize {
        input
          .iter()
          .position(|&b| !predicate(b))
          .unwrap_or(input.len())
      }

      if input.len() < $crate::__macro::SCALAR_THRESHOLD {
        return prefix_len_scalar(input);
      }

      #[cfg(target_arch = "aarch64")]
      {
        if $crate::__macro::neon_available() {
          return neon_path(input);
        }
      }

      #[cfg(target_arch = "x86_64")]
      {
        if $crate::__macro::avx2_available() {
          return unsafe { avx2_path(input) };
        }
      }

      #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
      {
        if $crate::__macro::sse42_available() {
          return unsafe { sse_path(input) };
        }
      }

      #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
      {
        if $crate::__macro::simd128_available() {
          return simd128_path(input);
        }
      }

      return prefix_len_scalar(input);

      #[cfg(target_arch = "aarch64")]
      #[inline(always)]
      fn neon_path(input: &[u8]) -> usize {
        use $crate::__macro::{
          NEON_CHUNK_SIZE, nibble_mask, range_mask, uint8x16_t, vceqq_u8,
          vdupq_n_u8, vld1q_u8, vorrq_u8,
        };

        // Combine every byte equality and every range check into a single
        // `vorrq` accumulator. Constants are loop-hoisted by the inliner.
        #[inline(always)]
        fn mask(chunk: uint8x16_t) -> uint8x16_t {
          let mut acc = unsafe { vdupq_n_u8(0) };
          $( $(
            acc = unsafe { vorrq_u8(acc, vceqq_u8(chunk, vdupq_n_u8($byte))) };
          )+ )?
          $( $(
            acc = unsafe { vorrq_u8(acc, range_mask(chunk, $lo, $hi)) };
          )+ )?
          let _ = chunk; // pacify warning if both sections empty
          acc
        }

        let len = input.len();
        if len < NEON_CHUNK_SIZE {
          return prefix_len_scalar(input);
        }

        let ptr = input.as_ptr();

        // Scalar probe of the first chunk: cheap early-exit on dense-miss
        // workloads, a no-op-equivalent on long-run inputs.
        let first_chunk_len = prefix_len_scalar(&input[..NEON_CHUNK_SIZE]);
        if first_chunk_len != NEON_CHUNK_SIZE {
          return first_chunk_len;
        }

        let mut cur = NEON_CHUNK_SIZE;
        while cur + NEON_CHUNK_SIZE <= len {
          let chunk = unsafe { vld1q_u8(ptr.add(cur)) };
          let cmp = mask(chunk);
          let miss_bits = !nibble_mask(cmp);
          if miss_bits != 0 {
            return cur + (miss_bits.trailing_zeros() / 4) as usize;
          }
          cur += NEON_CHUNK_SIZE;
        }

        if cur == len {
          return len;
        }

        // Overlap-tail: load the final chunk so we never read out of
        // bounds, then mask off the lanes the main loop already covered.
        let overlap_start = len - NEON_CHUNK_SIZE;
        let chunk = unsafe { vld1q_u8(ptr.add(overlap_start)) };
        let cmp = mask(chunk);
        let already_scanned = cur - overlap_start;
        let lane_mask = (!0u64) << (already_scanned * 4);
        let miss_bits = !nibble_mask(cmp) & lane_mask;
        if miss_bits != 0 {
          overlap_start + (miss_bits.trailing_zeros() / 4) as usize
        } else {
          len
        }
      }

      #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
      #[target_feature(enable = "sse4.1")]
      unsafe fn sse_path(input: &[u8]) -> usize {
        use $crate::__macro::{
          SSE_CHUNK_SIZE as CHUNK,
          __m128i, _mm_and_si128, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_min_epu8,
          _mm_movemask_epi8, _mm_or_si128, _mm_set1_epi8, _mm_setzero_si128, _mm_sub_epi8,
        };

        #[target_feature(enable = "sse4.1")]
        unsafe fn mask(chunk: __m128i) -> __m128i {
          let mut acc = unsafe { _mm_setzero_si128() };
          $( $(
            acc = unsafe {
              _mm_or_si128(acc, _mm_cmpeq_epi8(chunk, _mm_set1_epi8($byte as i8)))
            };
          )+ )?
          $( $(
            acc = unsafe {
              let x = _mm_sub_epi8(chunk, _mm_set1_epi8($lo as i8));
              let lim = _mm_set1_epi8(($hi as u8).wrapping_sub($lo as u8) as i8);
              _mm_or_si128(acc, _mm_cmpeq_epi8(x, _mm_min_epu8(x, lim)))
            };
          )+ )?
          let _ = chunk;
          acc
        }

        let len = input.len();
        if len < CHUNK {
          return prefix_len_scalar(input);
        }
        let ptr = input.as_ptr();
        let first = prefix_len_scalar(&input[..CHUNK]);
        if first != CHUNK {
          return first;
        }
        let mut cur = CHUNK;

        while cur + 2 * CHUNK <= len {
          let c0 = unsafe { _mm_loadu_si128(ptr.add(cur) as *const __m128i) };
          let c1 = unsafe { _mm_loadu_si128(ptr.add(cur + CHUNK) as *const __m128i) };
          let m0 = unsafe { mask(c0) };
          let m1 = unsafe { mask(c1) };
          let combined = unsafe { _mm_movemask_epi8(_mm_and_si128(m0, m1)) } as u32;
          if combined != 0xFFFF {
            let b0 = unsafe { _mm_movemask_epi8(m0) } as u32;
            if b0 != 0xFFFF {
              return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
            }
            let b1 = unsafe { _mm_movemask_epi8(m1) } as u32;
            return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
          }
          cur += 2 * CHUNK;
        }

        while cur + CHUNK <= len {
          let chunk = unsafe { _mm_loadu_si128(ptr.add(cur) as *const __m128i) };
          let bits = unsafe { _mm_movemask_epi8(mask(chunk)) } as u32;
          if bits != 0xFFFF {
            return cur + ((!bits) & 0xFFFF).trailing_zeros() as usize;
          }
          cur += CHUNK;
        }

        if cur == len {
          return len;
        }

        let overlap_start = len - CHUNK;
        let chunk = unsafe { _mm_loadu_si128(ptr.add(overlap_start) as *const __m128i) };
        let bits = unsafe { _mm_movemask_epi8(mask(chunk)) } as u32;
        let already = cur - overlap_start;
        let scan_mask = (!0u32) << already;
        let non_match = (!bits) & scan_mask & 0xFFFF;
        if non_match != 0 {
          overlap_start + non_match.trailing_zeros() as usize
        } else {
          len
        }
      }

      #[cfg(target_arch = "x86_64")]
      #[target_feature(enable = "avx2")]
      unsafe fn avx2_path(input: &[u8]) -> usize {
        use $crate::__macro::{
          AVX2_CHUNK_SIZE as CHUNK,
          __m256i, _mm256_and_si256, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_min_epu8,
          _mm256_movemask_epi8, _mm256_or_si256, _mm256_set1_epi8, _mm256_setzero_si256,
          _mm256_sub_epi8,
        };

        #[target_feature(enable = "avx2")]
        unsafe fn mask256(chunk: __m256i) -> __m256i {
          let mut acc = unsafe { _mm256_setzero_si256() };
          $( $(
            acc = unsafe {
              _mm256_or_si256(acc, _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8($byte as i8)))
            };
          )+ )?
          $( $(
            acc = unsafe {
              let x = _mm256_sub_epi8(chunk, _mm256_set1_epi8($lo as i8));
              let lim = _mm256_set1_epi8(($hi as u8).wrapping_sub($lo as u8) as i8);
              _mm256_or_si256(acc, _mm256_cmpeq_epi8(x, _mm256_min_epu8(x, lim)))
            };
          )+ )?
          let _ = chunk;
          acc
        }

        let len = input.len();
        if len < CHUNK {
          return prefix_len_scalar(input);
        }
        let ptr = input.as_ptr();
        let first = prefix_len_scalar(&input[..CHUNK]);
        if first != CHUNK {
          return first;
        }
        let mut cur = CHUNK;

        while cur + 2 * CHUNK <= len {
          let c0 = unsafe { _mm256_loadu_si256(ptr.add(cur) as *const __m256i) };
          let c1 = unsafe { _mm256_loadu_si256(ptr.add(cur + CHUNK) as *const __m256i) };
          let m0 = unsafe { mask256(c0) };
          let m1 = unsafe { mask256(c1) };
          let combined = unsafe { _mm256_movemask_epi8(_mm256_and_si256(m0, m1)) } as u32;
          if combined != !0u32 {
            let b0 = unsafe { _mm256_movemask_epi8(m0) } as u32;
            if b0 != !0u32 {
              return cur + (!b0).trailing_zeros() as usize;
            }
            let b1 = unsafe { _mm256_movemask_epi8(m1) } as u32;
            return cur + CHUNK + (!b1).trailing_zeros() as usize;
          }
          cur += 2 * CHUNK;
        }

        while cur + CHUNK <= len {
          let chunk = unsafe { _mm256_loadu_si256(ptr.add(cur) as *const __m256i) };
          let bits = unsafe { _mm256_movemask_epi8(mask256(chunk)) } as u32;
          if bits != !0u32 {
            return cur + (!bits).trailing_zeros() as usize;
          }
          cur += CHUNK;
        }

        if cur == len {
          return len;
        }

        let overlap_start = len - CHUNK;
        let chunk = unsafe { _mm256_loadu_si256(ptr.add(overlap_start) as *const __m256i) };
        let bits = unsafe { _mm256_movemask_epi8(mask256(chunk)) } as u32;
        let already = cur - overlap_start;
        let scan_mask = (!0u32) << already;
        let non_match = (!bits) & scan_mask;
        if non_match != 0 {
          overlap_start + non_match.trailing_zeros() as usize
        } else {
          len
        }
      }

      #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
      fn simd128_path(input: &[u8]) -> usize {
        use $crate::__macro::{
          SSE_CHUNK_SIZE as CHUNK,
          i8x16_bitmask, i8x16_eq, i8x16_splat, i8x16_sub, u8x16_lt, u8x16_splat,
          v128, v128_and, v128_load, v128_or,
        };

        fn mask_s128(chunk: v128) -> v128 {
          let mut acc = u8x16_splat(0);
          $( $(
            acc = v128_or(acc, i8x16_eq(chunk, i8x16_splat($byte as i8)));
          )+ )?
          $( $(
            {
              let x = i8x16_sub(chunk, i8x16_splat($lo as i8));
              let in_range =
                u8x16_lt(x, u8x16_splat(($hi as u8).wrapping_sub($lo as u8).wrapping_add(1)));
              acc = v128_or(acc, in_range);
            }
          )+ )?
          let _ = chunk;
          acc
        }

        let len = input.len();
        if len < CHUNK {
          return prefix_len_scalar(input);
        }
        let ptr = input.as_ptr();
        let first = prefix_len_scalar(&input[..CHUNK]);
        if first != CHUNK {
          return first;
        }
        let mut cur = CHUNK;

        while cur + 2 * CHUNK <= len {
          let c0 = unsafe { v128_load(ptr.add(cur) as *const v128) };
          let c1 = unsafe { v128_load(ptr.add(cur + CHUNK) as *const v128) };
          let m0 = mask_s128(c0);
          let m1 = mask_s128(c1);
          let combined = i8x16_bitmask(v128_and(m0, m1)) as u32;
          if combined != 0xFFFF {
            let b0 = i8x16_bitmask(m0) as u32;
            if b0 != 0xFFFF {
              return cur + ((!b0) & 0xFFFF).trailing_zeros() as usize;
            }
            let b1 = i8x16_bitmask(m1) as u32;
            return cur + CHUNK + ((!b1) & 0xFFFF).trailing_zeros() as usize;
          }
          cur += 2 * CHUNK;
        }

        while cur + CHUNK <= len {
          let chunk = unsafe { v128_load(ptr.add(cur) as *const v128) };
          let bits = i8x16_bitmask(mask_s128(chunk)) as u32;
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
        let bits = i8x16_bitmask(mask_s128(chunk)) as u32;
        let already = cur - overlap_start;
        let scan_mask = (!0u32) << already;
        let non_match = (!bits) & scan_mask & 0xFFFF;
        if non_match != 0 {
          overlap_start + non_match.trailing_zeros() as usize
        } else {
          len
        }
      }
    }
  };
}
