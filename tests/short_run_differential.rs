//! Differential test for the ASCII-class `skip_*` family.
//!
//! Each public scanner is compared byte-for-byte against an independent scalar
//! oracle spelled out in this file. The oracle shares no code with the library:
//! it is a plain `position` loop over a predicate written from the documented
//! definition of the class, so a change that alters *either* the scalar
//! fallback or a SIMD kernel shows up as a disagreement rather than as two
//! matching mistakes.
//!
//! The scanners take a scalar probe of the first `CLASS_PROBE` bytes before
//! entering their vector loop, and the vector loop then re-reads the probed
//! bytes and finishes with an overlapping tail chunk. That gives three seams
//! per call — probe/loop, loop/tail, and the slice end — and this test pins
//! every one of them:
//!
//! * **exhaustive over short lengths**: every length up to 96 crossed with
//!   every possible position of the first non-member byte. 96 spans the probe
//!   (8), the NEON chunk (16), the AVX2 chunk (32), the AVX-512 chunk (64) and
//!   the 2x-unrolled stride of each, together with their +/-1 neighbours;
//! * **runs that end exactly at the slice end**, which is the all-match case of
//!   the sweep above and the only way to reach the `cur == len` early return;
//! * **every alignment**, by running the whole sweep again over slices carved
//!   at each offset 0..=64 of a larger allocation, so a kernel that assumed an
//!   aligned base would fail;
//! * **long pseudo-random inputs**, where run lengths vary from call to call
//!   the way they do in real input and no single seam is favoured;
//! * **all 256 byte values at every offset past the probe**, which is the only
//!   sweep here that reaches the vector masks at all.
//!
//! That last one is load-bearing and easy to get wrong. The obvious way to
//! write a whole-alphabet check is to put the candidate byte at offset 0 — and
//! then the scalar probe answers it and returns before any vector code runs, so
//! the check pins the scalar predicate and says nothing about the mask beside
//! it. The sweeps above cannot cover the gap either: they vary length and
//! alignment but use a single non-member byte per class, so a mask that
//! disagrees with the predicate about some *other* byte slips through all of
//! them. Only [`all_256_bytes_classify_identically_past_the_probe`] crosses
//! every byte value with an offset the vector loop actually classifies.
//!
//! Whichever backend the host dispatches to is the one under test. CI reaches
//! the rest by re-running with `memspan_force_scalar`, `memspan_disable_avx512`
//! and friends, and by running the x86 suite under Intel SDE and the wasm suite
//! under wasmtime.

use memspan::skip;

// ── oracles ──────────────────────────────────────────────────────────────────

fn oracle(input: &[u8], member: fn(u8) -> bool) -> usize {
  input
    .iter()
    .position(|&b| !member(b))
    .unwrap_or(input.len())
}

fn is_binary(b: u8) -> bool {
  b == b'0' || b == b'1'
}
fn is_octal(b: u8) -> bool {
  (b'0'..=b'7').contains(&b)
}
fn is_digit(b: u8) -> bool {
  b.is_ascii_digit()
}
fn is_hex(b: u8) -> bool {
  b.is_ascii_hexdigit()
}
fn is_alpha(b: u8) -> bool {
  b.is_ascii_alphabetic()
}
fn is_alphanumeric(b: u8) -> bool {
  b.is_ascii_alphanumeric()
}
fn is_ident_start(b: u8) -> bool {
  b.is_ascii_alphabetic() || b == b'_'
}
fn is_ident(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_'
}
fn is_whitespace(b: u8) -> bool {
  b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}
fn is_lower(b: u8) -> bool {
  b.is_ascii_lowercase()
}
fn is_upper(b: u8) -> bool {
  b.is_ascii_uppercase()
}
fn is_ascii(b: u8) -> bool {
  b.is_ascii()
}
fn is_non_ascii(b: u8) -> bool {
  !b.is_ascii()
}
fn is_graphic(b: u8) -> bool {
  (0x21..=0x7E).contains(&b)
}
fn is_control(b: u8) -> bool {
  b <= 0x1F || b == 0x7F
}

