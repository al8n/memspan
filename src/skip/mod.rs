use crate::Needles;

#[cfg(target_arch = "aarch64")]
use crate::utils::neon_available;

#[cfg(target_arch = "aarch64")]
pub(crate) mod neon;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::utils::sse42_available;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) mod sse42;

#[cfg(target_arch = "x86_64")]
use crate::utils::{avx2_available, avx512bw_available};

#[cfg(target_arch = "x86_64")]
pub(crate) mod avx2;

#[cfg(target_arch = "x86_64")]
pub(crate) mod avx512;

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
pub(crate) mod simd128;

// ── scalar predicates ────────────────────────────────────────────────────────

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_binary_digit(byte: u8) -> bool {
  byte == b'0' || byte == b'1'
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_octal_digit(byte: u8) -> bool {
  matches!(byte, b'0'..=b'7')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_digit(byte: u8) -> bool {
  byte.is_ascii_digit()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_hex_digit(byte: u8) -> bool {
  let lower = byte | 0x20;
  is_digit(byte) || matches!(lower, b'a'..=b'f')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_alpha(byte: u8) -> bool {
  (byte | 0x20).is_ascii_lowercase()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_alphanumeric(byte: u8) -> bool {
  is_alpha(byte) || is_digit(byte)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_ident_start(byte: u8) -> bool {
  is_alpha(byte) || byte == b'_'
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_ident(byte: u8) -> bool {
  is_alphanumeric(byte) || byte == b'_'
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_whitespace(byte: u8) -> bool {
  matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_by(input: &[u8], is_match: impl Fn(u8) -> bool) -> usize {
  input
    .iter()
    .position(|&byte| !is_match(byte))
    .unwrap_or(input.len())
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_binary(input: &[u8]) -> usize {
  prefix_len_by(input, is_binary_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_octal_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_octal_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_hex_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_hex_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_alpha(input: &[u8]) -> usize {
  prefix_len_by(input, is_alpha)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_alphanumeric(input: &[u8]) -> usize {
  prefix_len_by(input, is_alphanumeric)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_ident_start(input: &[u8]) -> usize {
  prefix_len_by(input, is_ident_start)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_ident(input: &[u8]) -> usize {
  prefix_len_by(input, is_ident)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_whitespace(input: &[u8]) -> usize {
  prefix_len_by(input, is_whitespace)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_lower(byte: u8) -> bool {
  byte.is_ascii_lowercase()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_upper(byte: u8) -> bool {
  byte.is_ascii_uppercase()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_ascii_byte(byte: u8) -> bool {
  byte.is_ascii()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_non_ascii(byte: u8) -> bool {
  !byte.is_ascii()
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_ascii_graphic(byte: u8) -> bool {
  matches!(byte, 0x21..=0x7E)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn is_ascii_control(byte: u8) -> bool {
  matches!(byte, 0x00..=0x1F | 0x7F)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_lower(input: &[u8]) -> usize {
  prefix_len_by(input, is_lower)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_upper(input: &[u8]) -> usize {
  prefix_len_by(input, is_upper)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_ascii(input: &[u8]) -> usize {
  prefix_len_by(input, is_ascii_byte)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_non_ascii(input: &[u8]) -> usize {
  prefix_len_by(input, is_non_ascii)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_ascii_graphic(input: &[u8]) -> usize {
  prefix_len_by(input, is_ascii_graphic)
}

#[cfg_attr(not(tarpaulin), inline(always))]
pub(crate) fn prefix_len_ascii_control(input: &[u8]) -> usize {
  prefix_len_by(input, is_ascii_control)
}

// ── x86/x86_64 dispatch helpers ──────────────────────────────────────────────

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(not(tarpaulin), inline(always))]
fn dispatch_skip_until_x86<Nd: Needles>(input: &[u8], needles: Nd) -> Option<usize> {
  #[cfg(target_arch = "x86_64")]
  if avx512bw_available() {
    if input.len() >= 64 {
      return unsafe { avx512::skip_until(input, needles) };
    }
    if avx2_available() && input.len() >= 32 {
      return unsafe { avx2::skip_until(input, needles) };
    }
  } else if avx2_available() && input.len() >= 32 {
    return unsafe { avx2::skip_until(input, needles) };
  }
  if sse42_available() {
    return unsafe { sse42::skip_until(input, needles) };
  }
  needles.tail_find(input)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(not(tarpaulin), inline(always))]
fn dispatch_skip_while_x86<Nd: Needles>(input: &[u8], needles: Nd) -> usize {
  #[cfg(target_arch = "x86_64")]
  if avx512bw_available() {
    if input.len() >= 64 {
      return unsafe { avx512::skip_while(input, needles) };
    }
    if avx2_available() && input.len() >= 32 {
      return unsafe { avx2::skip_while(input, needles) };
    }
  } else if avx2_available() && input.len() >= 32 {
    return unsafe { avx2::skip_while(input, needles) };
  }
  if sse42_available() {
    return unsafe { sse42::skip_while(input, needles) };
  }
  needles.prefix_len(input)
}

fn count_matches_scalar<Nd: Needles>(input: &[u8], needles: Nd) -> usize {
  input
    .iter()
    .filter(|&&b| needles.tail_find(core::slice::from_ref(&b)).is_some())
    .count()
}

fn find_last_scalar<Nd: Needles>(input: &[u8], needles: Nd) -> Option<usize> {
  let mut last = None;
  for (i, &b) in input.iter().enumerate() {
    if needles.tail_find(core::slice::from_ref(&b)).is_some() {
      last = Some(i);
    }
  }
  last
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(not(tarpaulin), inline(always))]
fn dispatch_count_matches_x86<Nd: Needles>(input: &[u8], needles: Nd) -> usize {
  #[cfg(target_arch = "x86_64")]
  {
    if avx512bw_available() {
      return unsafe { avx512::count_matches(input, needles) };
    }
    if avx2_available() {
      return unsafe { avx2::count_matches(input, needles) };
    }
  }
  if sse42_available() {
    return unsafe { sse42::count_matches(input, needles) };
  }
  count_matches_scalar(input, needles)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(not(tarpaulin), inline(always))]
fn dispatch_find_last_x86<Nd: Needles>(input: &[u8], needles: Nd) -> Option<usize> {
  #[cfg(target_arch = "x86_64")]
  {
    if avx512bw_available() {
      return unsafe { avx512::find_last(input, needles) };
    }
    if avx2_available() {
      return unsafe { avx2::find_last(input, needles) };
    }
  }
  if sse42_available() {
    return unsafe { sse42::find_last(input, needles) };
  }
  find_last_scalar(input, needles)
}

/// Selects the right SIMD tier for a specialized ASCII-class function on x86/x86_64.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
macro_rules! x86_class_dispatch {
  ($input:expr, $scalar:ident, $sse42_fn:path, $avx2_fn:path, $avx512_fn:path) => {{
    #[cfg(target_arch = "x86_64")]
    if avx512bw_available() {
      if $input.len() >= 64 {
        return unsafe { $avx512_fn($input) };
      }
      if avx2_available() && $input.len() >= 32 {
        return unsafe { $avx2_fn($input) };
      }
    } else if avx2_available() && $input.len() >= 32 {
      return unsafe { $avx2_fn($input) };
    }
    if sse42_available() {
      return unsafe { $sse42_fn($input) };
    }
    $scalar($input)
  }};
}

/// Returns the index of the first byte in `input` that matches any of `needles`.
///
/// Dispatches to AVX-512BW / AVX2 / SSE4.2 (x86_64), NEON (aarch64), or
/// WASM SIMD128 (wasm32) depending on what the CPU supports at runtime.
/// Falls back to a scalar loop on unsupported targets or when SIMD is
/// disabled via `memspan_force_scalar`.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  cfg_select! {
    target_arch = "aarch64" => {
      if needles.needle_count() == 0 { return None; }
      if input.len() < 16 { return needles.tail_find(input); }
      if neon_available() { return neon::skip_until(input, needles); }
      needles.tail_find(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      if needles.needle_count() == 0 { return None; }
      if input.len() < 16 { return needles.tail_find(input); }
      dispatch_skip_until_x86(input, needles)
    }
    target_arch = "wasm32" => {
      if needles.needle_count() == 0 { return None; }
      if input.len() < 16 { return needles.tail_find(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_until(input, needles); }
      needles.tail_find(input)
    }
    _ => {
      if needles.needle_count() == 0 { return None; }
      needles.tail_find(input)
    }
  }
}

/// Returns the number of leading bytes in `input` that match any of `needles`.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  cfg_select! {
    target_arch = "aarch64" => {
      let count = needles.needle_count();
      if count <= 1 || input.len() < 32 { return needles.prefix_len(input); }
      if neon_available() { return neon::skip_while(input, needles); }
      needles.prefix_len(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      let count = needles.needle_count();
      if count <= 1 || input.len() < 16 { return needles.prefix_len(input); }
      dispatch_skip_while_x86(input, needles)
    }
    target_arch = "wasm32" => {
      let count = needles.needle_count();
      if count <= 1 || input.len() < 16 { return needles.prefix_len(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_while(input, needles); }
      needles.prefix_len(input)
    }
    _ => {
      needles.prefix_len(input)
    }
  }
}

/// Returns the length of the leading ASCII binary-digit prefix (`0` or `1`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_binary(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_binary(input); }
      if neon_available() { return neon::skip_binary(input); }
      prefix_len_binary(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_binary, sse42::skip_binary, avx2::skip_binary, avx512::skip_binary)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_binary(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_binary(input); }
      prefix_len_binary(input)
    }
    _ => { prefix_len_binary(input) }
  }
}

/// Returns the length of the leading ASCII decimal-digit prefix (`0..=9`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_digits(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_digits(input); }
      if neon_available() { return neon::skip_digits(input); }
      prefix_len_digits(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_digits, sse42::skip_digits, avx2::skip_digits, avx512::skip_digits)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_digits(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_digits(input); }
      prefix_len_digits(input)
    }
    _ => { prefix_len_digits(input) }
  }
}

/// Returns the length of the leading ASCII hexadecimal-digit prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_hex_digits(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_hex_digits(input); }
      if neon_available() { return neon::skip_hex_digits(input); }
      prefix_len_hex_digits(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_hex_digits, sse42::skip_hex_digits, avx2::skip_hex_digits, avx512::skip_hex_digits)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_hex_digits(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_hex_digits(input); }
      prefix_len_hex_digits(input)
    }
    _ => { prefix_len_hex_digits(input) }
  }
}

/// Returns the length of the leading ASCII octal-digit prefix (`0..=7`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_octal_digits(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_octal_digits(input); }
      if neon_available() { return neon::skip_octal_digits(input); }
      prefix_len_octal_digits(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_octal_digits, sse42::skip_octal_digits, avx2::skip_octal_digits, avx512::skip_octal_digits)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_octal_digits(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_octal_digits(input); }
      prefix_len_octal_digits(input)
    }
    _ => { prefix_len_octal_digits(input) }
  }
}

/// Returns the length of the leading ASCII whitespace prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_whitespace(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_whitespace(input); }
      if neon_available() { return neon::skip_whitespace(input); }
      prefix_len_whitespace(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_whitespace, sse42::skip_whitespace, avx2::skip_whitespace, avx512::skip_whitespace)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_whitespace(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_whitespace(input); }
      prefix_len_whitespace(input)
    }
    _ => { prefix_len_whitespace(input) }
  }
}

/// Returns the length of the leading ASCII alphabetic prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_alpha(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_alpha(input); }
      if neon_available() { return neon::skip_alpha(input); }
      prefix_len_alpha(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_alpha, sse42::skip_alpha, avx2::skip_alpha, avx512::skip_alpha)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_alpha(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_alpha(input); }
      prefix_len_alpha(input)
    }
    _ => { prefix_len_alpha(input) }
  }
}

/// Returns the length of the leading ASCII alphanumeric prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_alphanumeric(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_alphanumeric(input); }
      if neon_available() { return neon::skip_alphanumeric(input); }
      prefix_len_alphanumeric(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_alphanumeric, sse42::skip_alphanumeric, avx2::skip_alphanumeric, avx512::skip_alphanumeric)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_alphanumeric(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_alphanumeric(input); }
      prefix_len_alphanumeric(input)
    }
    _ => { prefix_len_alphanumeric(input) }
  }
}

/// Returns the length of the leading C-style identifier-start prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_ident_start(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_ident_start(input); }
      if neon_available() { return neon::skip_ident_start(input); }
      prefix_len_ident_start(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_ident_start, sse42::skip_ident_start, avx2::skip_ident_start, avx512::skip_ident_start)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_ident_start(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_ident_start(input); }
      prefix_len_ident_start(input)
    }
    _ => { prefix_len_ident_start(input) }
  }
}

/// Returns the length of the leading C-style identifier-continuation prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_ident(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_ident(input); }
      if neon_available() { return neon::skip_ident(input); }
      prefix_len_ident(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_ident, sse42::skip_ident, avx2::skip_ident, avx512::skip_ident)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_ident(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_ident(input); }
      prefix_len_ident(input)
    }
    _ => { prefix_len_ident(input) }
  }
}

