//! Benchmarks for the **lexer** call shape: a short run at the head of a long
//! slice.
//!
//! Every other bench in this crate hands `skip_*` a buffer whose length *is*
//! the question — `micro/full_match` matches to the last byte, `density_sweep`
//! walks a buffer whose remaining length shrinks toward zero. A lexer never
//! does that. It hands over *the rest of the document* and asks about a run
//! that is typically three to twenty bytes long, so the slice stays kilobytes
//! long for the whole scan while every individual answer is tiny.
//!
//! Two populations, deliberately opposed:
//!
//! * `short_run/*` — runs of 2–20 bytes at the head of a 64 KiB slice. This is
//!   the population the dispatch heuristic does *not* see, because it reads
//!   `input.len()`.
//! * `long_run/*` — one run covering the whole 64 KiB slice. This is the
//!   population the current heuristic serves, and the one any short-run fix
//!   must not regress.
//!
//! and two aggregate groups that drive a scanner the way a lexer does, from a
//! cursor whose remaining slice stays long while the run lengths vary:
//!
//! * `lexer_sweep/*` — the `skip_ascii_class!` kernels, and **only** those.
//!   `.github/workflows/probe-sweep.yml` selects this group by name to sweep
//!   `CLASS_PROBE`, and `ci/probe_sweep.py` fails if a member of it is not one
//!   of the macro-generated kernels.
//! * `generic_sweep/*` — `skip_while` and `skip_until`, which probe a whole
//!   chunk and never read `CLASS_PROBE`. Kept because they are the evidence
//!   that the probe-width defect is specific to the class kernels rather than
//!   general to the dispatcher, but held out of the sweep: a row the constant
//!   cannot move is not evidence about it.
//!
//! Three implementations are compared on both populations:
//!
//! * `memspan` — the library entry point as shipped.
//! * `scalar` — a plain `iter().position` predicate loop. On the short-run
//!   population this early-exits after a handful of bytes, which makes it the
//!   *floor*: no dispatcher can beat it. On the long-run population it is the
//!   ceiling that SIMD is supposed to demolish.
//! * `probe_then` — the caller-side workaround `al8n/pql` carries: classify up
//!   to 16 bytes scalar, hand only the tail to `memspan`. Benched here so the
//!   claim that it is worth anything can be confirmed or refuted in this
//!   crate's own harness rather than quoted from another repository.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use memspan::skip;
use std::hint::black_box;

/// Length of the slice handed to the scanner. Kilobytes, as a lexer's would be.
const SLICE_LEN: usize = 64 * 1024;

/// Run lengths that a lexer actually produces: identifiers, keywords, numbers.
const SHORT_RUNS: [usize; 7] = [2, 3, 5, 8, 12, 16, 20];

/// Bytes scalar-classified before delegating, matching `pql`'s `PROBE`.
const PROBE: usize = 16;

/// `pql`'s caller-side workaround, reproduced verbatim in shape.
#[inline(always)]
fn probe_then<F, P>(input: &[u8], member: P, simd: F) -> usize
where
  P: Fn(u8) -> bool,
  F: FnOnce(&[u8]) -> usize,
{
  let head = input.len().min(PROBE);
  let mut at = 0;
  while at < head {
    if !member(input[at]) {
      return at;
    }
    at += 1;
  }
  if at == input.len() {
    return at;
  }
  at + simd(&input[at..])
}

#[inline(always)]
fn scalar_prefix_len(input: &[u8], pred: impl Fn(u8) -> bool) -> usize {
  input.iter().position(|&b| !pred(b)).unwrap_or(input.len())
}

#[inline(always)]
fn is_ident(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_'
}

#[inline(always)]
fn is_digit(b: u8) -> bool {
  b.is_ascii_digit()
}

