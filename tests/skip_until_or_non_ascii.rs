//! Differential test for `skip_until_or_non_ascii`.
//!
//! The function returns the index of the first byte that is one of the needles
//! or is not ASCII (`0x80..=0xFF`). Every case is compared against an oracle
//! written from that definition alone,
//!
//! ```text
//! input.iter().position(|&b| b >= 0x80 || needles.contains(&b))
//! ```
//!
//! which shares no code with the crate, so a defect in the scalar adapter or in
//! a vector mask shows up as a disagreement rather than as two matching
//! mistakes.
//!
//! The function is `skip_until` over a crate-private needle adapter, so it runs
//! `skip_until`'s dispatcher, scalar probe, 2x-unrolled loop, single-chunk loop
//! and overlapping tail. Each case calls it with one needle *type*, because each
//! type reaches a different `Needles` impl and mask arm: `u8`; `[u8; N]` for N
//! in {0, 1, 4, 5, 9}, where 0 checks that an empty set still stops at
//! non-ASCII bytes and 9 takes the dynamic arm; `&[u8]` with 0, 4 and 16
//! needles; and one `&&[u8]` through the blanket impl. Among the sets are the
//! four bytes a GraphQL lexer stops on inside a string (`"`, `\`, `\r`, `\n`),
//! a set with a non-ASCII needle, and a set whose needles the sweeps never
//! plant.
//!
//! * **exhaustive over short lengths**: every length up to 200 crossed with
//!   every position of the first hit, where the hit is a needle, a non-ASCII
//!   byte, or both kinds in both orders at distances around each chunk width.
//!   200 covers the probe, loop and tail seams of the 16, 32 and 64-byte
//!   backends and of their 2x strides, plus or minus one;
//! * **every alignment**, by running a reduced sweep over slices carved at each
//!   offset 0..=64 of a larger allocation;
//! * **all 256 byte values per tier**, at the offsets past the scalar probe of
//!   the backend each group of lengths is for;
//! * **pseudo-random inputs**, including rounds with no non-ASCII byte at all,
//!   scanned whole and then from every cursor position the way a lexer does.
//!
//! The all-256 sweep is the one that pins every byte value in the vector masks:
//! `0x7F` must pass and `0x80` must stop. The structured sweeps plant one
//! non-ASCII value and the case's own needles, the random one also mixes in
//! lowercase letters and any non-ASCII value, and a hit early in the slice is
//! answered by the scalar probe before any vector code runs. Every sweep fills
//! with `0x7F`, so each lane a mask classifies must at least let that byte pass.
//!
//! Whichever backend the host dispatches to is the one under test. The
//! forced-scalar and SSE4.2-max runs live in `.github/workflows/coverage.yml`
//! (lines 92-118: the `macos-aarch64-scalar`, `linux-x86_64-sse42-max` and
//! `linux-x86_64-scalar` legs), not in `ci.yml`. The no-std legs of `ci.yml`'s
//! `--feature-powerset` test job are the de-facto x86 scalar run: without
//! `std` the x86 tiers are detected at compile time, and the default targets
//! enable none of them. Two seams predate this test and stay outside `ci.yml`:
//! the i686 SSE2 mask is only ever compiled, never run, and the SSE4.2
//! 2x-unrolled loop runs only in coverage's `sse42-max` leg, because every other
//! x86 runner takes AVX2 at the lengths that reach it.

use core::fmt;

use memspan::skip_until_or_non_ascii;

// ── oracle and cases ─────────────────────────────────────────────────────────

/// The definition, as a plain scan.
fn oracle(input: &[u8], needles: &[u8]) -> Option<usize> {
  input
    .iter()
    .position(|&b| b >= 0x80 || needles.contains(&b))
}

/// The byte every sweep fills with: the largest ASCII value, and a needle of no
/// case.
const FILL: u8 = 0x7F;

/// The non-ASCII byte the exhaustive sweep plants: the smallest one.
const LOW_NON_ASCII: u8 = 0x80;