/// Returns the length of the leading ASCII lowercase prefix (`a..=z`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_lower(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_lower(input); }
      if neon_available() { return neon::skip_lower(input); }
      prefix_len_lower(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_lower, sse42::skip_lower, avx2::skip_lower, avx512::skip_lower)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_lower(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_lower(input); }
      prefix_len_lower(input)
    }
    _ => { prefix_len_lower(input) }
  }
}

/// Returns the length of the leading ASCII uppercase prefix (`A..=Z`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_upper(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_upper(input); }
      if neon_available() { return neon::skip_upper(input); }
      prefix_len_upper(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_upper, sse42::skip_upper, avx2::skip_upper, avx512::skip_upper)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_upper(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_upper(input); }
      prefix_len_upper(input)
    }
    _ => { prefix_len_upper(input) }
  }
}

/// Returns the length of the leading ASCII byte prefix (bytes `0x00..=0x7F`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_ascii(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_ascii(input); }
      if neon_available() { return neon::skip_ascii(input); }
      prefix_len_ascii(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_ascii, sse42::skip_ascii, avx2::skip_ascii, avx512::skip_ascii)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_ascii(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_ascii(input); }
      prefix_len_ascii(input)
    }
    _ => { prefix_len_ascii(input) }
  }
}

/// Returns the length of the leading non-ASCII byte prefix (bytes `0x80..=0xFF`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_non_ascii(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_non_ascii(input); }
      if neon_available() { return neon::skip_non_ascii(input); }
      prefix_len_non_ascii(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_non_ascii, sse42::skip_non_ascii, avx2::skip_non_ascii, avx512::skip_non_ascii)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_non_ascii(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_non_ascii(input); }
      prefix_len_non_ascii(input)
    }
    _ => { prefix_len_non_ascii(input) }
  }
}

