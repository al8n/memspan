//! Benchmarks for the ASCII-class `skip_*` family
//! (`skip_binary`, `skip_octal_digits`, `skip_digits`, `skip_hex_digits`).
//!
//! Each specialization is compared against three references:
//! * `skip_while_arr` — `skip::skip_while` with the equivalent array of
//!   needle bytes. For ≥4 needles this exercises the dynamic NEON loop with
//!   N×`vceqq` + (N-1)×`vorrq` per chunk; for ≤3 needles it routes to
//!   `memchr`/`memchr2`/`memchr3`.
//! * `scalar_predicate` — a raw `iter().position` loop with the equivalent
//!   predicate, the path the auto-vectorizer compiles to. This is the
//!   apples-to-apples baseline for "what would the user write by hand".
//!
//! Two axes:
//! * `micro/full_match`: a single `skip_*` call on a `len`-byte buffer that
//!   matches everywhere except the last byte. Measures peak SIMD throughput.
//! * `density_sweep`: scan-all over a 64 KB buffer with a controlled match-run
//!   length between misses. Exposes where the per-call dispatch overhead
//!   matters more than the SIMD throughput win.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use skipchr::{Needles, skip};
use std::hint::black_box;

/// Sizes for the per-call micro benches.
const MICRO_LENGTHS: [usize; 6] = [16, 32, 64, 256, 4 * 1024, 64 * 1024];

/// Length of the contiguous matching run between misses for the density sweep.
const DENSITY_RUNS: [usize; 5] = [4, 16, 64, 256, 1024];

/// Sweep buffer length (large enough to dominate per-call costs over many
/// iterations of `scan_all`).
const SWEEP_LEN: usize = 64 * 1024;

// ---- input builders -------------------------------------------------------

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

// ---- scan-all helpers -----------------------------------------------------

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

// ---- scalar reference predicates -----------------------------------------

#[inline(always)]
fn scalar_prefix_len_by(input: &[u8], pred: impl Fn(u8) -> bool) -> usize {
  input.iter().position(|&b| !pred(b)).unwrap_or(input.len())
}

#[inline(always)]
fn is_binary(b: u8) -> bool {
  b == b'0' || b == b'1'
}
#[inline(always)]
fn is_octal(b: u8) -> bool {
  b.is_ascii_digit() && b <= b'7'
}
#[inline(always)]
fn is_digit(b: u8) -> bool {
  b.is_ascii_digit()
}
#[inline(always)]
fn is_hex(b: u8) -> bool {
  b.is_ascii_hexdigit()
}

// ---- bench builder --------------------------------------------------------

/// One micro/full_match group per specialization. `specialized` is the public
/// fn under test; `needles` is the equivalent needle array; `pred` is the
/// scalar predicate; `fill`/`miss` are bytes guaranteed to be in/out of the
/// class.
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

// ---- per-class entry points ----------------------------------------------

fn bench_binary(c: &mut Criterion) {
  // 2 needles → skip_while routes to memchr2 (already SIMD-saturated).
  bench_one_micro(
    c,
    "skip_binary/micro/full_match",
    skip::skip_binary,
    *b"01",
    is_binary,
    b'0',
    b'2',
  );
  bench_one_density(
    c,
    "skip_binary/density_sweep",
    skip::skip_binary,
    *b"01",
    is_binary,
    b'0',
    b'2',
  );
}

fn bench_octal(c: &mut Criterion) {
  // 8 needles → skip_while uses the 8-needle const NEON path.
  bench_one_micro(
    c,
    "skip_octal_digits/micro/full_match",
    skip::skip_octal_digits,
    *b"01234567",
    is_octal,
    b'5',
    b'8',
  );
  bench_one_density(
    c,
    "skip_octal_digits/density_sweep",
    skip::skip_octal_digits,
    *b"01234567",
    is_octal,
    b'5',
    b'8',
  );
}

fn bench_digits(c: &mut Criterion) {
  // 10 needles → skip_while falls through to the dynamic >8-needle NEON loop
  // (10 vceqq + 9 vorrq per chunk). This is where the range-mask
  // specialization should win the most.
  bench_one_micro(
    c,
    "skip_digits/micro/full_match",
    skip::skip_digits,
    *b"0123456789",
    is_digit,
    b'5',
    b'a',
  );
  bench_one_density(
    c,
    "skip_digits/density_sweep",
    skip::skip_digits,
    *b"0123456789",
    is_digit,
    b'5',
    b'a',
  );
}