/// The non-ASCII byte the alignment sweep plants: the largest one.
const HIGH_NON_ASCII: u8 = 0xFF;

const NO_NEEDLES: [u8; 0] = [];
const NEWLINE: [u8; 1] = *b"\n";
const STRING_STOPS: [u8; 4] = *b"\"\\\r\n";
const WITH_NON_ASCII: [u8; 5] = *b"{};,\xC3";
const NINE: [u8; 9] = *b"()[]<>!?@";
const FOUR: [u8; 4] = *b"#$%&";
const SIXTEEN: [u8; 16] = *b"0123456789+-*/=^";
const TWO: [u8; 2] = *b":|";
const NEVER: [u8; 3] = *b"\x01\x02\x03";

/// One needle type and set under test.
struct Case {
  /// The needle type, and what the set is for, in failure messages.
  name: &'static str,
  /// The set as the oracle sees it.
  needles: &'static [u8],
  /// The ASCII needles the sweeps plant as needle hits. Empty for a set with
  /// none, and for the set whose needles must never occur.
  plant: &'static [u8],
  /// The public function, called with this case's needle type.
  scan: fn(&[u8]) -> Option<usize>,
}

const CASES: &[Case] = &[
  Case {
    name: "u8",
    needles: b"\"",
    plant: b"\"",
    scan: |input| skip_until_or_non_ascii(input, b'"'),
  },
  Case {
    name: "[u8; 0]",
    needles: &NO_NEEDLES,
    plant: &[],
    scan: |input| skip_until_or_non_ascii(input, NO_NEEDLES),
  },
  Case {
    name: "[u8; 1]",
    needles: &NEWLINE,
    plant: &NEWLINE,
    scan: |input| skip_until_or_non_ascii(input, NEWLINE),
  },
  Case {
    name: "[u8; 4], a GraphQL string's stops",
    needles: &STRING_STOPS,
    plant: &STRING_STOPS,
    scan: |input| skip_until_or_non_ascii(input, STRING_STOPS),
  },
  Case {
    name: "[u8; 5] with a non-ASCII needle",
    needles: &WITH_NON_ASCII,
    plant: b"{};,",
    scan: |input| skip_until_or_non_ascii(input, WITH_NON_ASCII),
  },
  Case {
    name: "[u8; 9]",
    needles: &NINE,
    plant: &NINE,
    scan: |input| skip_until_or_non_ascii(input, NINE),
  },
  Case {
    name: "&[u8], empty",
    needles: &[],
    plant: &[],
    scan: |input| {
      let needles: &[u8] = &[];
      skip_until_or_non_ascii(input, needles)
    },
  },
  Case {
    name: "&[u8], 4 needles",
    needles: &FOUR,
    plant: &FOUR,
    scan: |input| {
      let needles: &[u8] = &FOUR;
      skip_until_or_non_ascii(input, needles)
    },
  },
  Case {
    name: "&[u8], 16 needles",
    needles: &SIXTEEN,
    plant: &SIXTEEN,
    scan: |input| {
      let needles: &[u8] = &SIXTEEN;
      skip_until_or_non_ascii(input, needles)
    },
  },
  Case {
    name: "&&[u8]",
    needles: &TWO,
    plant: &TWO,
    scan: |input| {
      let slice: &[u8] = &TWO;
      let needles: &&[u8] = &slice;
      skip_until_or_non_ascii(input, needles)
    },
  },
  Case {
    name: "[u8; 3] that never occurs",
    needles: &NEVER,
    plant: &[],
    scan: |input| skip_until_or_non_ascii(input, NEVER),
  },
];

/// The needle a sweep plants at `pos`, cycling through the case's plantable
/// needles so each one is a hit somewhere. `None` when it has none.
fn needle_for(case: &Case, pos: usize) -> Option<u8> {
  pos.checked_rem(case.plant.len()).map(|i| case.plant[i])
}