/// Returns the length of the leading ASCII graphic character prefix (`0x21..=0x7E`,
/// i.e. printable non-space characters).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_ascii_graphic(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_ascii_graphic(input); }
      if neon_available() { return neon::skip_ascii_graphic(input); }
      prefix_len_ascii_graphic(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_ascii_graphic, sse42::skip_ascii_graphic, avx2::skip_ascii_graphic, avx512::skip_ascii_graphic)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_ascii_graphic(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_ascii_graphic(input); }
      prefix_len_ascii_graphic(input)
    }
    _ => { prefix_len_ascii_graphic(input) }
  }
}

/// Returns the length of the leading ASCII control character prefix
/// (`0x00..=0x1F` and `0x7F`).
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_ascii_control(input: &[u8]) -> usize {
  cfg_select! {
    target_arch = "aarch64" => {
      if input.len() < 32 { return prefix_len_ascii_control(input); }
      if neon_available() { return neon::skip_ascii_control(input); }
      prefix_len_ascii_control(input)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      x86_class_dispatch!(input, prefix_len_ascii_control, sse42::skip_ascii_control, avx2::skip_ascii_control, avx512::skip_ascii_control)
    }
    target_arch = "wasm32" => {
      if input.len() < 16 { return prefix_len_ascii_control(input); }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::skip_ascii_control(input); }
      prefix_len_ascii_control(input)
    }
    _ => { prefix_len_ascii_control(input) }
  }
}

