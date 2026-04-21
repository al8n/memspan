//! Coverage for the public `skip_class!` macro. We define three custom
//! classes that exercise the bytes-only, ranges-only, and mixed paths, then
//! run the same byte-table + miss-position checks the built-in fns get.

// ---- bytes only ----------------------------------------------------------

lexsimd::skip_class! {
  /// Whitespace plus a comma separator.
  pub fn skip_ws_and_comma, bytes = [b' ', b'\t', b'\r', b'\n', b','];
}

// ---- ranges only ---------------------------------------------------------

lexsimd::skip_class! {
  /// Lowercase ASCII letters only.
  pub fn skip_lowercase, ranges = [b'a'..=b'z'];
}

// ---- bytes + ranges ------------------------------------------------------

lexsimd::skip_class! {
  /// Alphanumeric plus a few punctuation bytes.
  pub fn skip_punct_ident,
    bytes = [b'_', b'-', b'!', b'?'],
    ranges = [b'a'..=b'z', b'A'..=b'Z', b'0'..=b'9'];
}

// ---- helpers -------------------------------------------------------------

fn assert_byte_table<F, P>(skip_fn: F, pred: P, name: &str)
where
  F: Fn(&[u8]) -> usize,
  P: Fn(u8) -> bool,
{
  for b in 0u8..=255 {
    let input = [b];
    let got = skip_fn(&input);
    let want = if pred(b) { 1 } else { 0 };
    assert_eq!(
      got,
      want,
      "{name}: byte 0x{b:02x} ({:?}) — pred says match={}",
      b as char,
      pred(b)
    );
  }
}

fn assert_miss_position_exhaustive<F>(skip_fn: F, fill: u8, miss: u8, name: &str)
where
  F: Fn(&[u8]) -> usize,
{
  for len in 1usize..=80 {
    for miss_pos in 0..len {
      let mut input = vec![fill; len];
      input[miss_pos] = miss;
      assert_eq!(
        skip_fn(&input),
        miss_pos,
        "{name}: len={len}, miss_pos={miss_pos}"
      );
    }
    let input = vec![fill; len];
    assert_eq!(skip_fn(&input), len, "{name}: len={len}, all-match");
  }
}

// ---- skip_ws_and_comma ----------------------------------------------------

#[test]
fn ws_and_comma_basic() {
  assert_eq!(skip_ws_and_comma(b""), 0);
  assert_eq!(skip_ws_and_comma(b"   ,\t\n"), 6);
  assert_eq!(skip_ws_and_comma(b" foo"), 1);
  assert_eq!(skip_ws_and_comma(b"foo,"), 0);
  // Vertical-tab and form-feed are NOT in the set.
  assert_eq!(skip_ws_and_comma(b" \x0B"), 1);
}

#[test]
fn ws_and_comma_byte_table() {
  assert_byte_table(
    skip_ws_and_comma,
    |b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b','),
    "skip_ws_and_comma",
  );
}

#[test]
fn ws_and_comma_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip_ws_and_comma, b' ', b'a', "skip_ws_and_comma");
  assert_miss_position_exhaustive(skip_ws_and_comma, b',', b'q', "skip_ws_and_comma");
}

// ---- skip_lowercase (ranges only) ----------------------------------------

#[test]
fn lowercase_basic() {
  assert_eq!(skip_lowercase(b""), 0);
  assert_eq!(skip_lowercase(b"abcXYZ"), 3);
  assert_eq!(skip_lowercase(b"abcdefghijklmnopqrstuvwxyz_"), 26);
  assert_eq!(skip_lowercase(b"_abc"), 0);
  assert_eq!(skip_lowercase(b"a_b"), 1);
}

#[test]
fn lowercase_byte_table() {
  assert_byte_table(
    skip_lowercase,
    |b: u8| b.is_ascii_lowercase(),
    "skip_lowercase",
  );
}

#[test]
fn lowercase_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip_lowercase, b'a', b'A', "skip_lowercase");
  assert_miss_position_exhaustive(skip_lowercase, b'z', b'1', "skip_lowercase");
}

// ---- skip_punct_ident (mixed) --------------------------------------------

#[test]
fn punct_ident_basic() {
  assert_eq!(skip_punct_ident(b""), 0);
  assert_eq!(skip_punct_ident(b"hello-world!"), 12);
  assert_eq!(skip_punct_ident(b"empty? 42"), 6);
  assert_eq!(skip_punct_ident(b"_foo-bar?"), 9);
  assert_eq!(skip_punct_ident(b"+plus"), 0);
}

#[test]
fn punct_ident_byte_table() {
  assert_byte_table(
    skip_punct_ident,
    |b: u8| {
      matches!(b, b'_' | b'-' | b'!' | b'?')
        || b.is_ascii_lowercase()
        || b.is_ascii_uppercase()
        || b.is_ascii_digit()
    },
    "skip_punct_ident",
  );
}

#[test]
fn punct_ident_miss_position_exhaustive() {
  assert_miss_position_exhaustive(skip_punct_ident, b'a', b' ', "skip_punct_ident");
  assert_miss_position_exhaustive(skip_punct_ident, b'!', b'.', "skip_punct_ident");
  assert_miss_position_exhaustive(skip_punct_ident, b'9', b'@', "skip_punct_ident");
}

// ---- generated fn matches built-in equivalents ---------------------------

/// `skip_class!` with the same byte set as our built-in `skip_whitespace`
/// must produce identical results across the byte table — sanity-checks
/// that the macro hasn't drifted from the hand-written specialization.
#[test]
fn generated_whitespace_matches_builtin() {
  lexsimd::skip_class! {
    pub fn skip_ws_macro, bytes = [b' ', b'\t', b'\r', b'\n'];
  }

  for b in 0u8..=255 {
    let input = [b];
    assert_eq!(
      skip_ws_macro(&input),
      lexsimd::skip::skip_whitespace(&input),
      "byte 0x{b:02x}: macro vs built-in disagreed"
    );
  }

  // A few longer inputs spanning the SIMD probe + main loop + tail.
  for len in [16, 17, 32, 64, 128] {
    for offset in 0..len {
      let mut input = vec![b' '; len];
      input[offset] = b'!';
      assert_eq!(
        skip_ws_macro(&input),
        lexsimd::skip::skip_whitespace(&input),
        "len={len}, offset={offset}"
      );
    }
  }
}