/// One class under test: the public scanner, the independent oracle predicate
/// it must agree with, and a byte inside and outside the class.
struct Class {
  name: &'static str,
  scanner: fn(&[u8]) -> usize,
  member: fn(u8) -> bool,
  fill: u8,
  miss: u8,
}

const fn class(
  name: &'static str,
  scanner: fn(&[u8]) -> usize,
  member: fn(u8) -> bool,
  fill: u8,
  miss: u8,
) -> Class {
  Class {
    name,
    scanner,
    member,
    fill,
    miss,
  }
}

const CLASSES: &[Class] = &[
  class("skip_binary", skip::skip_binary, is_binary, b'1', b'2'),
  class(
    "skip_octal_digits",
    skip::skip_octal_digits,
    is_octal,
    b'7',
    b'8',
  ),
  class("skip_digits", skip::skip_digits, is_digit, b'9', b'a'),
  class("skip_hex_digits", skip::skip_hex_digits, is_hex, b'F', b'g'),
  class("skip_alpha", skip::skip_alpha, is_alpha, b'q', b'0'),
  class(
    "skip_alphanumeric",
    skip::skip_alphanumeric,
    is_alphanumeric,
    b'q',
    b'_',
  ),
  class(
    "skip_ident_start",
    skip::skip_ident_start,
    is_ident_start,
    b'_',
    b'0',
  ),
  class("skip_ident", skip::skip_ident, is_ident, b'_', b'-'),
  class(
    "skip_whitespace",
    skip::skip_whitespace,
    is_whitespace,
    b'\t',
    b'x',
  ),
  class("skip_lower", skip::skip_lower, is_lower, b'z', b'Z'),
  class("skip_upper", skip::skip_upper, is_upper, b'Z', b'z'),
  class("skip_ascii", skip::skip_ascii, is_ascii, b'~', 0x80),
  class(
    "skip_non_ascii",
    skip::skip_non_ascii,
    is_non_ascii,
    0xFF,
    b'a',
  ),
  class(
    "skip_ascii_graphic",
    skip::skip_ascii_graphic,
    is_graphic,
    b'!',
    b' ',
  ),
  class(
    "skip_ascii_control",
    skip::skip_ascii_control,
    is_control,
    0x7F,
    b'a',
  ),
];

/// Longest length swept exhaustively. Covers the 8-byte probe, the 16/32/64
/// byte chunks, their 2x-unrolled strides and the neighbours of each — 129
/// reaches one past the AVX-512 2x stride at 128, which a shorter sweep never
/// entered. Raising it from 96 costs nothing now the buffers are reused.
const MAX_LEN: usize = 129;

// ── volume under Miri ────────────────────────────────────────────────────────
//
// **The reduction below is about volume, not coverage, and the distinction is
// the whole point.**
//
// The native run is the correctness sweep: it is what checks that each kernel
// returns the same prefix length as the oracle for every input, and it stays
// exhaustive because per-target arithmetic can differ anywhere.
//
// The Miri run is the UB sweep. It checks for out-of-bounds reads, misaligned
// or over-long vector loads, and provenance violations, and every one of those
// surfaces the *first* time a code path executes. Running the same shape a
// hundred thousand more times tells Miri nothing further about UB, and on i686
// — the only 32-bit target, so the only one with a 4 GiB simulated address
// space — it exhausts the address space before finishing.
//
// So the schedule shrinks under `cfg(miri)`, and what it must not lose is the
// set of *paths* it enters: the scalar probe, the vector loop, the 2x-unrolled
// stride and each shape of tail, for every backend width the crate has. A
// reduction whose longest run never reaches a vector width would leave Miri
// examining the scalar path alone and the job would pass having checked
// nothing — a gate that runs, passes, and is empty.
//
// `miri_schedule_reaches_every_path` asserts that, so the property is checked
// rather than believed. Do not lower these numbers without re-running it, and
// do not raise them back: the exhaustion they avoid is real.

