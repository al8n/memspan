#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]
#![deny(missing_docs)]

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc as std;

#[cfg(feature = "std")]
extern crate std;

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
}

/// Define a custom `skip_*` function for an ASCII byte class, generating the
/// same scalar fallback + SIMD loop the built-in [`skip::skip_digits`],
/// [`skip::skip_whitespace`], etc. use internally.
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
///     pub fn skip_my_class
///         , bytes  = [b' ', b'\t']           // optional
///         , ranges = [b'a'..=b'z', 0x30..=0x39]   // optional
///     ;
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
///     pub fn skip_ws_and_comma, bytes = [b' ', b'\t', b'\r', b'\n', b','];
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
///     pub fn skip_lowercase, ranges = [b'a'..=b'z'];
/// }
///
/// assert_eq!(skip_lowercase(b"abcXYZ"), 3);
/// ```
///
/// A mixed class — alphanumeric plus a few punctuation bytes:
///
/// ```
/// skipchr::skip_class! {
///     pub fn skip_punct_ident,
///         bytes  = [b'_', b'-', b'!', b'?'],
///         ranges = [b'a'..=b'z', b'A'..=b'Z', b'0'..=b'9'];
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
    $(, bytes = [$($byte:expr),+ $(,)?])?
    $(, ranges = [$($lo:literal ..= $hi:literal),+ $(,)?])?
    $(,)?
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
    }
  };
}
