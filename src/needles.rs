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
    #[cfg_attr(not(tarpaulin), inline(always))]
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

tail_find_fixed!(tail_find4, a, b, c, d);
tail_find_fixed!(tail_find5, a, b, c, d, e);
tail_find_fixed!(tail_find6, a, b, c, d, e, f);
tail_find_fixed!(tail_find7, a, b, c, d, e, f, g);
tail_find_fixed!(tail_find8, a, b, c, d, e, f, g, h);

macro_rules! prefix_len_fixed {
  ($name:ident, $($needle:ident),+) => {
    #[cfg_attr(not(tarpaulin), inline(always))]
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
  /// The dispatcher uses this to pick between [`memchr`]-family scalar paths
  /// (which already saturate SIMD for 1–3 needles) and the wider SIMD loop.
  fn needle_count(&self) -> usize;

  /// Whether the needles are empty (e.g., an empty slice).
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn is_empty(&self) -> bool {
    self.needle_count() == 0
  }

  /// Find the first occurrence of any of the needles in `tail`, returning
  /// the index (relative to the start of `tail`) if found.
  ///
  /// This is the scalar primitive that backs [`crate::skip::skip_until`]'s
  /// fallback path. For 1–3 needles it dispatches to the [`memchr`] family,
  /// which is already SIMD-saturated; for 4–8 needles it uses an unrolled
  /// per-byte loop that LLVM auto-vectorizes well.
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

  /// Returns a Neon byte mask where matching lanes are `0xFF` and non-matching
  /// lanes are `0x00`.
  #[cfg(target_arch = "aarch64")]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t;
}

impl<T> Needles for &T
where
  T: Needles + ?Sized,
{
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn needle_count(&self) -> usize {
    (**self).needle_count()
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    (**self).tail_find(tail)
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn prefix_len(&self, input: &[u8]) -> usize {
    (**self).prefix_len(input)
  }

  #[cfg(target_arch = "aarch64")]
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    (**self).eq_any_mask_neon(chunk)
  }
}

impl Needles for u8 {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn needle_count(&self) -> usize {
    1
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    memchr::memchr(*self, tail)
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn prefix_len(&self, input: &[u8]) -> usize {
    prefix_len1(input, *self)
  }

  #[cfg(target_arch = "aarch64")]
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    let target = unsafe { core::arch::aarch64::vdupq_n_u8(*self) };
    unsafe { core::arch::aarch64::vceqq_u8(chunk, target) }
  }
}

impl Needles for [u8] {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn needle_count(&self) -> usize {
    self.len()
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    match self {
      [] => None,
      [a] => memchr::memchr(*a, tail),
      [a, b] => memchr::memchr2(*a, *b, tail),
      [a, b, c] => memchr::memchr3(*a, *b, *c, tail),
      [a, b, c, d] => tail_find4(tail, *a, *b, *c, *d),
      [a, b, c, d, e] => tail_find5(tail, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => tail_find6(tail, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => tail_find7(tail, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => tail_find8(tail, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => tail.iter().position(|byte| self.contains(byte)),
    }
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
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
  #[cfg_attr(not(tarpaulin), inline(always))]
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
}

impl<const N: usize> Needles for [u8; N] {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn needle_count(&self) -> usize {
    N
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
  fn tail_find(&self, tail: &[u8]) -> Option<usize> {
    match self.as_slice() {
      [] => None,
      [a] => memchr::memchr(*a, tail),
      [a, b] => memchr::memchr2(*a, *b, tail),
      [a, b, c] => memchr::memchr3(*a, *b, *c, tail),
      [a, b, c, d] => tail_find4(tail, *a, *b, *c, *d),
      [a, b, c, d, e] => tail_find5(tail, *a, *b, *c, *d, *e),
      [a, b, c, d, e, f] => tail_find6(tail, *a, *b, *c, *d, *e, *f),
      [a, b, c, d, e, f, g] => tail_find7(tail, *a, *b, *c, *d, *e, *f, *g),
      [a, b, c, d, e, f, g, h] => tail_find8(tail, *a, *b, *c, *d, *e, *f, *g, *h),
      _ => tail.iter().position(|byte| self.contains(byte)),
    }
  }

  #[cfg_attr(not(tarpaulin), inline(always))]
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
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq_any_mask_neon(
    &self,
    chunk: core::arch::aarch64::uint8x16_t,
  ) -> core::arch::aarch64::uint8x16_t {
    arch::aarch64::eq_any_mask_const(chunk, *self)
  }
}
