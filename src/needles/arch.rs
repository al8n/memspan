/// Needles extension trait for aarch64.
#[cfg(target_arch = "aarch64")]
#[cfg_attr(docsrs, doc(cfg(target_arch = "aarch64")))]
pub mod aarch64;

/// Needles extension trait for x86 / x86_64.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(docsrs, doc(cfg(any(target_arch = "x86", target_arch = "x86_64"))))]
pub mod x86;

/// Needles extension trait for wasm32.
#[cfg(target_arch = "wasm32")]
#[cfg_attr(docsrs, doc(cfg(target_arch = "wasm32")))]
pub mod wasm32;
