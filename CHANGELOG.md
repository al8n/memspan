# 0.1.1 (August 9th, 2026)

PERFORMANCE

1. Narrow the NEON ASCII-class scalar probe from 16 bytes to 8 ([#13])
   - Affects **aarch64 only**. No other backend's probe width changes.
   - The probe scans a slice of compile-time-constant length, so LLVM unrolls it and
     rebuilds each copy as a short-circuit branch tree. Run lengths that vary from token
     to token then enter a different copy on every call and mispredict in all of them.
     The cost tracked the number of predicate terms rather than the number of bytes, so
     it hit `skip_ident` hardest and left single-term classes alone. Halving the probe
     halves the trees.
   - Measured with `cargo bench --bench short_run`, group **`realistic_sweep`** — a 1 MiB
     PromQL fragment with the cursor swept over the first 64 KiB, so every call asks about
     a short run while the slice it is handed stays ~960 KiB long. Host: Apple M4 Pro,
     aarch64-apple-darwin, **not an idle machine**; figures are the mean of two rounds
     run alternately before/after so both share conditions, and the round-to-round
     spread on that host was 1-5%.

     These numbers were originally taken under the group name `lexer_sweep`. That name was
     later repurposed to a synthetic wide-run schedule, and the PromQL corpus moved to
     `realistic_sweep`, which carries exactly the classes named below so each row can still
     be re-derived by name. Running `lexer_sweep` reproduces a different corpus.

     | `realistic_sweep` group | before | after | vs. plain scalar loop | distinguishing calls |
     |---|---|---|---|---|
     | `skip_ident` | 47.45 us | 25.83 us | 1.92x -> 1.03x | 950 of 22793 (4.2%) |
     | `skip_alpha` | 24.59 us | 25.64 us | 1.04x -> 1.10x | 950 of 32287 (2.9%) |
     | ~~`skip_hex_digits`~~ | 35.94 us | 36.56 us | ~~1.16x -> 1.20x~~ | **0** of 49391 |
     | ~~`skip_digits`~~ | 34.63 us | 35.95 us | ~~1.04x -> 1.07x~~ | **0** of 58891 |
     | ~~`skip_whitespace`~~ | 34.95 us | 35.25 us | ~~0.99x -> 1.00x~~ | **0** of 57941 |

     **The three struck rows are not evidence about probe width and are retained only so
     the record is not silently trimmed.** A probe width only changes behaviour for runs at
     least that long; on this corpus those classes never produce a run of 8 bytes, so
     probe 8 and probe 16 execute identical code on every one of their calls and the
     timing difference shown is unrelated variation. The counts come from
     `corpus-profile-realistic.json`, which the bench writes beside the results.

     The two surviving rows rest on few calls — 4.2% and 2.9% — and that is not a defect:
     the calls that reach a width are the ones that do the extra work, which is how 4.2%
     of calls move `skip_ident` by 46%. The narrowing was decided on `skip_ident`.

     The `long_run` group — one run covering a whole 64 KiB slice, the shape the dispatch
     threshold was tuned for — moved between -0.2% and +3.4% across its five scanners,
     i.e. within that spread. That group is unchanged and still reproduces by name.

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
   - Correction to the table in entry 1: three of its five rows are struck there, with the
     counts that retract them. This bullet deliberately does not restate which — an earlier
     revision did, said two of the struck rows "were genuinely exercised and stand", and
     contradicted the table it was correcting. The table is the record; read it. What is
     safe to add here is only what the table does not say: re-measured on the same corpus,
     `skip_ident` reads 1.96x -> 1.08x against the 1.92x -> 1.03x reported, and the
     narrowing was decided on that class.
   - The sweep bench builds a corpus per class and profiles it. The reporter **renders**
     those measurements and makes no pass/fail judgement about them: it prints each width's
     ratio, its per-round values, its gain against the shipped width and the spread of that
     pair, and how many calls could distinguish the widths at all. Choosing a width is left
     to whoever reads it.
   - It still refuses to publish a row it can prove is empty: a class whose corpus produces
     **zero** calls reaching a compared width cannot distinguish those widths, because both
     execute identical code on every call. That is a deduction from a count of zero, not a
     threshold.
   - The sweep reports **pairwise** results under one shared synthetic schedule. It does not
     rank widths and its rows must not be added up across classes: the schedule's shape was
     derived from two classes and reused for all fifteen, and it is decision-sensitive
     enough that an equal-weight version of it reversed the outcome on fourteen rows.

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
