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
//!   general to the dispatcher, but held out of the sweep: a row the sweep
//!   cannot move contributes only noise, and that noise would widen the spread
//!   the report thresholds against.
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
  const BUF_LEN: usize = 1024 * 1024;
  const FRAGMENT: &[u8] =
    b"sum by (job) rate(http_requests_total{code=~\"5..\"}[5m]) / 1024 + x_7 ";

  let buf: Vec<u8> = FRAGMENT.iter().copied().cycle().take(BUF_LEN).collect();

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

  const NEEDLES: [u8; 10] = *b"0123456789";

  // ── lexer_sweep: ASCII-class scanners only ─────────────────────────────────
  //
  // These are the `skip_ascii_class!` kernels, and they are the only scanners
  // whose scalar probe is sized by `CLASS_PROBE`. The probe-sweep workflow
  // selects this group by name and `ci/probe_sweep.py` cross-checks every row
  // in it against the macro invocations the kernels are generated from, so a
  // scanner that does not read the constant must not be filed here — its noise
  // would be scored as probe-width evidence and would widen the spread the
  // report thresholds against.
  one(
    c,
    "lexer_sweep",
    &buf,
    "skip_ident",
    skip::skip_ident,
    is_ident,
  );
  one(
    c,
    "lexer_sweep",
    &buf,
    "skip_digits",
    skip::skip_digits,
    is_digit,
  );
  one(
    c,
    "lexer_sweep",
    &buf,
    "skip_whitespace",
    skip::skip_whitespace,
    is_space,
  );
  one(
    c,
    "lexer_sweep",
    &buf,
    "skip_alpha",
    skip::skip_alpha,
    |b| b.is_ascii_alphabetic(),
  );
  one(
    c,
    "lexer_sweep",
    &buf,
    "skip_hex_digits",
    skip::skip_hex_digits,
    |b| b.is_ascii_hexdigit(),
  );

  // ── generic_sweep: the multi-needle scanners ───────────────────────────────
  //
  // `skip_while` and `skip_until` still probe a whole `CHUNK` and never consult
  // `CLASS_PROBE`, so the sweep cannot move them. They stay benched — they were
  // the evidence that the defect is specific to the class kernels rather than
  // general to the dispatcher — but under a group name the sweep does not
  // select.
  one(
    c,
    "generic_sweep",
    &buf,
    "skip_while_10",
    |s| skip::skip_while(s, NEEDLES),
    is_digit,
  );
  one(
    c,
    "generic_sweep",
    &buf,
    "skip_until_newline",
    skip::skip_until_newline,
    |c| c != b'\n',
  );
}

criterion_group!(
  benches,
  bench_classes,
  bench_skip_while,
  bench_skip_until,
  bench_lexer_sweep
);
criterion_main!(benches);