#[inline(always)]
fn is_space(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// A `SLICE_LEN` buffer whose first `run` bytes are `fill` and whose remaining
/// bytes are `miss`. The run is at the head; the slice stays long.
fn head_run(run: usize, fill: u8, miss: u8) -> Vec<u8> {
  let mut buf = vec![miss; SLICE_LEN];
  buf[..run].fill(fill);
  buf
}

/// A `SLICE_LEN` buffer that matches everywhere except the final byte.
fn full_run(fill: u8, miss: u8) -> Vec<u8> {
  let mut buf = vec![fill; SLICE_LEN];
  buf[SLICE_LEN - 1] = miss;
  buf
}

/// One `skip_*` class benched over the short-run population.
fn class_short_run<F, P>(c: &mut Criterion, name: &str, memspan_fn: F, pred: P, fill: u8, miss: u8)
where
  F: Fn(&[u8]) -> usize + Copy,
  P: Fn(u8) -> bool + Copy,
{
  let mut group = c.benchmark_group(format!("short_run/{name}"));
  group.throughput(Throughput::Elements(1));

  for run in SHORT_RUNS {
    let input = head_run(run, fill, miss);

    group.bench_with_input(BenchmarkId::new("memspan", run), &input, |b, input| {
      b.iter(|| black_box(memspan_fn(black_box(input.as_slice()))))
    });

    group.bench_with_input(BenchmarkId::new("scalar", run), &input, |b, input| {
      b.iter(|| black_box(scalar_prefix_len(black_box(input.as_slice()), pred)))
    });

    group.bench_with_input(BenchmarkId::new("probe_then", run), &input, |b, input| {
      b.iter(|| black_box(probe_then(black_box(input.as_slice()), pred, memspan_fn)))
    });
  }

  group.finish();
}

/// The same class benched over the long-run population, where the current
/// heuristic is right and a short-run fix could regress things.
fn class_long_run<F, P>(c: &mut Criterion, name: &str, memspan_fn: F, pred: P, fill: u8, miss: u8)
where
  F: Fn(&[u8]) -> usize + Copy,
  P: Fn(u8) -> bool + Copy,
{
  let mut group = c.benchmark_group(format!("long_run/{name}"));
  group.throughput(Throughput::Bytes(SLICE_LEN as u64));

  let input = full_run(fill, miss);

  group.bench_with_input(
    BenchmarkId::new("memspan", SLICE_LEN),
    &input,
    |b, input| b.iter(|| black_box(memspan_fn(black_box(input.as_slice())))),
  );

  group.bench_with_input(BenchmarkId::new("scalar", SLICE_LEN), &input, |b, input| {
    b.iter(|| black_box(scalar_prefix_len(black_box(input.as_slice()), pred)))
  });

  group.bench_with_input(
    BenchmarkId::new("probe_then", SLICE_LEN),
    &input,
    |b, input| b.iter(|| black_box(probe_then(black_box(input.as_slice()), pred, memspan_fn))),
  );

  group.finish();
}

fn bench_classes(c: &mut Criterion) {
  class_short_run(c, "skip_ident", skip::skip_ident, is_ident, b'a', b'-');
  class_long_run(c, "skip_ident", skip::skip_ident, is_ident, b'a', b'-');

  class_short_run(c, "skip_digits", skip::skip_digits, is_digit, b'7', b'x');
  class_long_run(c, "skip_digits", skip::skip_digits, is_digit, b'7', b'x');

  class_short_run(
    c,
    "skip_whitespace",
    skip::skip_whitespace,
    is_space,
    b' ',
    b'x',
  );
  class_long_run(
    c,
    "skip_whitespace",
    skip::skip_whitespace,
    is_space,
    b' ',
    b'x',
  );
}

/// `skip_while` with a needle array — the generic prefix-length entry point.
fn bench_skip_while(c: &mut Criterion) {
  const NEEDLES: [u8; 10] = *b"0123456789";
  let f = |s: &[u8]| skip::skip_while(s, NEEDLES);

  class_short_run(c, "skip_while_10", f, is_digit, b'7', b'x');
  class_long_run(c, "skip_while_10", f, is_digit, b'7', b'x');
}

/// `skip_until` — the string-literal / comment shape, where the run is the
/// distance to a delimiter rather than a class prefix. Benched through the
/// `usize`-returning `skip_until_newline` so it shares the harness above.
fn bench_skip_until(c: &mut Criterion) {
  let mut group = c.benchmark_group("short_run/skip_until_newline");
  group.throughput(Throughput::Elements(1));

  for run in SHORT_RUNS {
    // Run of non-newline bytes, then a newline, then more non-newlines.
    let mut input = vec![b'x'; SLICE_LEN];
    input[run] = b'\n';

    group.bench_with_input(BenchmarkId::new("memspan", run), &input, |b, input| {
      b.iter(|| black_box(skip::skip_until_newline(black_box(input.as_slice()))))
    });

    group.bench_with_input(BenchmarkId::new("scalar", run), &input, |b, input| {
      b.iter(|| {
        black_box(scalar_prefix_len(black_box(input.as_slice()), |c| {
          c != b'\n'
        }))
      })
    });

    group.bench_with_input(BenchmarkId::new("probe_then", run), &input, |b, input| {
      b.iter(|| {
        black_box(probe_then(
          black_box(input.as_slice()),
          |c| c != b'\n',
          skip::skip_until_newline,
        ))
      })
    });
  }

  group.finish();

  let mut group = c.benchmark_group("long_run/skip_until_newline");
  group.throughput(Throughput::Bytes(SLICE_LEN as u64));
  let input = vec![b'x'; SLICE_LEN];

  group.bench_with_input(
    BenchmarkId::new("memspan", SLICE_LEN),
    &input,
    |b, input| b.iter(|| black_box(skip::skip_until_newline(black_box(input.as_slice())))),
  );

  group.bench_with_input(BenchmarkId::new("scalar", SLICE_LEN), &input, |b, input| {
    b.iter(|| {
      black_box(scalar_prefix_len(black_box(input.as_slice()), |c| {
        c != b'\n'
      }))
    })
  });

  group.bench_with_input(
    BenchmarkId::new("probe_then", SLICE_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(probe_then(
          black_box(input.as_slice()),
          |c| c != b'\n',
          skip::skip_until_newline,
        ))
      })
    },
  );

  group.finish();
}