/// Checks one input: first that it holds exactly the first hit the sweep
/// planted, which pins the shared buffer (a byte left behind by an earlier step
/// would change the input without either side noticing), then that the public
/// function agrees with the oracle.
#[track_caller]
fn check(case: &Case, input: &[u8], planted: Option<usize>, what: fmt::Arguments<'_>) {
  let name = case.name;
  let want = oracle(input, case.needles);
  assert_eq!(
    want, planted,
    "{name}: the input does not hold what the sweep planted ({what})"
  );
  assert_eq!((case.scan)(input), want, "{name}: {what}");
}

// ── volume under Miri ────────────────────────────────────────────────────────
//
// As in `short_run_differential.rs`, the reduction is about volume, not
// coverage. The native run is the correctness sweep and stays exhaustive. The
// Miri run is the UB sweep: an out-of-bounds read, an over-long vector load or
// a provenance violation surfaces the first time its path executes, so what the
// reduced schedule must keep is every path — the scalar answer, the probe-only
// return, the overlap tail, the single-chunk loop and the 2x-unrolled loop, at
// each backend width — not every input. `miri_schedule_reaches_every_path`
// checks that it does. Every sweep below reuses one buffer per case instead of
// allocating per step, which is what exhausted Miri's address space on i686.

/// Longest slice the structured sweeps build.
const MAX_LEN: usize = 200;

/// Lengths under Miri. For each chunk width W of 16, 32 and 64: a length below
/// it, W itself (the probe covers everything), W + 1 (the overlap tail), 2W
/// (one single-chunk iteration), 3W (one 2x-unrolled iteration) and 3W + 1
/// (the 2x loop, then the tail).
const MIRI_LENGTHS: &[usize] = &[
  0, 1, 15, 16, 17, 31, 32, 33, 48, 49, 63, 64, 65, 96, 97, 128, 192, 193,
];

fn sweep_lengths() -> Vec<usize> {
  if cfg!(miri) {
    MIRI_LENGTHS.to_vec()
  } else {
    (0..=MAX_LEN).collect()
  }
}

/// The first, middle and last of `positions`, which must be sorted.
fn first_mid_last(positions: &[usize]) -> Vec<usize> {
  let mut reduced: Vec<usize> = [
    positions.first(),
    positions.get(positions.len() / 2),
    positions.last(),
  ]
  .into_iter()
  .flatten()
  .copied()
  .collect();
  reduced.dedup();
  reduced
}

/// Positions of the first hit: every one natively, the first, middle and last
/// under Miri.
fn sweep_positions(len: usize) -> Vec<usize> {
  let every: Vec<usize> = (0..len).collect();
  if cfg!(miri) {
    first_mid_last(&every)
  } else {
    every
  }
}

/// How far the second hit lies past the first: inside one lane group, and on
/// each side of every chunk width.
const DISTANCES: &[usize] = &[1, 2, 15, 16, 17, 31, 32, 33, 63, 64, 65];
const MIRI_DISTANCES: &[usize] = &[1];

fn sweep_distances() -> &'static [usize] {
  if cfg!(miri) {
    MIRI_DISTANCES
  } else {
    DISTANCES
  }
}

fn alignment_offsets() -> Vec<usize> {
  if cfg!(miri) {
    vec![0, 1, 7, 15, 31, 63]
  } else {
    (0..=64).collect()
  }
}

/// Lengths for the alignment sweep: the boundary lengths natively, and one
/// overlap-tail length per chunk width under Miri.
fn alignment_lengths() -> &'static [usize] {
  if cfg!(miri) {
    &[17, 65, 193]
  } else {
    &[
      0, 1, 15, 16, 17, 31, 32, 33, 48, 49, 63, 64, 65, 96, 97, 128, 129, 192, 193, 200,
    ]
  }
}

/// SSE4.2 on x86_64, NEON and simd128: the 16-byte probe, then one overlap tail.
const TIER_16_LENGTHS: &[usize] = &[16, 17, 31];

/// AVX2 on x86_64, where offsets below 32 are still its probe, so 32 and up are
/// the AVX2 mask's; the 16-byte tiers classify all of `16..len` here.
const TIER_32_LENGTHS: &[usize] = &[32, 33, 48, 49, 63];

