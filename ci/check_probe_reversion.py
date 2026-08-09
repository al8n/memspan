#!/usr/bin/env python3
"""Fail when a monitored ASCII-class kernel leaves the band its shipped probe measures in.

Run from `.github/workflows/ci.yml`'s `probe-reversion` job on every pull
request; `--selftest` proves it bites without needing a runner.

# What this claims

One thing: **the scalar probe inside `skip_ascii_class!` is still narrow.**
`CLASS_PROBE` is a single compile-time constant per backend, shared by every
class that macro generates, so widening it back to a full chunk moves both
monitored classes at once and by far more than any spread these measurements
show. This job times the two classes at the shipped width and fails when either
leaves the band that separates the shipped probe from a chunk-width one.

# What this does not claim

It does **not** certify that the SIMD scanner is at or under the plain scalar
`position` loop it replaced, and no green tick from it should be read that way.
It cannot make that claim: at the shipped width SSE4.2 `skip_ident` measures
1.18 and AVX2 `skip_ident_start` measures 1.11, both above parity, and both are
the best configuration the sweep found. A bar at 1.00 would fail correct code,
so no band here is at 1.00 and none is positioned to answer a parity question.
Each backend's `CLASS_PROBE` doc comment says the same thing about the class it
could not fix.

It also **reports rather than blocks**. This repository has no branch
protection rule and no ruleset, so a red leg is visible on the pull request and
a maintainer can still merge or push directly past it. Making this required is
a repository setting, not something a workflow can assert about itself.

# The bands, and the separation each one sits in

Two sources, both `memspan` over `scalar` timed in the same process, both on an
AMD EPYC 7763: the probe sweep (run `31298019336`, two rounds per width, which
is where the chunk-width column comes from) and this job's own two rounds on
GitHub's hosted pool (CI run `31318907511`).

| class              | band | worst shipped seen | chunk sse42 | chunk avx2 |
|--------------------|------|--------------------|-------------|------------|
| `skip_ident`       | 1.25 | 1.186              | 1.412       | 1.591      |
| `skip_ident_start` | 1.18 | 1.122              | 1.256       | 1.242      |

`skip_ident`'s band clears the worst shipped observation by 5.4% and sits 13.0%
under the nearest chunk-width value. `skip_ident_start`'s clears 1.122 by 5.2%
and sits 5.3% under the nearest. Both are the geometric midpoint of the pair
they separate, rounded to two places, so neither side is favoured by accident.

Against that, the observed spread. Round to round inside one run, over all
fourteen class x width cells the sweep measured for these two classes, the
ratio moved by at most 1.81%, and by 0.41%, 1.75%, 0.24% and 0.46% on the four
shipped cells. Across the two independent runs the shipped cells reproduce to
0.68% (sse42 `skip_ident`), 0.31% (avx2 `skip_ident`), 1.08% (avx2
`skip_ident_start`) and 5.42% (sse42 `skip_ident_start`, which moves between
0.646 and 0.681 and so constrains nothing). Each band therefore stands off the
cell that constrains it by roughly five to eight times that cell's entire
observed spread.

# Why the bands are per class and not one number over both

A single bar has to clear the loosest cell, and 1.18 shipped on SSE4.2
`skip_ident` sets that floor. One bar above it cannot also catch AVX2
`skip_ident_start` reverting to 1.24, so with one number that cell is
unmonitored and a real 11% regression on it passes. Per-class bands cost
nothing -- neither is looser than the single 1.25 bar was -- and every
monitored cell now leaves its own band when the probe reverts.

The load-bearing detection is still `skip_ident`, on both tiers, with 13.0% and
27.3% of margin. `skip_ident_start`'s band is a second tripwire on the same
event with 5.3% of margin; if a future host makes it flake, tightening or
widening it does not remove the reversion check.

# Why the median of five rounds

The statistic has to be one that a single round cannot decide. An earlier
version took the minimum across two rounds, which passes a tree whose every
other round is over the band -- an intermittent regression ships green -- and
which biases every reported ratio downward by construction.

The median of an odd number of rounds requires a majority in either direction:
three of five rounds must be over the band to fail, and three must be under to
pass. That is the right shape for what is being detected. `CLASS_PROBE` is a
compile-time constant, so a reversion is present in every round of every run;
requiring a majority costs nothing against this failure mode and buys immunity
to one fast round and to one slow one alike. An even count is refused because
the median of an even sample is the mean of the two middle rounds, which one
slow round can drag over the band; fewer than three is refused because one
round is then a majority.

This job reports each round, the median and the spread, and judges only the
median. It does not judge the runner's noise: a wide spread is printed so a
reader can see it, and is not itself a failure.

# Why absence is an error

Every missing series, missing round and empty result set below is fatal. This
gate exists partly because a workflow leg whose every step was skipped by a
shared condition then exited green having measured nothing; a check that passes
when it finds no data would rebuild that defect one level out.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys

# Criterion flattens `/` in a group name into `_` on disk, so the directory for
# group `lexer_sweep/skip_ident` is `lexer_sweep_skip_ident`.
GROUP_PREFIX = "lexer_sweep_"

# One `--save-baseline` name per round, `shipped_r1`..`shipped_rN`. The workflow
# writes them with the same prefix; `--rounds` is the only thing that varies.
BASELINE_PREFIX = "shipped_r"

# The two series the ratio is built from. `benches/short_run.rs` also benches
# `probe_then`, which is deliberately not read here: it measures a different
# dispatch strategy, not the scanner this crate ships.
NUMERATOR = "memspan"
DENOMINATOR = "scalar"

# The shipped bands, and the only copy of them. A workflow that restated them
# could pass a looser number than this file documents and nothing would notice,
# so the workflow passes no bands at all and the override below is named for
# the only caller entitled to use it.
BANDS = "skip_ident=1.25,skip_ident_start=1.18"

# Five rounds, median. See the module docstring for why odd, and why not two.
DEFAULT_ROUNDS = 5


def die(message: str) -> None:
    print(f"\n**probe-reversion failed:** {message}\n")
    sys.exit(1)


def load(criterion_home: str, baseline: str) -> dict[str, float]:
    """Mean point estimates for every benchmark saved under `baseline`."""
    out: dict[str, float] = {}
    for dirpath, _dirnames, _filenames in os.walk(criterion_home):
        if os.path.basename(dirpath) != baseline:
            continue
        estimates = os.path.join(dirpath, "estimates.json")
        if not os.path.exists(estimates):
            continue
        rel = os.path.relpath(os.path.dirname(dirpath), criterion_home)
        try:
            with open(estimates) as handle:
                out[rel] = json.load(handle)["mean"]["point_estimate"]
        except (OSError, ValueError, KeyError) as err:
            die(f"cannot read `{estimates}`: {err}")
    return out


def series_time(
    measured: dict[str, float], name: str, series: str, baseline: str
) -> float:
    """The one timing for `<class>/<series>`, or an error naming what is absent."""
    group = f"{GROUP_PREFIX}{name}"
    hits = [
        value
        for key, value in measured.items()
        for parts in [key.split(os.sep)]
        if len(parts) >= 3 and parts[0] == group and parts[-2] == series
    ]
    if not hits:
        die(
            f"no `{series}` result for `{name}` under baseline `{baseline}`. The "
            "bench filter matched nothing, the build produced no bench, or the "
            "run died partway. This is an error and not an empty pass: a check "
            "that greens on absent data checks nothing."
        )
    if len(hits) > 1:
        die(
            f"{len(hits)} `{series}` results for `{name}` under baseline "
            f"`{baseline}`. The bench measures one input size per series, so a "
            "second one means the layout changed and this script no longer "
            "knows which cell it is reading."
        )
    return hits[0]


def ratios(
    criterion_home: str, classes: list[str], baselines: list[str]
) -> dict[str, list[float]]:
    """`{class: [memspan/scalar, one per round]}`, with every cell required."""
    out: dict[str, list[float]] = {name: [] for name in classes}
    for baseline in baselines:
        measured = load(criterion_home, baseline)
        if not measured:
            die(
                f"no criterion results at all under baseline `{baseline}` in "
                f"`{criterion_home}`. The measurement step did not run, did not "
                "save under this name, or wrote somewhere else."
            )
        for name in classes:
            numerator = series_time(measured, name, NUMERATOR, baseline)
            denominator = series_time(measured, name, DENOMINATOR, baseline)
            if denominator <= 0:
                die(
                    f"`{DENOMINATOR}` for `{name}` under `{baseline}` measured "
                    f"{denominator}, which cannot be a divisor."
                )
            out[name].append(numerator / denominator)
    return out


def parse_bands(spec: str) -> dict[str, float]:
    """`class=band,...` into a mapping, refusing anything it cannot read."""
    bands: dict[str, float] = {}
    for item in spec.split(","):
        if not item.strip():
            continue
        name, sep, raw = item.partition("=")
        if not sep or not name.strip():
            die(
                f"`--selftest-bands` entry `{item}` is not `class=ratio`. The "
                "bands are what this check enforces, so an entry it cannot read "
                "is a silently unmonitored class rather than a formatting nit."
            )
        try:
            band = float(raw)
        except ValueError:
            die(f"`--selftest-bands` entry `{item}` has a band that is not a number.")
        if band <= 0:
            die(f"`--selftest-bands` entry `{item}` has a band that is not positive.")
        bands[name.strip()] = band
    if not bands:
        die(
            "`--selftest-bands` is empty, so this check would hold nothing "
            "against anything."
        )
    return bands


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-home")
    parser.add_argument("--tier", help="label for the table; does not select data")
    parser.add_argument(
        "--rounds",
        type=int,
        default=DEFAULT_ROUNDS,
        help="how many `--save-baseline` rounds to require; odd and at least 3",
    )
    parser.add_argument(
        "--selftest-bands",
        default=BANDS,
        dest="bands",
        help="comma-separated `class=ratio` replacing the shipped bands. Named "
        "for its only legitimate caller: the workflow passes no bands, so "
        "loosening this check means editing this file",
    )
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    if not args.criterion_home or not args.tier:
        parser.error("--criterion-home and --tier are required")

    bands = parse_bands(args.bands)
    if args.rounds < 3 or args.rounds % 2 == 0:
        die(
            f"`--rounds {args.rounds}` must be odd and at least 3. The statistic "
            "is the median: with an even count it becomes the mean of the two "
            "middle rounds, which a single slow round can drag over a band, and "
            "with one round there is no majority to require."
        )
    baselines = [f"{BASELINE_PREFIX}{n}" for n in range(1, args.rounds + 1)]

    classes = sorted(bands)
    measured = ratios(args.criterion_home, classes, baselines)

    print(f"### `{args.tier}` shipped-probe reversion check\n")
    print(
        "`memspan` over the plain scalar `position` loop, both timed in the "
        f"same process. Each class must keep the **median of {args.rounds} "
        "rounds** inside the band that separates its shipped probe width from a "
        "chunk-width one. This is a reversion check and not a parity check: two "
        "of the four monitored cells sit legitimately above 1.00, and this job "
        "makes no claim about them. It reports a breach; it does not block a "
        "merge.\n"
    )
    header = " | ".join(f"`r{n}`" for n in range(1, args.rounds + 1))
    print(f"| class | {header} | median | spread | band | |")
    print("|" + "---|" * (args.rounds + 5))

    over: list[tuple[str, float, float]] = []
    for name in classes:
        rounds = measured[name]
        median = statistics.median(rounds)
        spread = max(rounds) / min(rounds) - 1
        breached = median > bands[name]
        if breached:
            over.append((name, median, bands[name]))
        print(
            f"| `{name}` | "
            + " | ".join(f"{value:.3f}" for value in rounds)
            + f" | **{median:.3f}** | {spread * 100:.1f}% | {bands[name]:.2f} | "
            + ("**over**" if breached else "ok")
            + " |"
        )

    if over:
        die(
            f"on `{args.tier}`, "
            + ", ".join(
                f"`{name}` has a median of {median:.3f} against a band of "
                f"{band:.2f}"
                for name, median, band in over
            )
            + ".\n\nThat is a majority of rounds outside the band the shipped "
            "probe width was measured in, which is what a widened `CLASS_PROBE` "
            "looks like: the probe scans a compile-time-constant length, LLVM "
            "unrolls it into one short-circuit branch tree per byte, and a "
            "caller whose run lengths vary mispredicts in a different copy on "
            "every call. See that constant's doc comment in the backend, and "
            "`.github/workflows/probe-sweep.yml` for the measured widths.\n\n"
            "It is not a statement about scalar parity. Two monitored cells are "
            "above 1.00 at the shipped width and always were; the bands are "
            "positioned between the shipped and chunk-width measurements, not "
            "at 1.00."
        )

    print(
        f"\nEvery monitored class on `{args.tier}` kept its median inside its "
        "band, so the probe width has not reverted. This says nothing about "
        "whether these kernels beat the scalar loop; see the bands above."
    )
    return 0


# Every ratio below was measured, and each tuple holds every recorded
# observation of that one cell in the order it was taken. The first two come
# from probe-sweep run `31298019336` (AMD EPYC 7763, two rounds per width); for
# the shipped width the next two come from this job's own rounds on GitHub's
# hosted pool in CI run `31318907511`, same CPU model. The selftest cycles a
# cell's observations to fill however many rounds a case asks for, so the
# statistic is exercised on the dispersion these cells really have; the cycling
# is a construction of this selftest and not a claim that five rounds were run.
SHIPPED_SSE42 = {
    "skip_ident": (1.186, 1.181, 1.178, 1.183),
    "skip_ident_start": (0.670, 0.681, 0.658, 0.646),
}
CHUNK_SSE42 = {"skip_ident": (1.412, 1.417), "skip_ident_start": (1.279, 1.256)}
SHIPPED_AVX2 = {
    "skip_ident": (0.961, 0.959, 0.960, 0.958),
    "skip_ident_start": (1.116, 1.110, 1.113, 1.122),
}
CHUNK_AVX2 = {"skip_ident": (1.591, 1.591), "skip_ident_start": (1.245, 1.242)}


def selftest() -> int:
    """Build synthetic criterion trees and require each verdict."""
    import itertools
    import shutil
    import subprocess
    import tempfile

    roots: list[str] = []

    def tree(
        per_class: dict[str, tuple[float, ...]],
        rounds: int = DEFAULT_ROUNDS,
        drop: tuple[int, str, str] | None = None,
    ) -> str:
        """Plant `{class: recorded ratios}`, cycled to `rounds`; `drop` cuts a cell."""
        root = tempfile.mkdtemp()
        roots.append(root)
        for name, recorded in per_class.items():
            cycled = list(itertools.islice(itertools.cycle(recorded), rounds))
            for index, ratio in enumerate(cycled, start=1):
                for series, ns in (
                    (DENOMINATOR, 1000.0),
                    (NUMERATOR, 1000.0 * ratio),
                ):
                    if drop == (index, name, series):
                        continue
                    path = os.path.join(
                        root,
                        f"{GROUP_PREFIX}{name}",
                        series,
                        "65536",
                        f"{BASELINE_PREFIX}{index}",
                    )
                    os.makedirs(path, exist_ok=True)
                    with open(os.path.join(path, "estimates.json"), "w") as handle:
                        json.dump({"mean": {"point_estimate": ns}}, handle)
        return root

    def run(root: str, extra: list[str] | None = None) -> tuple[bool, str]:
        proc = subprocess.run(
            [
                sys.executable,
                os.path.abspath(__file__),
                "--criterion-home",
                root,
                "--tier",
                "selftest",
            ]
            + (extra or []),
            capture_output=True,
            text=True,
        )
        return proc.returncode == 0, proc.stdout + proc.stderr

    ident, start = "skip_ident", "skip_ident_start"
    shipped_start_avx2 = SHIPPED_AVX2[start]

    cases: list[tuple[str, str, list[str] | None, bool, str]] = [
        (
            "the recorded shipped SSE4.2 ratios pass, 1.18 included",
            tree(SHIPPED_SSE42),
            None,
            True,
            "",
        ),
        (
            "the recorded shipped AVX2 ratios pass, 1.11 included",
            tree(SHIPPED_AVX2),
            None,
            True,
            "",
        ),
        (
            "a chunk-width SSE4.2 probe fails",
            tree(CHUNK_SSE42),
            None,
            False,
            "`skip_ident` has a median of 1.412",
        ),
        (
            "a chunk-width AVX2 probe fails",
            tree(CHUNK_AVX2),
            None,
            False,
            "`skip_ident` has a median of 1.591",
        ),
        (
            "AVX2 `skip_ident_start` alone at chunk width fails, which one 1.25 "
            "bar over both classes could not catch",
            tree({ident: SHIPPED_AVX2[ident], start: CHUNK_AVX2[start]}),
            None,
            False,
            "`skip_ident_start` has a median of 1.245",
        ),
        (
            "the bands are per class: 1.20 passes `skip_ident` and fails "
            "`skip_ident_start` in the same tree",
            tree({ident: (1.20,), start: (1.20,)}),
            None,
            False,
            "`skip_ident_start` has a median of 1.200",
        ),
        (
            "one fast round no longer clears a slower majority",
            tree({ident: (1.41, 1.41, 1.18, 1.41, 1.41), start: shipped_start_avx2}),
            None,
            False,
            "`skip_ident` has a median of 1.410",
        ),
        (
            "one slow round does not fail a clean tree",
            tree(
                {
                    ident: (1.186, 1.181, 1.44, 1.178, 1.183),
                    start: shipped_start_avx2,
                }
            ),
            None,
            True,
            "",
        ),
        (
            "two slow rounds out of five do not either",
            tree(
                {ident: (1.186, 1.181, 1.44, 1.44, 1.183), start: shipped_start_avx2}
            ),
            None,
            True,
            "",
        ),
        (
            "three slow rounds out of five do",
            tree(
                {ident: (1.186, 1.44, 1.44, 1.44, 1.183), start: shipped_start_avx2}
            ),
            None,
            False,
            "`skip_ident` has a median of 1.440",
        ),
        (
            "exactly at the band passes; the comparison is strict",
            tree({ident: (1.25,), start: (1.18,)}),
            None,
            True,
            "",
        ),
        (
            "one thousandth over the band fails",
            tree({ident: (1.251,), start: (1.18,)}),
            None,
            False,
            "`skip_ident` has a median of 1.251",
        ),
        (
            "a band of 1.00 would reject the shipped tree, so these bands are "
            "load-bearing and are not a parity claim",
            tree(SHIPPED_SSE42),
            ["--selftest-bands", f"{ident}=1.00,{start}=1.00"],
            False,
            "`skip_ident` has a median of 1.18",
        ),
        (
            "an even round count is refused",
            tree(SHIPPED_SSE42, rounds=4),
            ["--rounds", "4"],
            False,
            "must be odd and at least 3",
        ),
        (
            "a single round is refused",
            tree(SHIPPED_SSE42, rounds=1),
            ["--rounds", "1"],
            False,
            "must be odd and at least 3",
        ),
        (
            "a missing `scalar` series is an error, not a pass",
            tree(SHIPPED_SSE42, drop=(3, ident, DENOMINATOR)),
            None,
            False,
            "no `scalar` result for `skip_ident`",
        ),
        (
            "a missing `memspan` series is an error, not a pass",
            tree(SHIPPED_SSE42, drop=(1, start, NUMERATOR)),
            None,
            False,
            "no `memspan` result for `skip_ident_start`",
        ),
        (
            "a round that never ran is an error, not a pass",
            tree(SHIPPED_SSE42, rounds=3),
            None,
            False,
            f"no criterion results at all under baseline `{BASELINE_PREFIX}4`",
        ),
        (
            "an empty criterion home is an error, not a pass",
            tree({}),
            None,
            False,
            "no criterion results at all",
        ),
        (
            "a class the bench never produced is an error, not a pass",
            tree(SHIPPED_SSE42),
            ["--selftest-bands", f"{ident}=1.25,skip_never_benched=1.25"],
            False,
            "no `memspan` result for `skip_never_benched`",
        ),
        (
            "a `--selftest-bands` entry that is not `class=ratio` is refused",
            tree(SHIPPED_SSE42),
            ["--selftest-bands", "skip_ident"],
            False,
            "is not `class=ratio`",
        ),
        (
            "an empty `--selftest-bands` is refused",
            tree(SHIPPED_SSE42),
            ["--selftest-bands", ","],
            False,
            "`--selftest-bands` is empty",
        ),
    ]

    failures: list[str] = []
    try:
        for label, root, extra, expect_ok, needle in cases:
            ok, output = run(root, extra)
            if ok != expect_ok:
                failures.append(
                    f"{label}: expected {'pass' if expect_ok else 'failure'}, got "
                    f"{'pass' if ok else 'failure'}\n{indent(output)}"
                )
            elif needle and needle not in output:
                failures.append(
                    f"{label}: failed for the wrong reason -- {needle!r} not in "
                    f"the message\n{indent(output)}"
                )
            else:
                print(f"  {'passes' if expect_ok else 'caught'}  {label}")
    finally:
        for root in roots:
            shutil.rmtree(root, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} selftest case(s) wrong:\n")
        for failure in failures:
            print(f"* {failure}")
        return 1

    print(f"\nall {len(cases)} selftest cases behave as claimed")
    return 0


def indent(text: str) -> str:
    return "\n".join(f"      {line}" for line in text.strip().splitlines())


if __name__ == "__main__":
    sys.exit(main())