/// Cursor positions scanned by each `lexer_sweep` iteration.
const SCAN_LIMIT: usize = 64 * 1024;

/// A lexer's dispatch loop, reduced to one scan.
///
/// `buf` is far longer than `SCAN_LIMIT`, so every call sees a long remaining
/// slice — the property that makes the dispatch heuristic misread the problem.
#[inline(always)]
fn sweep(buf: &[u8], scan: impl Fn(&[u8]) -> usize) -> usize {
  let mut pos = 0usize;
  let mut checksum = 0usize;
  while pos < SCAN_LIMIT {
    let advanced = scan(&buf[pos..]);
    pos += advanced + 1;
    checksum = checksum.wrapping_add(pos);
  }
  checksum
}

/// Run lengths the `lexer_sweep` corpus cycles through.
///
/// # What this schedule is, and is not
///
/// It is **one shared synthetic schedule applied to all fifteen classes**. It
/// is not fifteen measured class distributions. Its shape was derived by
/// profiling two classes on the PromQL fragment in `realistic_sweep`
/// (`skip_ident` advances zero bytes on 60% of calls with a mean of 1.8;
/// `skip_alpha` 71% zeros), and that shape is then reused for the other
/// thirteen, whose real distributions nobody here has measured. A row is
/// therefore evidence about **this schedule**, not about how that class behaves
/// in any caller, and rows must not be added up into a preference across
/// classes. `ci/probe_sweep.py` prints that scope with every table.
///
/// # Why it varies, and how long it has to be
///
/// It varies within one corpus because a single run length predicts every
/// branch in the scanner perfectly and cannot see the mispredict cost the probe
/// width exists to control.
///
/// How long it has to be is **derived, not chosen**. Two probe widths diverge
/// on runs of at least `min(shipped, candidate)` bytes: below that both answer
/// from the scalar probe, and at or above it the narrower one has handed off to
/// the vector loop while the wider one has not. Across the workflow's three
/// matrices the widest such threshold is 32 — `avx512` comparing its shipped 64
/// against a candidate 32 — so the corpus needs runs reaching 32 and nothing
/// beyond that serves any comparison at all.
///
/// | tier | shipped | candidates | thresholds | needs |
/// |------|---------|------------|------------|-------|
/// | `sse42`  | 8  | 16, 4     | 8, 4       | >= 8  |
/// | `avx2`   | 8  | 32, 16, 4 | 8, 8, 4    | >= 8  |
/// | `avx512` | 64 | 32, 16, 8 | 32, 16, 8  | >= 32 |
///
/// Narrowing the two x86 shipped widths to 8 *relaxed* their requirement —
/// `avx2` needed runs of 16 when it shipped 32 — so the schedule below is
/// unchanged and still satisfies every pair. `avx512` remains the binding
/// constraint at 32, and would stop binding if it were ever measured and
/// narrowed too; the schedule can then be shortened, not lengthened.
///
/// An earlier version of this schedule ran to 96 bytes, because the reach check
/// then demanded a call reaching *every compared width* rather than each
/// shipped-versus-candidate pair. That was the wrong quantifier, and it bought
/// its extra length with realism: mean 10.8 against a lexer's 1.8. With the
/// pair thresholds above, the longest useful run is 48 and the mean is 6.4.
///
/// The weighting is decision-sensitive and was got wrong once: an earlier
/// version cycled each length equally, giving a mean of 17.4, and on it probe 16
/// beat probe 8 on fourteen of fifteen classes — the reverse of the shipped
/// decision. Widening the runs widens the answer, which is why the length is
/// now pinned to what the comparisons require instead of to a preference.
#[rustfmt::skip]
const RUN_SCHEDULE: [usize; 40] = [
  0,  1, 0,  8, 0, 2, 0, 33, 0,  3,
  0, 16, 0,  4, 0, 40, 0, 5, 0, 12,
  0,  2, 0, 20, 0, 3, 0,  9, 0,  1,
  0, 48, 0,  4, 0, 24, 0, 5, 0, 17,
];