/// AVX-512BW under SDE, and AVX2 on the hosted x86_64 runners.
const TIER_64_LENGTHS: &[usize] = &[64, 65, 96, 129, 160, 192, 193, 200];
const TIER_64_POSITIONS: &[usize] = &[
  16, 17, 31, 32, 33, 40, 63, 64, 65, 80, 127, 128, 129, 191, 192,
];
/// Offsets counted back from the end, which land in each overlap tail.
const TIER_64_FROM_END: &[usize] = &[1, 2, 9, 17, 33, 65];

/// One length from each tier group runs under Miri.
const MIRI_TIER_LENGTHS: &[usize] = &[31, 63, 193];

/// `(length, offsets)` for the all-256 sweep. Every offset is at least 16, past
/// the narrowest probe, so a vector mask classifies it on some backend.
fn all_256_schedule() -> Vec<(usize, Vec<usize>)> {
  let mut schedule = Vec::new();
  for &len in TIER_16_LENGTHS.iter().chain(TIER_32_LENGTHS) {
    schedule.push((len, (16..len).collect::<Vec<usize>>()));
  }
  for &len in TIER_64_LENGTHS {
    let mut positions: Vec<usize> = TIER_64_POSITIONS
      .iter()
      .copied()
      .chain(
        TIER_64_FROM_END
          .iter()
          .filter_map(|&back| len.checked_sub(back)),
      )
      .filter(|p| (16..len).contains(p))
      .collect();
    positions.sort_unstable();
    positions.dedup();
    schedule.push((len, positions));
  }
  if cfg!(miri) {
    schedule.retain(|(len, _)| MIRI_TIER_LENGTHS.contains(len));
    for (_, positions) in &mut schedule {
      *positions = first_mid_last(positions);
    }
  }
  schedule
}

/// Byte values crossed with each offset: all 256 natively. Under Miri, the
/// class boundaries and one needle; the value a lane holds cannot change which
/// bytes a kernel reads, and the native run settles classification.
fn candidate_bytes(case: &Case) -> Vec<u8> {
  if cfg!(miri) {
    let mut bytes = vec![0x00, FILL, LOW_NON_ASCII, HIGH_NON_ASCII];
    bytes.extend(case.plant.first());
    bytes
  } else {
    (0..=u8::MAX).collect()
  }
}

/// `(needle gap, non-ASCII gap)` for each pseudo-random round: about one byte
/// in `gap` is of that kind, and a gap of 0 plants none, so those rounds hold
/// no byte of that kind at all. Miri runs only the first two rounds.
const DENSITIES: &[(u64, u64)] = &[
  (16, 0),
  (48, 64),
  (3, 0),
  (0, 6),
  (200, 0),
  (24, 9),
  (0, 0),
  (8, 300),
];

/// Longest pseudo-random input.
const RANDOM_MAX_LEN: usize = 4096;

fn random_lengths() -> &'static [usize] {
  if cfg!(miri) {
    &[129, 512]
  } else {
    &[129, 512, 1024, RANDOM_MAX_LEN]
  }
}

fn random_rounds() -> usize {
  if cfg!(miri) { 2 } else { 24 }
}

