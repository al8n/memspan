use crate::Needles;

#[cfg(target_arch = "aarch64")]
use crate::utils::*;

#[cfg(target_arch = "aarch64")]
pub(crate) mod neon;

/// Width of the NEON chunk processed per SIMD iteration. Inputs shorter than
/// this fall through to the scalar path.
#[cfg(target_arch = "aarch64")]
const NEON_CHUNK_SIZE: usize = 16;

/// Minimum input length where the NEON `skip_while` loop reliably beats the
/// auto-vectorized scalar `prefix_len`. Determined empirically (see the
/// `skip_while/micro/full_match` bench): for 5 needles the scalar path holds
/// at ~3 GiB/s through 17–24 bytes and only loses at ~32 bytes once the SIMD
/// loop has more than one chunk to amortize the probe + dispatch cost.
#[cfg(target_arch = "aarch64")]
const SKIP_WHILE_SIMD_THRESHOLD: usize = 2 * NEON_CHUNK_SIZE;

/// Maximum needle count handled by the `memchr` family. For 1–3 needles the
/// scalar path is already SIMD-saturating, so wrapping it in our own NEON loop
/// only adds dispatch overhead.
const MEMCHR_MAX_NEEDLES: usize = 3;

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
  // OR-with-0x20 case-folds A-Z to a-z and leaves a-z unchanged. Bytes
  // outside the alpha range case-fold to values that fall outside `a..=z`,
  // so a single bounded check covers both cases.
  let lower = byte | 0x20;
  lower.is_ascii_lowercase()
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
fn prefix_len_binary(input: &[u8]) -> usize {
  prefix_len_by(input, is_binary_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_octal_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_octal_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_hex_digits(input: &[u8]) -> usize {
  prefix_len_by(input, is_hex_digit)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_alpha(input: &[u8]) -> usize {
  prefix_len_by(input, is_alpha)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_alphanumeric(input: &[u8]) -> usize {
  prefix_len_by(input, is_alphanumeric)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_ident_start(input: &[u8]) -> usize {
  prefix_len_by(input, is_ident_start)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_ident(input: &[u8]) -> usize {
  prefix_len_by(input, is_ident)
}

#[cfg_attr(not(tarpaulin), inline(always))]
fn prefix_len_whitespace(input: &[u8]) -> usize {
  prefix_len_by(input, is_whitespace)
}

/// Returns the index of the first byte in `input` that matches any of `needles`.
///
/// Routing summary:
/// * `count == 0` → `None`.
/// * `count <= 3` → scalar [`Needles::tail_find`] (dispatches to
///   `memchr`/`memchr2`/`memchr3`, already SIMD-saturated).
/// * `input.len() < 16` → scalar; the SIMD path can't amortize over a single
///   chunk.
/// * Otherwise → NEON path with scalar probe + SIMD loop + overlap tail.
///
/// **Fast path for short inputs.** The dispatcher pays a few cycles per call
/// (length check, runtime feature check, function entry) which is invisible on
/// long inputs but visible in tight loops over very short slices. If you know
/// every call has `input.len() ≲ 32` bytes, calling [`Needles::tail_find`]
/// directly skips that overhead. For typical use (mixed input sizes, or any
/// input where you'd benefit from SIMD), prefer this dispatcher.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  let count = needles.needle_count();
  if count == 0 {
    return None;
  }

  if count <= MEMCHR_MAX_NEEDLES || input.len() < NEON_CHUNK_SIZE {
    return needles.tail_find(input);
  }

  if neon_available() {
    return neon::skip_until(input, needles);
  }

  needles.tail_find(input)
}

/// Returns the number of bytes from the start of `input` that match any of
/// `needles`.
///
/// Routing summary (thresholds picked from the `skip_while` bench suite):
/// * `count == 0` → returns 0 via [`Needles::prefix_len`].
/// * `count == 1` → scalar [`Needles::prefix_len`]. LLVM auto-vectorizes the
///   single-needle prefix scan to ~3 GiB/s, and that beats the NEON path on
///   tight scan-all loops (per-call dispatch overhead dominates). The trade-off
///   is that single-call full-buffer scans of one needle are capped at
///   ~3 GiB/s scalar throughput rather than the ~20 GiB/s the NEON loop could
///   reach; use a needle set with ≥2 entries if you need the SIMD path.
/// * `input.len() < SKIP_WHILE_SIMD_THRESHOLD` (32 bytes) → scalar. Below this
///   size the auto-vectorized scalar `prefix_len` is at least as fast as
///   probe + one SIMD chunk + overlap tail, and significantly faster between
///   17–24 bytes.
/// * Otherwise → NEON path with scalar probe + SIMD loop + overlap tail.
///
/// **Fast path for short inputs.** Inputs in the 16–32 byte range pay a
/// non-trivial wrapper overhead here (~1.7× slowdown vs. raw scalar) because
/// the dispatcher's body holds both branches and LLVM doesn't fully inline
/// `prefix_len` at the call site. If you call this in a tight loop with
/// known-short inputs, calling [`Needles::prefix_len`] directly recovers that
/// overhead. For general use prefer this dispatcher — at len ≥ 32 the NEON
/// path wins decisively (up to ~7× at 4 KB+).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  let count = needles.needle_count();
  if count <= 1 || input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return needles.prefix_len(input);
  }

  if neon_available() {
    return neon::skip_while(input, needles);
  }

  needles.prefix_len(input)
}

/// Returns the length of the leading ASCII binary-digit prefix (`0` or `1`).
///
/// This is a specialized [`skip_while`] for numeric lexers. On aarch64 it uses
/// a scalar short-input path and only enters NEON for longer prefixes, so the
/// common lexer case of tiny literals does not pay SIMD setup cost.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_binary(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_binary(input);
  }

  if neon_available() {
    return neon::skip_binary(input);
  }

  prefix_len_binary(input)
}

/// Returns the length of the leading ASCII decimal-digit prefix (`0..=9`).
///
/// This is range-based rather than implemented as ten separate needles. That
/// keeps the SIMD path to a couple of comparisons per chunk instead of ten
/// equality checks.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_digits(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_digits(input);
  }

  if neon_available() {
    return neon::skip_digits(input);
  }

  prefix_len_digits(input)
}