/// Lengths swept by the boundary tests.
///
/// Exhaustive natively. Under Miri, the boundaries themselves: every backend
/// chunk (16/32/64) and the 8-byte NEON probe, each with its neighbours, plus
/// the 2x-unrolled strides and one past them.
const MIRI_LENGTHS: &[usize] = &[
  0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 96, 128, 129,
];

fn sweep_lengths() -> Vec<usize> {
  if cfg!(miri) {
    MIRI_LENGTHS.to_vec()
  } else {
    (0..=MAX_LEN).collect()
  }
}

/// Positions of the single non-member byte, for a given length.
///
/// Exhaustive natively. Under Miri, the ends, the middle, and each backend
/// boundary that falls inside the slice — the offsets at which a kernel changes
/// which branch of its tail arithmetic it takes.
fn miri_miss_positions(len: usize) -> Vec<usize> {
  let mut v: Vec<usize> = [0, 1, 7, 8, 15, 16, 31, 32, 63, 64, len / 2]
    .into_iter()
    .chain(len.checked_sub(1))
    .filter(|&p| p < len)
    .collect();
  v.sort_unstable();
  v.dedup();
  v
}

fn miss_positions(len: usize) -> Vec<usize> {
  if cfg!(miri) {
    miri_miss_positions(len)
  } else {
    (0..len).collect()
  }
}

/// Alignment offsets. Every sub-16 phase plus each chunk edge under Miri; a
/// misaligned load is misaligned on its first execution, not its sixty-fifth.
fn alignment_offsets() -> Vec<usize> {
  if cfg!(miri) {
    vec![0, 1, 2, 3, 7, 8, 15, 16, 17, 31, 32, 63, 64]
  } else {
    (0..=64usize).collect()
  }
}

/// Lengths for the pseudo-random sweep. Under Miri the two that still cross a
/// vector width; the longer two only repeat the same paths on more bytes.
fn random_lengths() -> Vec<usize> {
  if cfg!(miri) {
    vec![129, 512]
  } else {
    vec![129, 512, 1024, 4096]
  }
}

/// Rounds of the pseudo-random sweep. Each round is a fresh density, and the
/// densities that matter for path coverage are "sparse" and "dense"; the rest
/// re-walk the same branches.
fn random_rounds() -> usize {
  if cfg!(miri) { 2 } else { 24 }
}

/// Lengths for the fully-random sweep.
fn random_byte_lengths() -> Vec<usize> {
  if cfg!(miri) {
    vec![0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 129, 200]
  } else {
    (0..=200usize).collect()
  }
}

/// Lengths for the past-the-probe alphabet sweep.
fn vector_lengths() -> Vec<usize> {
  if cfg!(miri) {
    vec![129]
  } else {
    VECTOR_LENGTHS.to_vec()
  }
}

