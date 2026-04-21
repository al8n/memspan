//! Quick perf check: macro-generated skip fn vs the equivalent
//! `skip::skip_while` call with a needle array. The macro generates a
//! per-class specialization, so it should match (or beat) the generic path
//! at every size.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lexsimd::skip;
use std::hint::black_box;

lexsimd::skip_class! {
  /// Whitespace plus a comma separator — a small (5-byte) class that
  /// exemplifies the macro-vs-`skip_while` comparison.
  pub fn skip_ws_and_comma, bytes = [b' ', b'\t', b'\r', b'\n', b','];
}

const NEEDLES: [u8; 5] = [b' ', b'\t', b'\r', b'\n', b','];

fn input_with_miss_at_end(len: usize, fill: u8, miss: u8) -> Vec<u8> {
  let mut input = vec![fill; len];
  if let Some(last) = input.last_mut() {
    *last = miss;
  }
  input
}

fn bench_macro_vs_skip_while(c: &mut Criterion) {
  let mut group = c.benchmark_group("macro_vs_skip_while/full_match");

  for len in [16usize, 32, 64, 256, 4096, 65536] {
    let input = input_with_miss_at_end(len, b' ', b'a');
    group.throughput(Throughput::Bytes(len as u64));

    group.bench_with_input(
      BenchmarkId::new("macro_skip_class", len),
      &input,
      |b, input| b.iter(|| black_box(skip_ws_and_comma(black_box(input.as_slice())))),
    );

    group.bench_with_input(
      BenchmarkId::new("skip_while_arr", len),
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
  }

  group.finish();
}

criterion_group!(benches, bench_macro_vs_skip_while);
criterion_main!(benches);