/// Returns the number of bytes in `input` that match any of `needles`.
///
/// Unlike [`skip_until`] this never returns early — every byte is examined and
/// matching bytes are counted via SIMD popcount (`count_ones` on the bitmask).
/// Useful for counting newlines to build line-number tables, counting delimiter
/// occurrences, etc.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn count_matches<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  cfg_select! {
    target_arch = "aarch64" => {
      if neon_available() { return neon::count_matches(input, needles); }
      count_matches_scalar(input, needles)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      dispatch_count_matches_x86(input, needles)
    }
    target_arch = "wasm32" => {
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::count_matches(input, needles); }
      count_matches_scalar(input, needles)
    }
    _ => { count_matches_scalar(input, needles) }
  }
}

/// Returns the index of the **last** byte in `input` that matches any of
/// `needles`, or `None` if no byte matches.
///
/// Scans the entire input front-to-back, accumulating the rightmost match
/// position using SIMD bitmask `leading_zeros` to find the last set bit in
/// each chunk. The SIMD backends are the same as those used by [`skip_until`].
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn find_last<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  cfg_select! {
    target_arch = "aarch64" => {
      if needles.needle_count() == 0 { return None; }
      if neon_available() { return neon::find_last(input, needles); }
      find_last_scalar(input, needles)
    }
    any(target_arch = "x86", target_arch = "x86_64") => {
      if needles.needle_count() == 0 { return None; }
      dispatch_find_last_x86(input, needles)
    }
    target_arch = "wasm32" => {
      if needles.needle_count() == 0 { return None; }
      #[cfg(target_feature = "simd128")]
      if crate::utils::simd128_available() { return simd128::find_last(input, needles); }
      find_last_scalar(input, needles)
    }
    _ => {
      if needles.needle_count() == 0 { return None; }
      find_last_scalar(input, needles)
    }
  }
}

/// Returns the number of bytes before the first `\n`, or `input.len()` if
/// there is no newline. Equivalent to `skip_until(input, b'\n').unwrap_or(input.len())`.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn skip_until_newline(input: &[u8]) -> usize {
  skip_until(input, b'\n').unwrap_or(input.len())
}

/// Returns `true` if any byte in `input` matches any of `needles`.
///
/// This is `skip_until(input, needles).is_some()` with a cleaner call-site name.
#[cfg_attr(not(tarpaulin), inline(always))]
pub fn contains_any<Nd: Needles>(input: &[u8], needles: Nd) -> bool {
  skip_until(input, needles).is_some()
}