/// Benches every `skip_ascii_class!` kernel under the `lexer_sweep` group.
///
/// Each class gets **its own corpus**, built from [`RUN_SCHEDULE`] with a fill
/// byte inside the class and a miss byte outside it. One shared corpus cannot
/// serve fifteen classes: a lowercase-ASCII fragment leaves `skip_upper`,
/// `skip_non_ascii` and `skip_ascii_control` returning zero at every cursor and
/// makes `skip_ascii` match the entire tail in a single call, so four of the
/// fifteen rows were reporting timings in which the probe width could play no
/// part. Criterion still emitted `memspan` and `scalar` cells for them, which is
/// exactly why the set checks passed: a row can be named, produced, counted —
/// and vacuous.
///
/// A macro rather than a `const` table of `fn` pointers, and the difference is
/// not stylistic. A table forces the generic parameters to the *pointer* types
/// `fn(&[u8]) -> usize` and `fn(u8) -> bool`, so the predicate is called
/// indirectly once per byte inside the scalar reference loop and cannot be
/// inlined into it. That inflates the denominator of every ratio in the report:
/// measured on this host, `skip_ident` read 1.90x the scalar loop with fn items
/// and 0.72x with fn pointers, for the same kernel. Passing each scanner and
/// predicate as a `path` keeps them zero-sized fn items.
///
/// `ci/probe_sweep.py` requires the class names here to be exactly the
/// backend's `skip_ascii_class!` invocations, so this list is not a sample.
macro_rules! sweep_classes {
  ($c:expr, $profiles:expr, $(($name:literal, $scanner:path, $pred:path, $fill:expr, $miss:expr)),+ $(,)?) => {
    $( sweep_one($c, $profiles, $name, $scanner, $pred, $fill, $miss); )+
  };
}