/// The reduced Miri schedule must still enter every path the kernels have.
///
/// Runs on every configuration so the native build catches a bad edit too, but
/// it is the `cfg(miri)` schedule it is guarding. Each backend answers a run
/// shorter than its probe from the scalar loop, enters its vector loop at the
/// chunk, takes the 2x-unrolled stride at twice it, and reaches the overlap
/// tail whenever the length is not an exact multiple.
#[test]
fn miri_schedule_reaches_every_path() {
  // Checks `MIRI_LENGTHS` directly rather than whatever this build selected,
  // so the native run guards the reduced schedule too. A schedule that is only
  // validated under the configuration that uses it is validated by the run
  // least able to afford failing.
  let lengths = MIRI_LENGTHS;
  let has = |n: usize| lengths.contains(&n);

  // 8 is the probe on NEON, SSE4.2 and AVX2; 16, 32 and 64 are the
  // SSE4.2/simd128, AVX2 and AVX-512 chunks. For each, the schedule needs a
  // length below it (scalar answer), at it (vector loop entry) and past it
  // (tail).
  for width in [8usize, 16, 32, 64] {
    assert!(
      lengths.iter().any(|&l| l > 0 && l < width),
      "no length below {width}: the scalar path before that width is unreached"
    );
    assert!(
      has(width),
      "no length exactly {width}: the vector loop at that width is unreached"
    );
    assert!(
      lengths.iter().any(|&l| l > width),
      "no length past {width}: the tail after that width is unreached"
    );
  }

  // 2x-unrolled strides, and one past each, so the unrolled loop and the
  // single-chunk loop after it are both entered.
  for stride in [32usize, 64, 128] {
    assert!(has(stride), "no length at the {stride}-byte 2x stride");
    assert!(
      lengths.iter().any(|&l| l > stride),
      "no length past the {stride}-byte 2x stride, so its remainder is unreached"
    );
  }

  // A length that is not a multiple of any chunk, so the overlap tail runs
  // with a partial lane mask rather than falling out at `cur == len`.
  assert!(
    lengths
      .iter()
      .any(|&l| l > 64 && !l.is_multiple_of(64) && !l.is_multiple_of(32)),
    "no non-multiple length past 64: the partial overlap tail is unreached"
  );

  // And the miss positions must still land on the boundaries inside a slice.
  let positions = miri_miss_positions(129);
  for boundary in [0usize, 8, 16, 32, 64] {
    assert!(
      positions.contains(&boundary),
      "miss position {boundary} missing: that boundary's tail arithmetic is unreached"
    );
  }
}

// ── the sweeps ───────────────────────────────────────────────────────────────

/// Every length up to [`MAX_LEN`], crossed with every position of the first
/// non-member byte plus the all-match case where the run ends at the slice end.
#[test]
fn every_short_length_and_miss_position_agrees_with_the_oracle() {
  for &Class {
    name,
    scanner,
    member,
    fill,
    miss,
  } in CLASSES
  {
    // One buffer per class, mutated in place and sliced to the length under
    // test. Allocating inside these loops instead cost 4753 allocations per
    // class and 71295 across the fifteen, which exhausted Miri's simulated
    // address space on i686 — the only 32-bit target, and so the only one where
    // 4 GiB can run out. Miri does not aggressively reuse addresses, so the
    // count is what matters rather than the size. Coverage is identical.
    let mut buf = [fill; MAX_LEN];

    for len in sweep_lengths() {
      let all_match = &buf[..len];
      // Both assertions matter, and the second is what makes the shared buffer
      // safe. The differential one compares the scanner against the oracle on
      // the *same* bytes, so a buffer left dirty by a previous iteration would
      // change which input is tested without either side noticing — silent
      // coverage loss rather than a failure. Restating the expected answer in
      // terms of the loop variables pins the buffer's state as well.
      assert_eq!(
        oracle(all_match, member),
        len,
        "{name}: buffer not all-member at len={len} — a previous iteration left it dirty"
      );
      assert_eq!(
        scanner(all_match),
        oracle(all_match, member),
        "{name}: run reaching the slice end, len={len}"
      );

      for miss_pos in miss_positions(len) {
        buf[miss_pos] = miss;
        let input = &buf[..len];
        assert_eq!(
          oracle(input, member),
          miss_pos,
          "{name}: expected exactly one miss at {miss_pos} in len={len}"
        );
        assert_eq!(
          scanner(input),
          oracle(input, member),
          "{name}: len={len}, miss_pos={miss_pos}"
        );
        buf[miss_pos] = fill;
      }
    }
  }
}

/// The same sweep, but every slice is carved out of a larger allocation at a
/// different offset, so no kernel can rely on the base pointer's alignment.
#[test]
fn every_alignment_agrees_with_the_oracle() {
  for &Class {
    name,
    scanner,
    member,
    fill,
    miss,
  } in CLASSES
  {
    let mut backing = [0u8; MAX_LEN + 65];

    for offset in alignment_offsets() {
      for len in [0usize, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 96] {
        for miss_pos in [None, Some(0), Some(len / 2), Some(len.saturating_sub(1))] {
          backing.fill(fill);
          let input = &mut backing[offset..offset + len];
          if let Some(p) = miss_pos
            && p < len
          {
            input[p] = miss;
          }
          let input = &backing[offset..offset + len];
          assert_eq!(
            scanner(input),
            oracle(input, member),
            "{name}: offset={offset}, len={len}, miss_pos={miss_pos:?}"
          );
        }
      }
    }
  }
}