/// The reduced Miri schedule must still enter every path the kernels have.
///
/// Checks the Miri constants directly rather than what this build selected, so
/// the native run guards the reduced schedule too.
#[test]
fn miri_schedule_reaches_every_path() {
  let lengths = MIRI_LENGTHS;
  let has = |n: usize| lengths.contains(&n);
  for chunk in [16usize, 32, 64] {
    assert!(
      lengths.iter().any(|&l| l > 0 && l < chunk),
      "no length below {chunk}: the scalar answer before that width is unreached"
    );
    assert!(
      has(chunk),
      "no length {chunk}: the probe-only return at that width is unreached"
    );
    assert!(
      has(chunk + 1),
      "no length {chunk} + 1: the overlap tail at that width is unreached"
    );
    assert!(
      has(2 * chunk),
      "no length 2 x {chunk}: the single-chunk loop at that width is unreached"
    );
    assert!(
      has(3 * chunk),
      "no length 3 x {chunk}: the 2x-unrolled loop at that width is unreached"
    );
    assert!(
      has(3 * chunk + 1),
      "no length 3 x {chunk} + 1: the tail after the 2x-unrolled loop is unreached"
    );
  }

  for len in [1usize, 2, 17, 193] {
    let every: Vec<usize> = (0..len).collect();
    let reduced = first_mid_last(&every);
    for want in [0, len / 2, len - 1] {
      assert!(
        reduced.contains(&want),
        "first/mid/last of len={len} lacks {want}"
      );
    }
  }

  for group in [TIER_16_LENGTHS, TIER_32_LENGTHS, TIER_64_LENGTHS] {
    assert!(
      MIRI_TIER_LENGTHS.iter().any(|len| group.contains(len)),
      "no Miri length in the all-256 group {group:?}"
    );
  }
  for &len in MIRI_TIER_LENGTHS {
    assert!(
      len > 16 && !len.is_multiple_of(16),
      "Miri all-256 length {len} reaches no overlap tail past the 16-byte probe"
    );
  }

  let miri_rounds = &DENSITIES[..2];
  assert!(
    miri_rounds.iter().any(|&(_, non_ascii)| non_ascii == 0),
    "no Miri random round without non-ASCII bytes"
  );
  assert!(
    miri_rounds
      .iter()
      .any(|&(needle, non_ascii)| needle != 0 && non_ascii != 0),
    "no Miri random round with both kinds of hit"
  );
}

// ── the sweeps ───────────────────────────────────────────────────────────────

/// Every length up to [`MAX_LEN`], crossed with every position of the first
/// hit: a needle alone, a non-ASCII byte alone, and both kinds in both orders.
#[test]
fn every_length_and_first_hit_agrees_with_the_oracle() {
  for case in CASES {
    let mut buf = [FILL; MAX_LEN];

    for len in sweep_lengths() {
      check(case, &buf[..len], None, format_args!("len={len}, no hit"));

      for pos in sweep_positions(len) {
        buf[pos] = LOW_NON_ASCII;
        check(
          case,
          &buf[..len],
          Some(pos),
          format_args!("len={len}, non-ASCII at {pos}"),
        );

        let Some(needle) = needle_for(case, pos) else {
          buf[pos] = FILL;
          continue;
        };
        buf[pos] = needle;
        check(
          case,
          &buf[..len],
          Some(pos),
          format_args!("len={len}, needle {needle:#04x} at {pos}"),
        );

        for &distance in sweep_distances() {
          let second = pos + distance;
          if second >= len {
            break;
          }
          buf[second] = LOW_NON_ASCII;
          check(
            case,
            &buf[..len],
            Some(pos),
            format_args!("len={len}, needle at {pos}, then non-ASCII at {second}"),
          );
          buf[pos] = LOW_NON_ASCII;
          buf[second] = needle;
          check(
            case,
            &buf[..len],
            Some(pos),
            format_args!("len={len}, non-ASCII at {pos}, then needle at {second}"),
          );
          buf[pos] = needle;
          buf[second] = FILL;
        }

        buf[pos] = FILL;
      }
    }
  }
}

/// A reduced sweep over slices carved at every offset of a larger allocation,
/// so no kernel can rely on the alignment of its base pointer.
#[test]
fn every_alignment_agrees_with_the_oracle() {
  for case in CASES {
    let mut backing = [FILL; MAX_LEN + 65];

    for offset in alignment_offsets() {
      for &len in alignment_lengths() {
        let end = offset + len;
        check(
          case,
          &backing[offset..end],
          None,
          format_args!("offset={offset}, len={len}, no hit"),
        );

        for pos in [0, len / 2, len.saturating_sub(1)] {
          if pos >= len {
            continue;
          }
          let at = offset + pos;

          backing[at] = HIGH_NON_ASCII;
          check(
            case,
            &backing[offset..end],
            Some(pos),
            format_args!("offset={offset}, len={len}, non-ASCII at {pos}"),
          );

          if let Some(needle) = needle_for(case, pos) {
            backing[at] = needle;
            check(
              case,
              &backing[offset..end],
              Some(pos),
              format_args!("offset={offset}, len={len}, needle {needle:#04x} at {pos}"),
            );
          }

          backing[at] = FILL;
        }
      }
    }
  }
}

