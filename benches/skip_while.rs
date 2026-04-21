use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use memspan::{Needles, skip};
use std::hint::black_box;

/// Sizes for the per-call micro benches. Includes points around the SIMD
/// dispatch threshold (`NEON_CHUNK_SIZE = 16`) so the boundary cost is visible.
const MICRO_LENGTHS: [usize; 8] = [15, 16, 17, 32, 64, 256, 4 * 1024, 64 * 1024];

/// Sizes for the workload benches.
const WORKLOAD_LENGTHS: [usize; 2] = [8 * 1024, 128 * 1024];

/// Length of contiguous matching prefix between miss positions in the density
/// sweep. Smaller values stress the per-call constant; larger values stress
/// SIMD throughput.
const DENSITY_RUNS: [usize; 5] = [4, 16, 64, 256, 1024];

/// Build a `len`-byte buffer that is entirely `fill` except the very last byte
/// which is `miss`. Models the worst case for `skip_while`: full-length prefix
/// scan with the first non-match at the very end.
fn input_with_miss_at_end(len: usize, fill: u8, miss: u8) -> Vec<u8> {
  let mut input = vec![fill; len];
  if let Some(last) = input.last_mut() {
    *last = miss;
  }
  input
}

/// Build a `len`-byte buffer where a `miss` byte appears every `run + 1`
/// bytes (so a contiguous matching run of length `run`, then one non-match,
/// repeat).
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

fn scalar_prefix_len<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles,
{
  needles.prefix_len(input)
}

/// Repeatedly skip the leading prefix and step past the non-match, mirroring
/// how a lexer would walk a buffer.
fn scan_all_dispatch<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles + Copy,
{
  let mut pos = 0;
  let mut checksum = 0usize;
  while pos < input.len() {
    let advanced = skip::skip_while(&input[pos..], needles);
    pos += advanced;
    checksum = checksum.wrapping_add(pos);
    // step over the non-match (or the end) to avoid an infinite loop
    pos += 1;
  }
  checksum
}

fn scan_all_scalar<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles + Copy,
{
  let mut pos = 0;
  let mut checksum = 0usize;
  while pos < input.len() {
    let advanced = scalar_prefix_len(&input[pos..], needles);
    pos += advanced;
    checksum = checksum.wrapping_add(pos);
    pos += 1;
  }
  checksum
}

/// Per-call worst case: scan the whole buffer, single non-match at the end.
fn bench_micro_full_match(c: &mut Criterion) {
  const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];

  let mut group = c.benchmark_group("skip_while/micro/full_match");

  for len in MICRO_LENGTHS {
    let input = input_with_miss_at_end(len, b' ', b'a');
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(
      BenchmarkId::new("dispatch_fixed", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(skip::skip_while(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_prefix_len", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scalar_prefix_len(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );
  }

  group.finish();
}

/// Density sweep: vary the length of contiguous matching runs between misses.
/// Short runs stress per-call overhead; long runs stress SIMD throughput.
fn bench_density_sweep(c: &mut Criterion) {
  const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];
  const LEN: usize = 64 * 1024;

  let mut group = c.benchmark_group("skip_while/density_sweep");

  for run in DENSITY_RUNS {
    let input = input_with_periodic_miss(LEN, b' ', b'a', run);
    group.throughput(Throughput::Bytes(LEN as u64));

    group.bench_with_input(
      BenchmarkId::new("dispatch_fixed", run),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_dispatch(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_prefix_len", run),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_scalar(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );
  }

  group.finish();
}

/// Realistic JSON-ish workload: pretty-printed text with a mix of short and
/// long whitespace runs.
fn bench_pretty_json_workload(c: &mut Criterion) {
  const NEEDLES: [u8; 4] = [b' ', b'\t', b'\r', b'\n'];
  const FRAGMENT: &[u8] = br#"{
    "id": 42,
    "name": "Ada Lovelace",
    "tags": ["math", "engine", "first"],
    "address": {
        "city": "London",
        "zip":  "NW1"
    },
    "notes": "x"
}
"#;

  let mut group = c.benchmark_group("skip_while/workload/pretty_json_scan_all");

  for len in WORKLOAD_LENGTHS {
    let input = repeat_to_len(FRAGMENT, len);
    let dynamic_needles: &[u8] = &NEEDLES;
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(
      BenchmarkId::new("dispatch_fixed", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_dispatch(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("dispatch_dynamic", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_dispatch(
            black_box(input.as_slice()),
            black_box(dynamic_needles),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_prefix_len", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_scalar(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );
  }

  group.finish();
}

/// Single-byte needle workload: skipping only spaces, common in CSV/INI etc.
fn bench_single_byte_workload(c: &mut Criterion) {
  const FRAGMENT: &[u8] = b"      key=value     # trailing comment with words           \n";

  let mut group = c.benchmark_group("skip_while/workload/spaces_scan_all");

  for len in WORKLOAD_LENGTHS {
    let input = repeat_to_len(FRAGMENT, len);
    let dynamic_needle: &[u8] = b" ";
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(BenchmarkId::new("dispatch_u8", len), &input, |b, input| {
      b.iter(|| {
        black_box(scan_all_dispatch(
          black_box(input.as_slice()),
          black_box(b' '),
        ))
      })
    });

    group.bench_with_input(
      BenchmarkId::new("dispatch_dynamic", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_dispatch(
            black_box(input.as_slice()),
            black_box(dynamic_needle),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_prefix_len", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_scalar(
            black_box(input.as_slice()),
            black_box(b' '),
          ))
        })
      },
    );
  }

  group.finish();
}

criterion_group!(
  benches,
  bench_micro_full_match,
  bench_density_sweep,
  bench_pretty_json_workload,
  bench_single_byte_workload
);
criterion_main!(benches);