/// Long inputs whose byte values, and therefore whose run lengths, vary from
/// position to position — the shape that made the defect visible in the first
/// place, and the one a fixed-run-length sweep cannot produce.
#[test]
fn pseudorandom_long_inputs_agree_with_the_oracle() {
  // xorshift64*, so the corpus is reproducible without a dev-dependency.
  let mut state = 0x2545_F491_4F6C_DD1Du64;
  let mut next = move || {
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
  };

  for &Class {
    name,
    scanner,
    member,
    fill,
    miss,
  } in CLASSES
  {
    // One buffer per class at the longest length, refilled per round.
    let mut buf = vec![fill; 4096];

    for len in random_lengths() {
      for round in 0..random_rounds() {
        // Sprinkle non-members at random positions with varying density, so
        // runs of every length from zero to the whole slice occur.
        let density = 1 + (round % 12) * 8;
        for byte in buf[..len].iter_mut() {
          *byte = if (next() as usize).is_multiple_of(density) {
            miss
          } else {
            fill
          };
        }
        let input = &buf[..len];
        assert_eq!(
          scanner(input),
          oracle(input, member),
          "{name}: len={len}, round={round}, density={density}"
        );

        // Also drive the scanner the way a lexer does: from every cursor
        // position, so the run being asked about is short while the slice
        // stays long.
        let mut pos = 0usize;
        while pos < input.len() {
          let tail = &input[pos..];
          assert_eq!(
            scanner(tail),
            oracle(tail, member),
            "{name}: sweep at pos={pos}, len={len}, round={round}"
          );
          pos += scanner(tail) + 1;
        }
      }
    }
  }
}

/// Fully random bytes, so members and non-members are drawn from the whole
/// `u8` range rather than from one chosen pair. Catches a mask that is right
/// for the sampled bytes and wrong for some other value in the class.
#[test]
fn fully_random_bytes_agree_with_the_oracle() {
  let mut state = 0x9E37_79B9_7F4A_7C15u64;
  let mut next = move || {
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
  };

  for &Class {
    name,
    scanner,
    member,
    ..
  } in CLASSES
  {
    // One buffer per class, refilled and sliced.
    let mut buf = [0u8; 200];

    for len in random_byte_lengths() {
      for byte in buf[..len].iter_mut() {
        *byte = (next() >> 33) as u8;
      }
      let input = &buf[..len];
      assert_eq!(
        scanner(input),
        oracle(input, member),
        "{name}: random bytes, len={len}"
      );
    }
  }
}

/// Every one of the 256 byte values at the **head** of the slice, where the
/// scalar probe answers before any vector code runs.
///
/// This pins the scalar spelling of each class for the whole alphabet. It says
/// nothing about the vector spelling — see
/// [`all_256_bytes_classify_identically_past_the_probe`], which is the one that
/// covers the kernels.
#[test]
fn all_256_bytes_classify_identically_at_the_head() {
  for &Class {
    name,
    scanner,
    member,
    fill,
    ..
  } in CLASSES
  {
    let mut buf = [fill; 128];

    for byte in 0..=255u8 {
      buf[0] = byte;
      let input = &buf[..];
      let got = scanner(input);
      assert_eq!(
        got,
        oracle(input, member),
        "{name}: byte={byte:#04x} at head"
      );
      assert_eq!(
        got != 0,
        member(byte),
        "{name}: byte={byte:#04x} membership disagrees"
      );
    }
  }
}