/// Every byte value at every offset that some backend's vector mask
/// classifies, grouped by the backend each length is for.
///
/// This is the sweep that pins every byte value in the vector masks: `0x7F`
/// must pass and `0x80` must stop. An offset below 16 would be answered by the
/// scalar probe on every backend and say nothing about a mask, which is why
/// every offset here is at least 16. The other bytes are all filler, so a mask
/// that misjudges the candidate moves the answer from `pos` to `None` or back,
/// never by one.
#[test]
fn all_256_bytes_agree_past_the_probe_on_every_tier() {
  let schedule = all_256_schedule();

  for case in CASES {
    let name = case.name;
    let bytes = candidate_bytes(case);
    let mut buf = [FILL; MAX_LEN];

    for (len, positions) in &schedule {
      let len = *len;
      check(
        case,
        &buf[..len],
        None,
        format_args!("len={len}, filler only"),
      );

      for &pos in positions {
        for &byte in &bytes {
          buf[pos] = byte;
          let input = &buf[..len];

          let got = (case.scan)(input);
          assert_eq!(
            got,
            oracle(input, case.needles),
            "{name}: byte={byte:#04x} at pos={pos}, len={len}"
          );
          // Restated from the loop variables, so an oracle and a mask that are
          // wrong in the same direction still fail.
          let member = byte >= 0x80 || case.needles.contains(&byte);
          assert_eq!(
            got,
            if member { Some(pos) } else { None },
            "{name}: byte={byte:#04x} at pos={pos}, len={len}, membership"
          );
        }
        buf[pos] = FILL;
      }
    }
  }
}

/// Long inputs whose hits, and so whose run lengths, vary from byte to byte,
/// scanned whole and then from every cursor position a lexer would ask about.
#[test]
fn pseudorandom_inputs_and_cursor_walks_agree_with_the_oracle() {
  // xorshift64*, so the corpus is reproducible without a dev-dependency.
  let mut state = 0x2545_F491_4F6C_DD1Du64;
  let mut next = move || {
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
  };

  for case in CASES {
    let name = case.name;
    // One buffer per case at the longest length, refilled per round.
    let mut buf = vec![FILL; RANDOM_MAX_LEN];

    for &len in random_lengths() {
      for round in 0..random_rounds() {
        let (needle_gap, non_ascii_gap) = DENSITIES[round % DENSITIES.len()];
        for byte in buf[..len].iter_mut() {
          let r = next();
          *byte = if non_ascii_gap != 0 && (r >> 8).is_multiple_of(non_ascii_gap) {
            // Any of the 128 non-ASCII values.
            0x80 | (r >> 57) as u8
          } else if needle_gap != 0
            && !case.plant.is_empty()
            && (r >> 16).is_multiple_of(needle_gap)
          {
            case.plant[(r >> 40) as usize % case.plant.len()]
          } else if (r >> 24).is_multiple_of(2) {
            FILL
          } else {
            // Lowercase letters, which no set contains.
            b'a' + ((r >> 48) % 26) as u8
          };
        }

        // The first step scans the whole input; each later one starts just
        // past the previous hit, so the slice stays long while the run it
        // asks about is short.
        let input = &buf[..len];
        let mut pos = 0usize;
        while pos < len {
          let tail = &input[pos..];
          let got = (case.scan)(tail);
          assert_eq!(
            got,
            oracle(tail, case.needles),
            "{name}: len={len}, round={round}, cursor={pos}"
          );
          let Some(hit) = got else {
            break;
          };
          pos += hit + 1;
        }
      }
    }
  }
}
