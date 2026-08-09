#!/usr/bin/env python3
"""Fail when an ASCII-class kernel loses to the scalar loop it replaces.

Run from `.github/workflows/ci.yml`'s `class-perf-bar` job on every pull
request; `--selftest` proves it bites without needing a runner.

# What this gates, and what it does not

`ci/probe_sweep.py` renders a *sweep* -- several probe widths, no before, no
after -- and deliberately passes no judgement, because a sweep has no baseline
to judge against. This script asks a different and much smaller question about
the **one width the crate actually ships**:

> does `skip_ident` or `skip_ident_start` take longer than the plain scalar
> `position` loop that the SIMD scanner replaced, by more than the bar?

That question has an absolute answer and needs no history. Both series are
measured in the same process on the same corpus, so the ratio is immune to the
between-run drift that makes a raw timing gate flake.

# Why 1.25 and not 1.00

Because 1.00 would fail correct code. After the 8-byte narrowing, SSE4.2
`skip_ident` measures 1.18 -- the sweep found no width that puts that one cell
under parity, and the change was merged saying so. A bar of 1.00 would reject
the best measured configuration that exists.

1.25 is the smallest round number above that 1.18 with usable headroom, and it
sits below the widths this gate exists to keep out. Against the pre-narrowing
measurements (run `31298019336`, AMD EPYC 7763):

| cell                       | shipped | before the fix | bar 1.25 |
|----------------------------|---------|----------------|----------|
| avx2 `skip_ident`          | 0.96    | 1.59           | catches  |
| avx2 `skip_ident_start`    | 1.11    | 1.24           | *misses* |
| sse42 `skip_ident`         | 1.18    | 1.41           | catches  |
| sse42 `skip_ident_start`   | 0.68    | 1.27           | catches  |

Three of the four regressions clear the bar, and at least one on each tier, so
restoring a chunk-width probe on either fails this job. The fourth is named
rather than hidden: AVX2 `skip_ident_start` was 1.24 before the fix, and no bar
that passes SSE4.2's legitimate 1.18 can also catch a 1.24. It is caught on
that tier by `skip_ident` instead, which is why the job fails on the *worst*
monitored cell rather than requiring every cell to be the detecting one.

# Why the minimum across rounds

Noise makes a benchmark slower, never faster, so the smallest ratio observed is
the closest estimate of the code's real cost and the least likely to fail a
correct tree. It is one-sided in the safe direction: it can only make a real
regression harder to catch, never make a clean one fail. The regressions above
clear the bar by 13% to 27%, against a measured round-to-round spread of 0-2%
on these cells, so the margin absorbed is nowhere near the margin needed.

# Why absence is an error

Every missing series, missing round and empty result set below is fatal. This
gate exists partly because a workflow leg whose every step was skipped by a
shared condition then exited green having measured nothing; a perf bar that
passes when it finds no data would rebuild that defect one level out.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

# Criterion flattens `/` in a group name into `_` on disk, so the directory for
# group `lexer_sweep/skip_ident` is `lexer_sweep_skip_ident`.
GROUP_PREFIX = "lexer_sweep_"

# The two series the ratio is built from. `benches/short_run.rs` also benches
# `probe_then`, which is deliberately not read here: it measures a different
# dispatch strategy, not the scanner this crate ships.
NUMERATOR = "memspan"
DENOMINATOR = "scalar"


def die(message: str) -> None:
    print(f"\n**class-perf-bar failed:** {message}\n")
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
            "run died partway. This is an error and not an empty pass: a bar "
            "that greens on absent data is not a bar."
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
) -> dict[str, dict[str, float]]:
    """`{class: {baseline: memspan/scalar}}`, with every cell required."""
    out: dict[str, dict[str, float]] = {name: {} for name in classes}
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
            out[name][baseline] = numerator / denominator
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-home")
    parser.add_argument("--tier", help="label for the table; does not select data")
    parser.add_argument(
        "--baselines",
        default="bar_r1,bar_r2",
        help="comma-separated `--save-baseline` names, one per round",
    )
    parser.add_argument(
        "--classes",
        default="skip_ident,skip_ident_start",
        help="comma-separated class names to hold against the bar",
    )
    parser.add_argument(
        "--max-ratio",
        type=float,
        default=1.25,
        help="fail when a class's best round exceeds this multiple of scalar",
    )
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    if not args.criterion_home or not args.tier:
        parser.error("--criterion-home and --tier are required")

    classes = [c for c in args.classes.split(",") if c]
    baselines = [b for b in args.baselines.split(",") if b]
    if not classes:
        die("`--classes` is empty, so the bar would hold nothing against anything.")
    if not baselines:
        die("`--baselines` is empty, so there is nothing to read.")

    measured = ratios(args.criterion_home, classes, baselines)

    print(f"### `{args.tier}` shipped-width perf bar\n")
    print(
        "Scanner time over the plain scalar `position` loop measured in the "
        f"same run. The bar is **{args.max_ratio:.2f}**, applied to the best "
        "round: above it, the SIMD path costs more than the code it replaced by "
        "more than this crate accepts.\n"
    )
    print("| class | " + " | ".join(f"`{b}`" for b in baselines) + " | best | bar |")
    print("|" + "---|" * (len(baselines) + 3))

    over: list[str] = []
    for name in classes:
        best = min(measured[name].values())
        if best > args.max_ratio:
            over.append(name)
        print(
            f"| `{name}` | "
            + " | ".join(f"{measured[name][b]:.3f}" for b in baselines)
            + f" | **{best:.3f}** | {args.max_ratio:.2f}"
            + (" **over**" if best > args.max_ratio else " ok")
            + " |"
        )

    if over:
        die(
            f"on `{args.tier}`, "
            + ", ".join(
                f"`{name}` at {min(measured[name].values()):.3f}" for name in over
            )
            + f" exceeds the {args.max_ratio:.2f} bar.\n\n"
            "These kernels are slower than the plain scalar loop they replace by "
            "more than this crate accepts. The usual cause is a widened "
            "`CLASS_PROBE`: the probe scans a compile-time-constant length, LLVM "
            "unrolls it into one short-circuit branch tree per byte, and a caller "
            "whose run lengths vary mispredicts in a different copy on every "
            "call. See that constant's doc comment in the backend, and "
            "`.github/workflows/probe-sweep.yml` for the measured widths."
        )

    print(
        f"\nEvery monitored class on `{args.tier}` is at or under the "
        f"{args.max_ratio:.2f} bar."
    )
    return 0


# Recorded ratios from run `31298019336` on an AMD EPYC 7763, the measurements
# this bar was calibrated against. Used by the selftest so the cases are the
# real before/after and not invented numbers.
SHIPPED_SSE42 = {"skip_ident": (1.18, 1.19), "skip_ident_start": (0.68, 0.69)}
REVERTED_SSE42 = {"skip_ident": (1.41, 1.42), "skip_ident_start": (1.27, 1.28)}
SHIPPED_AVX2 = {"skip_ident": (0.96, 0.97), "skip_ident_start": (1.11, 1.12)}
REVERTED_AVX2 = {"skip_ident": (1.59, 1.60), "skip_ident_start": (1.24, 1.25)}


def selftest() -> int:
    """Build synthetic criterion trees and require each verdict."""
    import shutil
    import subprocess
    import tempfile

    roots: list[str] = []

    def tree(
        per_class: dict[str, tuple[float, ...]],
        baselines: tuple[str, ...] = ("bar_r1", "bar_r2"),
        drop: tuple[str, str, str] | None = None,
    ) -> str:
        """Plant `{class: (ratio per baseline, ...)}`; `drop` removes one cell."""
        root = tempfile.mkdtemp()
        roots.append(root)
        for name, values in per_class.items():
            for baseline, ratio in zip(baselines, values):
                for series, ns in (
                    (DENOMINATOR, 1000.0),
                    (NUMERATOR, 1000.0 * ratio),
                ):
                    if drop == (baseline, name, series):
                        continue
                    path = os.path.join(
                        root, f"{GROUP_PREFIX}{name}", series, "65536", baseline
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

    cases: list[tuple[str, str, list[str] | None, bool, str]] = [
        (
            "the shipped SSE4.2 measurements pass, 1.18 included",
            tree(SHIPPED_SSE42),
            None,
            True,
            "",
        ),
        (
            "the shipped AVX2 measurements pass",
            tree(SHIPPED_AVX2),
            None,
            True,
            "",
        ),
        (
            "a reverted SSE4.2 probe fails",
            tree(REVERTED_SSE42),
            None,
            False,
            "`skip_ident` at 1.410",
        ),
        (
            "a reverted AVX2 probe fails",
            tree(REVERTED_AVX2),
            None,
            False,
            "`skip_ident` at 1.590",
        ),
        (
            "a bar of 1.00 would reject the shipped tree, so 1.25 is load-bearing",
            tree(SHIPPED_SSE42),
            ["--max-ratio", "1.00"],
            False,
            "`skip_ident` at 1.180",
        ),
        (
            "exactly at the bar passes; the comparison is strict",
            tree({ident: (1.25, 1.25), start: (1.25, 1.25)}),
            None,
            True,
            "",
        ),
        (
            "one thousandth over the bar fails",
            tree({ident: (1.251, 1.251), start: (0.7, 0.7)}),
            None,
            False,
            "`skip_ident` at 1.251",
        ),
        (
            "one noisy round does not fail a clean tree",
            tree({ident: (1.18, 1.44), start: (0.68, 0.71)}),
            None,
            True,
            "",
        ),
        (
            "a regression in every round is not excused by taking the minimum",
            tree({ident: (1.41, 1.44), start: (0.68, 0.71)}),
            None,
            False,
            "`skip_ident` at 1.410",
        ),
        (
            "a missing `scalar` series is an error, not a pass",
            tree(SHIPPED_SSE42, drop=("bar_r2", ident, DENOMINATOR)),
            None,
            False,
            "no `scalar` result for `skip_ident`",
        ),
        (
            "a missing `memspan` series is an error, not a pass",
            tree(SHIPPED_SSE42, drop=("bar_r1", start, NUMERATOR)),
            None,
            False,
            "no `memspan` result for `skip_ident_start`",
        ),
        (
            "a round that never ran is an error, not a pass",
            tree(SHIPPED_SSE42, baselines=("bar_r1",)),
            None,
            False,
            "no criterion results at all under baseline `bar_r2`",
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
            ["--classes", "skip_ident,skip_never_benched"],
            False,
            "no `memspan` result for `skip_never_benched`",
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