/// Lengths at which the public dispatcher hands the whole family to a vector
/// kernel.
///
/// These come from the dispatchers, **not** from the chunk sizes, and the two
/// disagree. `skip_*` on aarch64 falls back to scalar below `len < 32` even
/// though the NEON chunk is 16; the x86 macro needs `len >= 64` for AVX-512 and
/// `len >= 32` for AVX2, while SSE4.2 has no dispatcher gate and its kernel
/// gates at 16; wasm32 gates at 16. So 32 is the smallest length that reaches a
/// vector kernel everywhere, and 64 is the smallest that reaches the widest
/// one. Everything below starts at 64 so no backend is silently measured on its
/// scalar fallback, and the larger lengths give the 64-byte kernels a 2x-loop
/// iteration and a non-aligned overlap tail.
const VECTOR_LENGTHS: [usize; 4] = [64, 96, 129, 160];

/// Candidate offsets that land **after** the scalar probe, so the byte under
/// test is classified by the vector mask rather than by the scalar predicate.
fn positions_past_the_probe(len: usize) -> Vec<usize> {
  let mut positions = Vec::new();

  // 8..16 is the behavioural delta of narrowing the NEON probe from 16 to 8:
  // these bytes used to be scalar-classified and are now vector-classified.
  positions.extend(8..16);

  // Chunk edges and their neighbours for all three vector widths, plus a lane
  // in the middle of a 2x-unrolled iteration.
  positions.extend([16, 17, 31, 32, 33, 40, 63, 64, 65, 80]);

  // Overlap-tail lanes. Each kernel finishes by re-reading the final chunk and
  // masking off what the main loop already covered, so the last few bytes take
  // a different path from everything before them.
  for back in [1usize, 2, 9, 17, 33] {
    if let Some(p) = len.checked_sub(back) {
      positions.push(p);
    }
  }

  positions.retain(|&p| p >= 8 && p < len);
  positions.sort_unstable();
  positions.dedup();

  // Under Miri this sweep is the most expensive in the file: every position is
  // crossed with all 256 byte values and every class. The byte crossing is a
  // *classification* claim, which the native run settles; what Miri needs from
  // it is that each distinct tail shape is entered once. Keep the first offset
  // past the probe, one inside the 2x-unrolled stride, and the last byte.
  if cfg!(miri) {
    let last = positions.len().saturating_sub(1);
    let mid = positions.len() / 2;
    let mut reduced: Vec<usize> = [
      positions.first().copied(),
      positions.get(mid).copied(),
      positions.get(last).copied(),
    ]
    .into_iter()
    .flatten()
    .collect();
    reduced.dedup();
    return reduced;
  }

  positions
}

/// Every one of the 256 byte values, at every offset **past the scalar probe**,
/// in a slice long enough to reach a vector kernel.
///
/// This is the test that actually covers the SIMD masks. The head-position
/// sweep above cannot: the scalar probe answers offset 0 and returns before the
/// vector loop runs, so it exercises only the path that the probe-width change
/// left alone.
///
/// Every byte other than the candidate is a known member, so a mask that
/// disagrees with the scalar predicate about the candidate does not shift the
/// answer by one — it moves it from `pos` all the way to `len`.
#[test]
fn all_256_bytes_classify_identically_past_the_probe() {
  for &Class {
    name,
    scanner,
    member,
    fill,
    ..
  } in CLASSES
  {
    for len in vector_lengths() {
      let mut input = vec![fill; len];

      for pos in positions_past_the_probe(len) {
        for byte in 0..=255u8 {
          input[pos] = byte;

          let got = scanner(&input);
          assert_eq!(
            got,
            oracle(&input, member),
            "{name}: byte={byte:#04x} at pos={pos}, len={len}"
          );
          // Restate the expectation in terms of the class itself, so a mask
          // and a predicate that are wrong in the same direction still fail.
          assert_eq!(
            got,
            if member(byte) { len } else { pos },
            "{name}: byte={byte:#04x} at pos={pos}, len={len}, membership"
          );
        }

        input[pos] = fill;
      }
    }
  }
}