/// Builds a class's corpus and records what the sweep will actually do to it.
///
/// The advances are computed with the **scalar predicate**, never with the
/// library, so a broken kernel cannot make a vacuous corpus look healthy.
fn corpus_for(pred: impl Fn(u8) -> bool, fill: u8, miss: u8) -> (Vec<u8>, String) {
  debug_assert!(pred(fill), "fill byte must be inside the class");
  debug_assert!(!pred(miss), "miss byte must be outside the class");

  const BUF_LEN: usize = 1024 * 1024;
  let mut buf = Vec::with_capacity(BUF_LEN + 128);
  let mut schedule = RUN_SCHEDULE.iter().copied().cycle();
  while buf.len() < BUF_LEN {
    let run = schedule.next().expect("cycle never ends");
    buf.extend(core::iter::repeat_n(fill, run));
    buf.push(miss);
  }

  let profile = profile_of(&buf, &pred);
  (buf, profile)
}

/// Records what the sweep will make a scanner do on `buf`.
///
/// Computed with the **scalar predicate**, never with the library, so a broken
/// kernel cannot make a dead corpus look alive.
fn profile_of(buf: &[u8], pred: impl Fn(u8) -> bool) -> String {
  // Replay the sweep exactly as the benched loop will walk it.
  let mut pos = 0usize;
  let mut advances = Vec::new();
  while pos < SCAN_LIMIT {
    let advanced = scalar_prefix_len(&buf[pos..], &pred);
    advances.push(advanced);
    pos += advanced + 1;
  }

  // Emit the whole histogram rather than a summary. Whether a corpus supports
  // a given comparison depends on the widths being compared, which the bench
  // does not know and the reporter does; recording the raw distribution lets
  // the reporter ask its own question instead of trusting a statistic chosen
  // here for a different one.
  let calls = advances.len();
  let mut histogram: Vec<usize> = advances.clone();
  histogram.sort_unstable();
  let mut pairs: Vec<String> = Vec::new();
  let mut i = 0;
  while i < histogram.len() {
    let length = histogram[i];
    let mut count = 0;
    while i < histogram.len() && histogram[i] == length {
      count += 1;
      i += 1;
    }
    pairs.push(format!("\"{length}\": {count}"));
  }
  format!(
    "\"calls\": {}, \"buf_len\": {}, \"advances\": {{{}}}",
    calls,
    buf.len(),
    pairs.join(", ")
  )
}

