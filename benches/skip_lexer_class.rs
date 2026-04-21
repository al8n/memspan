//! Benchmarks for the lexer-class `skip_*` family
//! (`skip_whitespace`, `skip_alpha`, `skip_alphanumeric`,
//! `skip_ident_start`, `skip_ident`).
//!
//! Each fn is compared against the closest equivalent the user could write
//! today: `skip_while` with the same set as a needle array (the dispatch
//! cost is paid against the trait, plus the dynamic >8-needle NEON loop for
//! `alphanumeric`/`ident_start`/`ident`), and a hand-written
//! `iter().position` loop with the equivalent predicate (the auto-vec
//! baseline).

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use memspan::{Needles, skip};
use std::hint::black_box;

const MICRO_LENGTHS: [usize; 6] = [16, 32, 64, 256, 4 * 1024, 64 * 1024];
const DENSITY_RUNS: [usize; 5] = [4, 16, 64, 256, 1024];
const SWEEP_LEN: usize = 64 * 1024;

fn input_with_miss_at_end(len: usize, fill: u8, miss: u8) -> Vec<u8> {
  let mut input = vec![fill; len];
  if let Some(last) = input.last_mut() {
    *last = miss;
  }
  input
}

fn input_with_periodic_miss(len: usize, fill: u8, miss: u8, run: usize) -> Vec<u8> {
  let stride = run + 1;
  let mut input = vec![fill; len];
  let mut pos = run;
  while pos < len {
    input[pos] = miss;
    pos += stride;
  }
  input
}

fn repeat_to_len(fragment: &[u8], target_len: usize) -> Vec<u8> {
  let mut input = Vec::with_capacity(target_len + fragment.len());
  while input.len() < target_len {
    input.extend_from_slice(fragment);
  }
  input.truncate(target_len);
  input
}

fn scan_all_specialized<F>(input: &[u8], f: F) -> usize
where
  F: Fn(&[u8]) -> usize,
{
  let mut pos = 0;
  let mut checksum = 0usize;
  while pos < input.len() {
    let advanced = f(&input[pos..]);
    pos += advanced;
    checksum = checksum.wrapping_add(pos);
    pos += 1;
  }
  checksum
}

fn scan_all_skip_while<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles + Copy,
{
  let mut pos = 0;
  let mut checksum = 0usize;
  while pos < input.len() {
    let advanced = skip::skip_while(&input[pos..], needles);
    pos += advanced;
    checksum = checksum.wrapping_add(pos);
    pos += 1;
  }
  checksum
}

#[inline(always)]
fn scalar_prefix_len_by(input: &[u8], pred: impl Fn(u8) -> bool) -> usize {
  input.iter().position(|&b| !pred(b)).unwrap_or(input.len())
}

#[inline(always)]
fn is_ws(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}
#[inline(always)]
fn is_alpha(b: u8) -> bool {
  let lower = b | 0x20;
  lower.is_ascii_lowercase()
}
#[inline(always)]
fn is_alphanumeric(b: u8) -> bool {
  is_alpha(b) || b.is_ascii_digit()
}
#[inline(always)]
fn is_ident_start(b: u8) -> bool {
  is_alpha(b) || b == b'_'
}
#[inline(always)]
fn is_ident(b: u8) -> bool {
  is_alphanumeric(b) || b == b'_'
}

fn bench_one_micro<F, P, const N: usize>(
  c: &mut Criterion,
  group_name: &str,
  specialized: F,
  needles: [u8; N],
  pred: P,
  fill: u8,
  miss: u8,
) where
  F: Fn(&[u8]) -> usize + Copy,
  P: Fn(u8) -> bool + Copy,
{
  let mut group = c.benchmark_group(group_name);

  for len in MICRO_LENGTHS {
    let input = input_with_miss_at_end(len, fill, miss);
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(BenchmarkId::new("specialized", len), &input, |b, input| {
      b.iter(|| black_box(specialized(black_box(input.as_slice()))))
    });

    group.bench_with_input(
      BenchmarkId::new("skip_while_arr", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(skip::skip_while(
            black_box(input.as_slice()),
            black_box(needles),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_predicate", len),
      &input,
      |b, input| b.iter(|| black_box(scalar_prefix_len_by(black_box(input.as_slice()), pred))),
    );
  }

  group.finish();
}

fn bench_one_density<F, P, const N: usize>(
  c: &mut Criterion,
  group_name: &str,
  specialized: F,
  needles: [u8; N],
  pred: P,
  fill: u8,
  miss: u8,
) where
  F: Fn(&[u8]) -> usize + Copy,
  P: Fn(u8) -> bool + Copy,
{
  let mut group = c.benchmark_group(group_name);

  for run in DENSITY_RUNS {
    let input = input_with_periodic_miss(SWEEP_LEN, fill, miss, run);
    group.throughput(Throughput::Bytes(SWEEP_LEN as u64));

    group.bench_with_input(BenchmarkId::new("specialized", run), &input, |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(
          black_box(input.as_slice()),
          specialized,
        ))
      })
    });

    group.bench_with_input(
      BenchmarkId::new("skip_while_arr", run),
      &input,
      |b, input| b.iter(|| black_box(scan_all_skip_while(black_box(input.as_slice()), needles))),
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_predicate", run),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_specialized(black_box(input.as_slice()), |s| {
            scalar_prefix_len_by(s, pred)
          }))
        })
      },
    );
  }

  group.finish();
}

const WS_NEEDLES: [u8; 4] = [b' ', b'\t', b'\n', b'\r'];

