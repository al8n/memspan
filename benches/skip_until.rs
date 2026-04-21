use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use skipchr::{Needles, skip};
use std::hint::black_box;

/// Sizes for the per-call micro benches. Includes points around the SIMD
/// dispatch threshold (`NEON_CHUNK_SIZE = 16`) so the boundary cost is visible.
const MICRO_LENGTHS: [usize; 8] = [15, 16, 17, 32, 64, 256, 4 * 1024, 64 * 1024];

/// Sizes for the workload benches. Two pages-ish targets, one well past LLC.
const WORKLOAD_LENGTHS: [usize; 2] = [8 * 1024, 128 * 1024];

/// Gap between consecutive needle hits used by the density-sweep bench. Smaller
/// values flip the SIMD-vs-scalar verdict in favor of the scalar path because
/// the per-call constant dominates.
const DENSITY_GAPS: [usize; 5] = [4, 16, 64, 256, 1024];

fn input_with_match_at_end(len: usize, fill: u8, needle: u8) -> Vec<u8> {
  let mut input = vec![fill; len];

  if let Some(last) = input.last_mut() {
    *last = needle;
  }

  input
}

/// Build a `len`-byte buffer where `needle` appears at every `gap` bytes.
fn input_with_periodic_match(len: usize, fill: u8, needle: u8, gap: usize) -> Vec<u8> {
  assert!(gap >= 1, "gap must be at least 1");
  let mut input = vec![fill; len];
  let mut pos = gap.saturating_sub(1);
  while pos < len {
    input[pos] = needle;
    pos += gap;
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

fn scalar_tail_find<Nd>(input: &[u8], needles: Nd) -> Option<usize>
where
  Nd: Needles,
{
  if needles.is_empty() {
    return None;
  }

  needles.tail_find(input)
}

fn scan_all_dispatch<Nd>(input: &[u8], needles: Nd) -> usize
where
  Nd: Needles + Copy,
{
  let mut pos = 0;
  let mut checksum = 0usize;

  while pos < input.len() {
    let Some(hit) = skip::skip_until(&input[pos..], needles) else {
      break;
    };

    pos += hit;
    checksum = checksum.wrapping_add(pos);
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
    let Some(hit) = scalar_tail_find(&input[pos..], needles) else {
      break;
    };

    pos += hit;
    checksum = checksum.wrapping_add(pos);
    pos += 1;
  }

  checksum
}

/// Per-call micro: full-length scan, only one match at the very end. Measures
/// raw throughput of a single `skip_until` call across a range of sizes that
/// straddle the SIMD dispatch threshold.
fn bench_micro_end_match(c: &mut Criterion) {
  const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];

  let mut group = c.benchmark_group("skip_until/micro/end_match");

  for len in MICRO_LENGTHS {
    let input = input_with_match_at_end(len, b'a', b'\n');
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(
      BenchmarkId::new("dispatch_fixed", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(skip::skip_until(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );

    group.bench_with_input(
      BenchmarkId::new("scalar_tail_find", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scalar_tail_find(
            black_box(input.as_slice()),
            black_box(NEEDLES),
          ))
        })
      },
    );
  }

  group.finish();
}

/// Density sweep on a fixed buffer: same input length, varying gap between
/// needle hits. This isolates the variable that flips the SIMD-vs-scalar
/// verdict and exposes where the crossover lives.
fn bench_density_sweep(c: &mut Criterion) {
  const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];
  const LEN: usize = 64 * 1024;

  let mut group = c.benchmark_group("skip_until/density_sweep");

  for gap in DENSITY_GAPS {
    let input = input_with_periodic_match(LEN, b'a', b'\n', gap);
    group.throughput(Throughput::Bytes(LEN as u64));

    group.bench_with_input(
      BenchmarkId::new("dispatch_fixed", gap),
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
      BenchmarkId::new("scalar_tail_find", gap),
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

fn bench_graphql_ignored_workload(c: &mut Criterion) {
  const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];
  const FRAGMENT: &[u8] = br#"
query GetUser($id: ID!, $limit: Int = 10) {
  user(id: $id) {
    id
    name
    friends(first: $limit, after: null) {
      nodes {
        id
        name
        profile { avatarUrl(size: 64), bio }
      }
    }
  }
}

fragment UserFields on User {
  id
  name
  email
}
"#;

  let mut group = c.benchmark_group("skip_until/workload/graphql_ignored_scan_all");

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
      BenchmarkId::new("scalar_tail_find", len),
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

fn bench_quote_or_escape_workload(c: &mut Criterion) {
  const NEEDLES: [u8; 2] = [b'"', b'\\'];
  const FRAGMENT: &[u8] =
    br#"name: "Ada Lovelace", bio: "first programmer \"notes\" \\ archive", city: "London"
"#;

  let mut group = c.benchmark_group("skip_until/workload/quote_or_escape_scan_all");

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
      BenchmarkId::new("scalar_tail_find", len),
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

    // Direct memchr2 baseline: bypasses the Needles trait entirely so any
    // wrapper overhead shows up as the gap between this row and
    // `dispatch_fixed`.
    group.bench_with_input(
      BenchmarkId::new("memchr2_direct", len),
      &input,
      |b, input| {
        b.iter(|| {
          let buf: &[u8] = black_box(input.as_slice());
          let mut pos = 0;
          let mut checksum = 0usize;
          while pos < buf.len() {
            let Some(hit) = memchr::memchr2(b'"', b'\\', &buf[pos..]) else {
              break;
            };
            pos += hit;
            checksum = checksum.wrapping_add(pos);
            pos += 1;
          }
          black_box(checksum)
        })
      },
    );
  }

  group.finish();
}

fn bench_comment_newline_workload(c: &mut Criterion) {
  const FRAGMENT: &[u8] = br#"# warm cache comment with enough text to scan before newline
# another GraphQL comment that ends at the next line terminator
# short
"#;

  let mut group = c.benchmark_group("skip_until/workload/comment_newline_scan_all");

  for len in WORKLOAD_LENGTHS {
    let input = repeat_to_len(FRAGMENT, len);
    let dynamic_needle: &[u8] = b"\n";
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(BenchmarkId::new("dispatch_u8", len), &input, |b, input| {
      b.iter(|| {
        black_box(scan_all_dispatch(
          black_box(input.as_slice()),
          black_box(b'\n'),
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
      BenchmarkId::new("scalar_tail_find", len),
      &input,
      |b, input| {
        b.iter(|| {
          black_box(scan_all_scalar(
            black_box(input.as_slice()),
            black_box(b'\n'),
          ))
        })
      },
    );

    // Direct memchr baseline.
    group.bench_with_input(
      BenchmarkId::new("memchr_direct", len),
      &input,
      |b, input| {
        b.iter(|| {
          let buf: &[u8] = black_box(input.as_slice());
          let mut pos = 0;
          let mut checksum = 0usize;
          while pos < buf.len() {
            let Some(hit) = memchr::memchr(b'\n', &buf[pos..]) else {
              break;
            };
            pos += hit;
            checksum = checksum.wrapping_add(pos);
            pos += 1;
          }
          black_box(checksum)
        })
      },
    );
  }

  group.finish();
}

criterion_group!(
  benches,
  bench_micro_end_match,
  bench_density_sweep,
  bench_graphql_ignored_workload,
  bench_quote_or_escape_workload,
  bench_comment_newline_workload
);
criterion_main!(benches);