/// The aggregate shape a lexer actually runs: a stream of short tokens, each
/// scanned from a cursor that always has a long tail behind it, with the run
/// length **varying** from call to call.
///
/// That variation is the whole point, and it is what the `short_run/*` group
/// above cannot show: there a single run length repeats for millions of
/// iterations, so every branch in the scanner is perfectly predicted. A real
/// lexer never gets that. Any scanner whose short-run path spends branches
/// proportional to the run length pays for them here and nowhere else.
///
/// The buffer is 1 MiB but only the first 64 KiB of cursor positions are
/// scanned, so every call sees at least ~960 KiB of remaining slice — the
/// tail never shrinks into the scalar-threshold band — and the timer is
/// amortized over ~20k calls.
fn bench_lexer_sweep(c: &mut Criterion) {
  fn one<F, P>(
    c: &mut Criterion,
    group_prefix: &str,
    buf: &[u8],
    name: &str,
    memspan_fn: F,
    pred: P,
  ) where
    F: Fn(&[u8]) -> usize + Copy,
    P: Fn(u8) -> bool + Copy,
  {
    let mut group = c.benchmark_group(format!("{group_prefix}/{name}"));
    group.throughput(Throughput::Bytes(SCAN_LIMIT as u64));

    group.bench_with_input(BenchmarkId::new("memspan", SCAN_LIMIT), &buf, |b, buf| {
      b.iter(|| black_box(sweep(black_box(buf), memspan_fn)))
    });

    group.bench_with_input(BenchmarkId::new("scalar", SCAN_LIMIT), &buf, |b, buf| {
      b.iter(|| black_box(sweep(black_box(buf), |s| scalar_prefix_len(s, pred))))
    });

    group.bench_with_input(
      BenchmarkId::new("probe_then", SCAN_LIMIT),
      &buf,
      |b, buf| b.iter(|| black_box(sweep(black_box(buf), |s| probe_then(s, pred, memspan_fn)))),
    );

    group.finish();
  }

  fn sweep_one<F, P>(
    c: &mut Criterion,
    profiles: &mut Vec<String>,
    name: &str,
    memspan_fn: F,
    pred: P,
    fill: u8,
    miss: u8,
  ) where
    F: Fn(&[u8]) -> usize + Copy,
    P: Fn(u8) -> bool + Copy,
  {
    let (buf, profile) = corpus_for(pred, fill, miss);
    profiles.push(format!("  \"{name}\": {{{profile}}}"));
    one(c, "lexer_sweep", &buf, name, memspan_fn, pred);
  }

  let mut profiles: Vec<String> = Vec::new();

  // ── lexer_sweep: every `skip_ascii_class!` kernel, and nothing else ────────
  //
  // These are the only scanners whose scalar probe is sized by `CLASS_PROBE`.
  // The probe-sweep workflow selects this group by name, and
  // `ci/probe_sweep.py` asserts that the names here are *exactly* the macro
  // invocations the kernels are generated from — not a subset of them, and not
  // a superset. That equality is what makes a deleted row a red gate instead of
  // a shorter table.
  //
  // Set equality is necessary and not sufficient. It proves a kernel was named
  // and produced a cell; it cannot prove the cell measured anything, because a
  // kernel that never advances past byte 0 still produces one. The per-class
  // fill and miss bytes below, and the corpus profile written beside the
  // results, are what close that gap.
  sweep_classes!(
    c,
    &mut profiles,
    ("skip_binary", skip::skip_binary, is_binary, b'1', b'2'),
    (
      "skip_octal_digits",
      skip::skip_octal_digits,
      is_octal,
      b'7',
      b'8'
    ),
    ("skip_digits", skip::skip_digits, is_digit, b'9', b'a'),
    ("skip_hex_digits", skip::skip_hex_digits, is_hex, b'F', b'g'),
    ("skip_alpha", skip::skip_alpha, is_alphabetic, b'q', b'0'),
    (
      "skip_alphanumeric",
      skip::skip_alphanumeric,
      is_alphanumeric,
      b'q',
      b'_'
    ),
    (
      "skip_ident_start",
      skip::skip_ident_start,
      is_ident_start,
      b'_',
      b'0'
    ),
    ("skip_ident", skip::skip_ident, is_ident, b'_', b'-'),
    (
      "skip_whitespace",
      skip::skip_whitespace,
      is_space,
      b'\t',
      b'x'
    ),
    ("skip_lower", skip::skip_lower, is_lower, b'z', b'Z'),
    ("skip_upper", skip::skip_upper, is_upper, b'Z', b'z'),
    ("skip_ascii", skip::skip_ascii, is_ascii_byte, b'~', 0x80),
    (
      "skip_non_ascii",
      skip::skip_non_ascii,
      is_non_ascii,
      0xFF,
      b'a'
    ),
    (
      "skip_ascii_graphic",
      skip::skip_ascii_graphic,
      is_graphic,
      b'!',
      b' '
    ),
    (
      "skip_ascii_control",
      skip::skip_ascii_control,
      is_control,
      0x7F,
      b'a'
    ),
  );

  write_profile("corpus-profile.json", &profiles);

  // ── realistic_sweep: the corpus the merged NEON narrowing was decided on ───
  //
  // `lexer_sweep` above trades realism for control: its runs are synthetic so
  // that every class gets the same length schedule and every compared width is
  // reachable. That makes it the wrong place to reproduce the numbers in the
  // CHANGELOG, which were measured on this PromQL fragment before the group was
  // repurposed. Those numbers name *this* group, and it carries exactly the
  // classes they cite so each one can be re-derived by name.
  //
  // Its profile is written out too. The CHANGELOG marks the `skip_whitespace`
  // row as not evidence about probe width; that is a claim about this corpus,
  // so the corpus records the counts that justify it instead of asking a reader
  // to take the prose on trust.
  const FRAGMENT: &[u8] =
    b"sum by (job) rate(http_requests_total{code=~\"5..\"}[5m]) / 1024 + x_7 ";
  let realistic: Vec<u8> = FRAGMENT.iter().copied().cycle().take(1024 * 1024).collect();

  let mut realistic_profiles: Vec<String> = Vec::new();
  macro_rules! realistic_classes {
    ($(($name:literal, $scanner:path, $pred:path)),+ $(,)?) => {
      $(
        realistic_profiles.push(format!("  \"{}\": {{{}}}", $name, profile_of(&realistic, $pred)));
        one(c, "realistic_sweep", &realistic, $name, $scanner, $pred);
      )+
    };
  }
  realistic_classes!(
    ("skip_ident", skip::skip_ident, is_ident),
    ("skip_alpha", skip::skip_alpha, is_alphabetic),
    ("skip_hex_digits", skip::skip_hex_digits, is_hex),
    ("skip_digits", skip::skip_digits, is_digit),
    ("skip_whitespace", skip::skip_whitespace, is_space),
  );
  write_profile("corpus-profile-realistic.json", &realistic_profiles);

  // ── generic_sweep: the multi-needle scanners ───────────────────────────────
  //
  // `skip_while` and `skip_until` still probe a whole `CHUNK` and never consult
  // `CLASS_PROBE`, so the sweep cannot move them. They stay benched — they were
  // the evidence that the defect is specific to the class kernels rather than
  // general to the dispatcher — but under a group name the sweep does not
  // select.
  const NEEDLES: [u8; 10] = *b"0123456789";
  one(
    c,
    "generic_sweep",
    &realistic,
    "skip_while_10",
    |s| skip::skip_while(s, NEEDLES),
    is_digit,
  );
  one(
    c,
    "generic_sweep",
    &realistic,
    "skip_until_newline",
    skip::skip_until_newline,
    |c| c != b'\n',
  );
}