const ALPHA_NEEDLES: [u8; 52] = *b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

const ALNUM_NEEDLES: [u8; 62] = *b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

const IDENT_START_NEEDLES: [u8; 53] = *b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";

const IDENT_NEEDLES: [u8; 63] = *b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";

fn bench_whitespace(c: &mut Criterion) {
  bench_one_micro(
    c,
    "skip_whitespace/micro/full_match",
    skip::skip_whitespace,
    WS_NEEDLES,
    is_ws,
    b' ',
    b'a',
  );
  bench_one_density(
    c,
    "skip_whitespace/density_sweep",
    skip::skip_whitespace,
    WS_NEEDLES,
    is_ws,
    b' ',
    b'a',
  );
}

fn bench_alpha(c: &mut Criterion) {
  bench_one_micro(
    c,
    "skip_alpha/micro/full_match",
    skip::skip_alpha,
    ALPHA_NEEDLES,
    is_alpha,
    b'A',
    b'1',
  );
  bench_one_density(
    c,
    "skip_alpha/density_sweep",
    skip::skip_alpha,
    ALPHA_NEEDLES,
    is_alpha,
    b'a',
    b'1',
  );
}

fn bench_alphanumeric(c: &mut Criterion) {
  bench_one_micro(
    c,
    "skip_alphanumeric/micro/full_match",
    skip::skip_alphanumeric,
    ALNUM_NEEDLES,
    is_alphanumeric,
    b'a',
    b'-',
  );
  bench_one_density(
    c,
    "skip_alphanumeric/density_sweep",
    skip::skip_alphanumeric,
    ALNUM_NEEDLES,
    is_alphanumeric,
    b'a',
    b'-',
  );
}

fn bench_ident_start(c: &mut Criterion) {
  bench_one_micro(
    c,
    "skip_ident_start/micro/full_match",
    skip::skip_ident_start,
    IDENT_START_NEEDLES,
    is_ident_start,
    b'a',
    b'1',
  );
  bench_one_density(
    c,
    "skip_ident_start/density_sweep",
    skip::skip_ident_start,
    IDENT_START_NEEDLES,
    is_ident_start,
    b'a',
    b'1',
  );
}

fn bench_ident(c: &mut Criterion) {
  bench_one_micro(
    c,
    "skip_ident/micro/full_match",
    skip::skip_ident,
    IDENT_NEEDLES,
    is_ident,
    b'_',
    b'-',
  );
  bench_one_density(
    c,
    "skip_ident/density_sweep",
    skip::skip_ident,
    IDENT_NEEDLES,
    is_ident,
    b'_',
    b'-',
  );
}

/// Pretty-printed JSON with mixed identifier-like keys, whitespace, and
/// punctuation. Exercises `skip_whitespace` in a tight scan-all loop the way
/// a real lexer would.
fn bench_workload_pretty_json_whitespace(c: &mut Criterion) {
  const FRAGMENT: &[u8] = br#"{
    "id": 42,
    "name": "Ada Lovelace",
    "tags": ["math", "engine", "first"],
    "address": { "city": "London", "zip":  "NW1" },
    "notes": "x"
}
"#;
  let input = repeat_to_len(FRAGMENT, SWEEP_LEN);

  let mut group = c.benchmark_group("skip_whitespace/workload/pretty_json");
  group.throughput(Throughput::Bytes(SWEEP_LEN as u64));

  group.bench_with_input(
    BenchmarkId::new("specialized", SWEEP_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(
          black_box(input.as_slice()),
          skip::skip_whitespace,
        ))
      })
    },
  );
  group.bench_with_input(
    BenchmarkId::new("skip_while_arr", SWEEP_LEN),
    &input,
    |b, input| b.iter(|| black_box(scan_all_skip_while(black_box(input.as_slice()), WS_NEEDLES))),
  );
  group.bench_with_input(
    BenchmarkId::new("scalar_predicate", SWEEP_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(black_box(input.as_slice()), |s| {
          scalar_prefix_len_by(s, is_ws)
        }))
      })
    },
  );
  group.finish();
}

/// Stream of identifiers separated by punctuation. Mimics tokenizing a
/// programming-language source file.
fn bench_workload_ident_stream(c: &mut Criterion) {
  const FRAGMENT: &[u8] =
    b"foo bar_baz QUUX1 _hidden var2 longer_name_42 if else struct return XyzAbc data ";
  let input = repeat_to_len(FRAGMENT, SWEEP_LEN);

  let mut group = c.benchmark_group("skip_ident/workload/source_stream");
  group.throughput(Throughput::Bytes(SWEEP_LEN as u64));

  group.bench_with_input(
    BenchmarkId::new("specialized", SWEEP_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(
          black_box(input.as_slice()),
          skip::skip_ident,
        ))
      })
    },
  );
  group.bench_with_input(
    BenchmarkId::new("skip_while_arr", SWEEP_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_skip_while(
          black_box(input.as_slice()),
          IDENT_NEEDLES,
        ))
      })
    },
  );
  group.bench_with_input(
    BenchmarkId::new("scalar_predicate", SWEEP_LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(black_box(input.as_slice()), |s| {
          scalar_prefix_len_by(s, is_ident)
        }))
      })
    },
  );
  group.finish();
}

criterion_group!(
  benches,
  bench_whitespace,
  bench_alpha,
  bench_alphanumeric,
  bench_ident_start,
  bench_ident,
  bench_workload_pretty_json_whitespace,
  bench_workload_ident_stream
);
criterion_main!(benches);
