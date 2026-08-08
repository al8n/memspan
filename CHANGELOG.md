# UNRELEASED

PERFORMANCE

1. Narrow the NEON ASCII-class scalar probe from 16 bytes to 8 ([#13])
   - Affects **aarch64 only**. No other backend's probe width changes.
   - The probe scans a slice of compile-time-constant length, so LLVM unrolls it and
     rebuilds each copy as a short-circuit branch tree. Run lengths that vary from token
     to token then enter a different copy on every call and mispredict in all of them.
     The cost tracked the number of predicate terms rather than the number of bytes, so
     it hit `skip_ident` hardest and left single-term classes alone. Halving the probe
     halves the trees.
   - Measured with `cargo bench --bench short_run`, group `lexer_sweep` — a 1 MiB buffer
     with the cursor swept over the first 64 KiB, so every call asks about a short run
     while the slice it is handed stays ~960 KiB long. Host: Apple M4 Pro,
     aarch64-apple-darwin, **not an idle machine**; figures are the mean of two rounds
     run alternately before/after so both share conditions, and the round-to-round
     spread on that host was 1-5%.

     | `lexer_sweep` group | before | after | vs. plain scalar loop |
     |---|---|---|---|
     | `skip_ident` | 47.45 us | 25.83 us | 1.92x -> 1.03x |
     | `skip_hex_digits` | 35.94 us | 36.56 us | 1.16x -> 1.20x |
     | `skip_digits` | 34.63 us | 35.95 us | 1.04x -> 1.07x |
     | `skip_alpha` | 24.59 us | 25.64 us | 1.04x -> 1.10x |
     | `skip_whitespace` | 34.95 us | 35.25 us | 0.99x -> 1.00x |

     The `long_run` group — one run covering a whole 64 KiB slice, the shape the dispatch
     threshold was tuned for — moved between -0.2% and +3.4% across its five scanners,
     i.e. within that spread.

     Every ratio above is a same-run comparison against the scalar loop the bench
     measures alongside each scanner, so none of it depends on machine state being
     comparable between runs. Re-run the bench to reproduce; the numbers describe this
     host and this corpus and should not be read as portable.
   - Adds `benches/short_run.rs`, which covers that call shape, and
     `tests/short_run_differential.rs`, which holds every class against an independent
     scalar oracle across lengths, alignments, pseudo-random corpora, and all 256 byte
     values at offsets the vector loop classifies.

2. Add `.github/workflows/probe-sweep.yml`, which times each x86 backend's scalar-probe
   width against a plain scalar loop and reports a table ([#15])
   - The SSE4.2, AVX2 and AVX-512 kernels probe 16, 32 and 64 bytes and have the same
     unrolled-branch-tree exposure the NEON sweep found, but no maintainer host can time
     them. The workflow produces those numbers so the constants can be tuned from
     measurement in a follow-up rather than guessed now.
   - `--cfg memspan_class_probe="N"` is the measurement hook the sweep drives. It is not a
     tuning API: with the cfg unset every backend keeps the width it ships with, verified
     byte-identical in the generated code on x86_64 and aarch64.
   - Correction to the table in entry 1: the `skip_whitespace` row (0.99x -> 1.00x) was not
     meaningful evidence about probe width. On the PromQL corpus that class advances at most
     one byte and takes only two distinct run lengths, so the scanner never reaches the code
     the probe governs. The `skip_ident`, `skip_digits`, `skip_hex_digits` and `skip_alpha`
     rows were genuinely exercised and stand; re-measured on the same corpus, `skip_ident`
     reads 1.96x -> 1.08x against the 1.92x -> 1.03x reported. The narrowing itself is
     unaffected — it was decided on `skip_ident`.
   - The sweep bench now builds a corpus per class and refuses to score a row whose corpus
     never made the scanner classify anything.

[#13]: https://github.com/al8n/memspan/issues/13
[#15]: https://github.com/al8n/memspan/pull/15

# 0.1.0 (April 22nd, 2026)

FEATURES

1. SIMD-accelerated byte-class scanning for lexers and parsers ([#1])
   - Built-in ASCII class functions: `skip_binary`, `skip_octal_digits`, `skip_digits`,
     `skip_hex_digits`, `skip_alpha`, `skip_alphanumeric`, `skip_ident_start`, `skip_ident`,
     `skip_whitespace`, `skip_lower`, `skip_upper`, `skip_ascii`, `skip_non_ascii`,
     `skip_ascii_graphic`, `skip_ascii_control`
   - Generic multi-needle operations: `skip_while`, `skip_until`, `count_matches`, `find_last`
   - `skip_class!` macro for defining custom byte classes with the same SIMD dispatch as built-ins
   - `Needles` trait accepting `u8`, `[u8; N]`, and `&[u8]`
   - Runtime dispatch across AVX-512BW → AVX2 → SSE4.2 → scalar on x86/x86\_64,
     NEON → scalar on aarch64, SIMD128 → scalar on wasm32
   - Zero-allocation, `no_std`-compatible

[#1]: https://github.com/al8n/memspan/pull/1