fn bench_hex(c: &mut Criterion) {
  // 22 needles → dynamic NEON path. Specialized version uses 2 ranges + an
  // OR-with-0x20 case-fold (~7 SIMD ops), much cheaper than 22 vceqq.
  bench_one_micro(
    c,
    "skip_hex_digits/micro/full_match",
    skip::skip_hex_digits,
    *b"0123456789abcdefABCDEF",
    is_hex,
    b'A',
    b'g',
  );
  bench_one_density(
    c,
    "skip_hex_digits/density_sweep",
    skip::skip_hex_digits,
    *b"0123456789abcdefABCDEF",
    is_hex,
    b'A',
    b'g',
  );
}

// ---- realistic numeric workloads ----------------------------------------

/// CSV-like stream of decimal integers, comma-separated.
fn bench_workload_csv_numbers(c: &mut Criterion) {
  // average run length ≈ 5 digits; alternates with a 1-byte non-digit
  // separator. Mimics a typical CSV / log-line numeric column.
  const FRAGMENT: &[u8] =
    b"42,7,12345,9,8675309,1,3141592,27,0,99999,1024,65535,8,16,32,64,128,256,512,4096,";
  const LEN: usize = 64 * 1024;

  let input: Vec<u8> = FRAGMENT.iter().copied().cycle().take(LEN).collect();
  let needles: [u8; 10] = *b"0123456789";

  let mut group = c.benchmark_group("skip_digits/workload/csv_numbers");
  group.throughput(Throughput::Bytes(LEN as u64));

  group.bench_with_input(BenchmarkId::new("specialized", LEN), &input, |b, input| {
    b.iter(|| {
      black_box(scan_all_specialized(
        black_box(input.as_slice()),
        skip::skip_digits,
      ))
    })
  });

  group.bench_with_input(
    BenchmarkId::new("skip_while_arr", LEN),
    &input,
    |b, input| b.iter(|| black_box(scan_all_skip_while(black_box(input.as_slice()), needles))),
  );

  group.bench_with_input(
    BenchmarkId::new("scalar_predicate", LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(black_box(input.as_slice()), |s| {
          scalar_prefix_len_by(s, is_digit)
        }))
      })
    },
  );

  group.finish();
}

/// Stream of `0x`-prefixed hex literals separated by commas — as you'd see in
/// a hex dump or constant table.
fn bench_workload_hex_constants(c: &mut Criterion) {
  const FRAGMENT: &[u8] =
    b"0xdeadbeef,0xCAFEBABE,0x1234abcd,0xFFFF,0x42,0xABCDEF0123,0xDeAdBeEf,0x0,";
  const LEN: usize = 64 * 1024;

  let input: Vec<u8> = FRAGMENT.iter().copied().cycle().take(LEN).collect();
  let needles: [u8; 22] = *b"0123456789abcdefABCDEF";

  let mut group = c.benchmark_group("skip_hex_digits/workload/hex_constants");
  group.throughput(Throughput::Bytes(LEN as u64));

  group.bench_with_input(BenchmarkId::new("specialized", LEN), &input, |b, input| {
    b.iter(|| {
      black_box(scan_all_specialized(
        black_box(input.as_slice()),
        skip::skip_hex_digits,
      ))
    })
  });

  group.bench_with_input(
    BenchmarkId::new("skip_while_arr", LEN),
    &input,
    |b, input| b.iter(|| black_box(scan_all_skip_while(black_box(input.as_slice()), needles))),
  );

  group.bench_with_input(
    BenchmarkId::new("scalar_predicate", LEN),
    &input,
    |b, input| {
      b.iter(|| {
        black_box(scan_all_specialized(black_box(input.as_slice()), |s| {
          scalar_prefix_len_by(s, is_hex)
        }))
      })
    },
  );

  group.finish();
}

criterion_group!(
  benches,
  bench_binary,
  bench_octal,
  bench_digits,
  bench_hex,
  bench_workload_csv_numbers,
  bench_workload_hex_constants
);
criterion_main!(benches);