/// Returns the length of the leading ASCII hexadecimal-digit prefix
/// (`0..=9`, `a..=f`, or `A..=F`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_hex_digits(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_hex_digits(input);
  }

  if neon_available() {
    return neon::skip_hex_digits(input);
  }

  prefix_len_hex_digits(input)
}

/// Returns the length of the leading ASCII octal-digit prefix (`0..=7`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_octal_digits(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_octal_digits(input);
  }

  if neon_available() {
    return neon::skip_octal_digits(input);
  }

  prefix_len_octal_digits(input)
}

/// Returns the length of the leading ASCII whitespace prefix
/// (`' '`, `'\t'`, `'\n'`, `'\r'`).
///
/// Equivalent to [`skip_while`] called with the four-byte needle set, but
/// avoids the trait dispatch overhead and routes the SIMD branch through a
/// direct `vceqq` × 4 + `vorrq` × 3 mask.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_whitespace(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_whitespace(input);
  }

  if neon_available() {
    return neon::skip_whitespace(input);
  }

  prefix_len_whitespace(input)
}

/// Returns the length of the leading ASCII alphabetic prefix
/// (`a..=z`, `A..=F`).
///
/// Uses the `OR 0x20` case-fold trick so the SIMD path collapses to a single
/// `range_mask`: 3 ops per chunk (one `vorrq`, one `vsubq`, one `vcltq`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_alpha(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_alpha(input);
  }

  if neon_available() {
    return neon::skip_alpha(input);
  }

  prefix_len_alpha(input)
}

/// Returns the length of the leading ASCII alphanumeric prefix
/// (`a..=z`, `A..=Z`, `0..=9`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_alphanumeric(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_alphanumeric(input);
  }

  if neon_available() {
    return neon::skip_alphanumeric(input);
  }

  prefix_len_alphanumeric(input)
}

/// Returns the length of the leading C-style identifier-start prefix
/// (`a..=z`, `A..=Z`, `_`).
///
/// Common as the first character of an identifier in many languages
/// (C, Rust, Python, JavaScript, JSON keys, GraphQL field names, ...).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_ident_start(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_ident_start(input);
  }

  if neon_available() {
    return neon::skip_ident_start(input);
  }

  prefix_len_ident_start(input)
}

/// Returns the length of the leading C-style identifier-continuation prefix
/// (`a..=z`, `A..=Z`, `0..=9`, `_`).
///
/// Use [`skip_ident_start`] for the first byte and this for the rest:
///
/// ```ignore
/// let head = skip_ident_start(input);
/// if head == 0 { /* not an identifier */ }
/// let total = head + skip_ident(&input[head..]);
/// ```
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(target_arch = "aarch64")]
pub fn skip_ident(input: &[u8]) -> usize {
  if input.len() < SKIP_WHILE_SIMD_THRESHOLD {
    return prefix_len_ident(input);
  }

  if neon_available() {
    return neon::skip_ident(input);
  }

  prefix_len_ident(input)
}

/// Returns the index of the first byte in `input` that matches any of `needles`.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_until<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  if needles.needle_count() == 0 {
    return None;
  }

  needles.tail_find(input)
}

/// Returns the number of bytes from the start of `input` that match any of
/// `needles`.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  needles.prefix_len(input)
}

/// Returns the length of the leading ASCII binary-digit prefix (`0` or `1`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_binary(input: &[u8]) -> usize {
  prefix_len_binary(input)
}

/// Returns the length of the leading ASCII decimal-digit prefix (`0..=9`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_digits(input: &[u8]) -> usize {
  prefix_len_digits(input)
}

/// Returns the length of the leading ASCII hexadecimal-digit prefix
/// (`0..=9`, `a..=f`, or `A..=F`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_hex_digits(input: &[u8]) -> usize {
  prefix_len_hex_digits(input)
}

/// Returns the length of the leading ASCII octal-digit prefix (`0..=7`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_octal_digits(input: &[u8]) -> usize {
  prefix_len_octal_digits(input)
}

/// Returns the length of the leading ASCII whitespace prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_whitespace(input: &[u8]) -> usize {
  prefix_len_whitespace(input)
}

/// Returns the length of the leading ASCII alphabetic prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_alpha(input: &[u8]) -> usize {
  prefix_len_alpha(input)
}

/// Returns the length of the leading ASCII alphanumeric prefix.
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_alphanumeric(input: &[u8]) -> usize {
  prefix_len_alphanumeric(input)
}

/// Returns the length of the leading C-style identifier-start prefix
/// (`a..=z`, `A..=Z`, `_`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_ident_start(input: &[u8]) -> usize {
  prefix_len_ident_start(input)
}

/// Returns the length of the leading C-style identifier-continuation prefix
/// (`a..=z`, `A..=Z`, `0..=9`, `_`).
#[cfg_attr(not(tarpaulin), inline(always))]
#[cfg(not(target_arch = "aarch64"))]
pub fn skip_ident(input: &[u8]) -> usize {
  prefix_len_ident(input)
}