/// Writes the corpus profile beside the criterion results.
///
/// `ci/probe_sweep.py` refuses to score a row whose corpus never made the
/// scanner classify anything, and this is where it learns that. Emitting it
/// from the bench rather than recomputing it in the reporter keeps one
/// definition of what the sweep actually walked.
fn write_profile(file: &str, profiles: &[String]) {
  let home = std::env::var("CRITERION_HOME").unwrap_or_else(|_| "target/criterion".into());
  let path = std::path::Path::new(&home).join(file);
  if let Some(parent) = path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  let body = format!("{{\n{}\n}}\n", profiles.join(",\n"));
  if let Err(err) = std::fs::write(&path, body) {
    // A missing profile fails the reporter, so a warning here is enough; the
    // run should not die before producing the timings themselves.
    eprintln!("warning: could not write {}: {err}", path.display());
  }
}

/// Independent scalar spellings of each class, written from its documented
/// definition rather than reused from the library, so the bench's reference
/// loop is a genuine second opinion about membership.
#[inline(always)]
fn is_binary(b: u8) -> bool {
  b == b'0' || b == b'1'
}

#[inline(always)]
fn is_octal(b: u8) -> bool {
  (b'0'..=b'7').contains(&b)
}

#[inline(always)]
fn is_hex(b: u8) -> bool {
  b.is_ascii_hexdigit()
}

#[inline(always)]
fn is_alphabetic(b: u8) -> bool {
  b.is_ascii_alphabetic()
}

#[inline(always)]
fn is_alphanumeric(b: u8) -> bool {
  b.is_ascii_alphanumeric()
}

#[inline(always)]
fn is_ident_start(b: u8) -> bool {
  b.is_ascii_alphabetic() || b == b'_'
}

#[inline(always)]
fn is_lower(b: u8) -> bool {
  b.is_ascii_lowercase()
}

#[inline(always)]
fn is_upper(b: u8) -> bool {
  b.is_ascii_uppercase()
}

#[inline(always)]
fn is_ascii_byte(b: u8) -> bool {
  b.is_ascii()
}

#[inline(always)]
fn is_non_ascii(b: u8) -> bool {
  !b.is_ascii()
}

#[inline(always)]
fn is_graphic(b: u8) -> bool {
  (0x21..=0x7E).contains(&b)
}

#[inline(always)]
fn is_control(b: u8) -> bool {
  b <= 0x1F || b == 0x7F
}

criterion_group!(
  benches,
  bench_classes,
  bench_skip_while,
  bench_skip_until,
  bench_lexer_sweep
);
criterion_main!(benches);
