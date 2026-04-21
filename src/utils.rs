// ── aarch64 / NEON ──────────────────────────────────────────────────────────

/// NEON availability on aarch64 — std variant (runtime detection).
#[cfg(all(target_arch = "aarch64", feature = "std"))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn neon_available() -> bool {
  if cfg!(memspan_force_scalar) {
    return false;
  }
  std::arch::is_aarch64_feature_detected!("neon")
}

/// NEON availability on aarch64 — no-std variant (compile-time).
#[cfg(all(target_arch = "aarch64", not(feature = "std")))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub const fn neon_available() -> bool {
  !cfg!(memspan_force_scalar) && cfg!(target_feature = "neon")
}

// ── x86 / x86_64 ────────────────────────────────────────────────────────────

/// SSE4.2 availability — std variant (runtime detection).
///
/// SSE4.2 is the baseline for the 128-bit x86 SIMD path; it implies SSE4.1,
/// SSSE3, SSE3, and SSE2. Nearly all x86-64 CPUs since 2008 have it.
#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), feature = "std"))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn sse42_available() -> bool {
  if cfg!(memspan_force_scalar) || cfg!(memspan_disable_sse42) {
    return false;
  }
  std::arch::is_x86_feature_detected!("sse4.2")
}

/// SSE4.2 availability — no-std variant (compile-time).
#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), not(feature = "std")))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub const fn sse42_available() -> bool {
  !cfg!(memspan_force_scalar) && !cfg!(memspan_disable_sse42) && cfg!(target_feature = "sse4.2")
}

/// AVX2 availability — std variant (runtime detection).
#[cfg(all(target_arch = "x86_64", feature = "std"))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn avx2_available() -> bool {
  if cfg!(memspan_force_scalar) || cfg!(memspan_disable_avx2) {
    return false;
  }
  std::arch::is_x86_feature_detected!("avx2")
}

/// AVX2 availability — no-std variant (compile-time).
#[cfg(all(target_arch = "x86_64", not(feature = "std")))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub const fn avx2_available() -> bool {
  !cfg!(memspan_force_scalar) && !cfg!(memspan_disable_avx2) && cfg!(target_feature = "avx2")
}

/// AVX-512BW availability — std variant (runtime detection).
///
/// AVX-512BW is required for 512-bit byte-level operations
/// (`_mm512_cmpeq_epi8_mask`, `_mm512_cmple_epu8_mask`, …).
#[cfg(all(target_arch = "x86_64", feature = "std"))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn avx512bw_available() -> bool {
  if cfg!(memspan_force_scalar) || cfg!(memspan_disable_avx512) {
    return false;
  }
  std::arch::is_x86_feature_detected!("avx512bw")
}

/// AVX-512BW availability — no-std variant (compile-time).
#[cfg(all(target_arch = "x86_64", not(feature = "std")))]
#[cfg_attr(not(tarpaulin), inline(always))]
pub const fn avx512bw_available() -> bool {
  !cfg!(memspan_force_scalar) && !cfg!(memspan_disable_avx512) && cfg!(target_feature = "avx512bw")
}

// ── wasm32 / SIMD128 ─────────────────────────────────────────────────────────

/// WASM SIMD128 availability (compile-time only; WASM has no runtime CPUID).
#[cfg(target_arch = "wasm32")]
#[cfg_attr(not(tarpaulin), inline(always))]
pub const fn simd128_available() -> bool {
  !cfg!(memspan_force_scalar) && !cfg!(memspan_disable_simd128) && cfg!(target_feature = "simd128")
}
