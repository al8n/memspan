#!/usr/bin/env python3
"""Report how the shipped ASCII-class probe times against the plain scalar loop.

Run from `.github/workflows/ci.yml`'s `probe-timing` job on every pull request;
`--selftest` proves its checks bite without needing a runner.

# This reports. It does not decide.

**Whether the `CLASS_PROBE` narrowing has been reverted is decided by
`ci/check_probe_width.py`, from the source, and by nothing here.** That is a
compile-time constant; whether it is still 8 is a property a reader -- or a
parser -- can settle exactly, without a CPU. This job exists for the question
source cannot answer: what the shipped width actually costs, on hardware.

So a ratio outside its reference line below is printed, annotated as a warning,
and **does not fail this job**. Earlier versions failed on it, and the reason
they no longer do is not tolerance: it is that the verdict was never sound.
The bands were calibrated on an AMD EPYC 7763, `runs-on: ubuntu-latest` does not
pin a CPU model, and a systematic shift between models moves all five rounds
together -- which a median cannot absorb. That produced two ways to be wrong at
once: a clean tree failing on a slower host, and a reverted tree passing on a
faster one. A gate whose verdict depends on which machine GitHub allocated is
not a gate, and the property it was proxying is now checked where it is exact.

# What still fails, and why absence is not tolerance

Every missing series, missing round and empty result set is fatal. A report that
prints nothing and exits 0 is the defect this repository already shipped once,
in a workflow leg whose every step was skipped by a shared condition and which
then reported absence as success. Report-only means no verdict on a *timing*; it
does not mean a run that measured nothing is acceptable.

The host is named rather than assumed. `--host` is required, the job passes the
runner's own CPU model, and the reference observations below came from an AMD
EPYC 7763 -- so a reader comparing a row against a reference line can see
whether the two came from the same kind of machine.

# The reference lines, and how they are derived

Both are the **geometric midpoint** of the pair they separate -- the worst
shipped ratio ever recorded for that class, and the nearest chunk-width one --
rounded to two places. Nothing below is a restated number: `reference_lines()`
computes them from the recorded observations at the foot of this file, and a
selftest case recomputes every cell of both tables from the same data, so the
prose and the code cannot drift apart.

| class | reference | worst shipped | nearest chunk | over shipped | under chunk |
|---|---|---|---|---|---|
| `skip_ident` | 1.29 | 1.186 | 1.412 | 8.8% | 8.6% |
| `skip_ident_start` | 1.18 | 1.122 | 1.242 | 5.2% | 5.0% |

An earlier revision of this file claimed the same neutral rule and did not
follow it: it documented both lines as geometric midpoints while shipping 1.25
for `skip_ident`, where the midpoint of 1.186 and 1.412 is 1.294. The stated
rule could not rederive the number, and the line sat 5.4% above shipped against
13.0% below chunk -- much closer to the code being watched than to the reversion
being watched for. Deriving the lines is what makes the rule true; the selftest
is what keeps it true.

Against those margins, the spread each constraining cell actually shows.
Max over min across every recorded observation of that class at the shipped
width, per tier:

| class | `sse42` spread | `avx2` spread |
|---|---|---|
| `skip_ident` | 0.7% | 0.3% |
| `skip_ident_start` | 5.4% | 1.1% |

`skip_ident_start` on SSE4.2 moves 5.4% between runs and constrains nothing --
it sits near 0.67, far under either line. The cells that do constrain the lines
reproduce to a percent or better.

# What the numbers are not

They are **not** a scalar-parity claim. At the shipped width SSE4.2 `skip_ident`
measures 1.18 and AVX2 `skip_ident_start` 1.11, both above parity, and both are
the best configuration the probe sweep found. No line here sits at 1.00, and
none is positioned to answer a parity question; each backend's `CLASS_PROBE` doc
comment says the same about the class its narrowing could not fix.

# Why the median of five rounds

The reported statistic should not be movable by one round. The median of an odd
count needs a majority to shift: three of five rounds have to agree. An even
count is refused because the median of an even sample is the mean of the two
middle rounds, which one slow round drags; fewer than three is refused because
one round is then a majority. Each round is printed alongside the median and the
spread, so a reader can see the dispersion rather than take the summary's word
for it.
"""

from __future__ import annotations

import argparse
import json
import math
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

# Five rounds, median. See the module docstring for why odd, and why not two.
DEFAULT_ROUNDS = 5

# Every ratio below was measured, and each tuple holds every recorded
# observation of that one cell in the order it was taken. The first two of each
# shipped tuple come from probe-sweep run `31298019336` (AMD EPYC 7763, two
# rounds per width); the next two come from this job's own rounds on GitHub's
# hosted pool in CI run `31318907511`, same CPU model. The chunk-width tuples are
# the sweep's two rounds at that width.
#
# These are the reference lines' only inputs. `reference_lines()` derives the
# lines from them, and the selftest derives both docstring tables from them, so
# adding an observation moves the documentation with the data.
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

SHIPPED_BY_TIER = {"sse42": SHIPPED_SSE42, "avx2": SHIPPED_AVX2}
CHUNK_BY_TIER = {"sse42": CHUNK_SSE42, "avx2": CHUNK_AVX2}


def die(message: str) -> None:
    print(f"\n**probe-timing report failed:** {message}\n")
    sys.exit(1)


def monitored() -> list[str]:
    """The classes every recorded table covers, refusing a partial record."""
    tables = (SHIPPED_SSE42, SHIPPED_AVX2, CHUNK_SSE42, CHUNK_AVX2)
    classes = set(tables[0])
    for table in tables[1:]:
        if set(table) != classes:
            die(
                "the recorded observations do not cover the same classes in "
                "every table, so a reference line would be derived from a "
                "different set of measurements than it is compared against."
            )
    return sorted(classes)


def worst_shipped(name: str) -> float:
    """The slowest ratio ever recorded for `name` at the shipped width."""
    return max(v for table in SHIPPED_BY_TIER.values() for v in table[name])


def nearest_chunk(name: str) -> float:
    """The fastest ratio ever recorded for `name` at a chunk width."""
    return min(v for table in CHUNK_BY_TIER.values() for v in table[name])


def reference_lines() -> dict[str, float]:
    """Geometric midpoint of (worst shipped, nearest chunk), to two places.

    Derived rather than restated. A midpoint in log space puts the same
    proportional distance on each side, so neither the code being watched nor
    the reversion being watched for is favoured; rounding to two places is
    applied here so the printed line and the compared line are one number.
    """
    return {
        name: round(math.sqrt(worst_shipped(name) * nearest_chunk(name)), 2)
        for name in monitored()
    }


def spread(values: tuple[float, ...] | list[float]) -> float:
    """Max over min, minus one: the fraction the observations move across."""
    return max(values) / min(values) - 1


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
            "run died partway. This is an error and not an empty pass: a report "
            "that greens on absent data reports nothing."
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


def parse_reference(spec: str) -> dict[str, float]:
    """`class=ratio,...` into a mapping, refusing anything it cannot read."""
    lines: dict[str, float] = {}
    for item in spec.split(","):
        if not item.strip():
            continue
        name, sep, raw = item.partition("=")
        if not sep or not name.strip():
            die(
                f"`--selftest-reference` entry `{item}` is not `class=ratio`. An "
                "entry this script cannot read is a silently unreported class "
                "rather than a formatting nit."
            )
        try:
            line = float(raw)
        except ValueError:
            die(
                f"`--selftest-reference` entry `{item}` has a line that is not a "
                "number."
            )
        if line <= 0:
            die(
                f"`--selftest-reference` entry `{item}` has a line that is not "
                "positive."
            )
        lines[name.strip()] = line
    if not lines:
        die(
            "`--selftest-reference` is empty, so this report would hold nothing "
            "against anything."
        )
    return lines


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-home")
    parser.add_argument("--tier", help="label for the table; does not select data")
    parser.add_argument(
        "--host",
        help="the CPU model that produced these rounds. Required: a ratio is "
        "only comparable to the recorded ones if the reader can see what ran it",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=DEFAULT_ROUNDS,
        help="how many `--save-baseline` rounds to require; odd and at least 3",
    )
    parser.add_argument(
        "--selftest-reference",
        dest="reference",
        help="comma-separated `class=ratio` replacing the derived reference "
        "lines. Named for its only legitimate caller: the workflow passes none, "
        "and the shipped lines are computed from the recorded observations",
    )
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    if not args.criterion_home or not args.tier or not args.host:
        parser.error("--criterion-home, --tier and --host are required")

    lines = parse_reference(args.reference) if args.reference else reference_lines()
    if args.rounds < 3 or args.rounds % 2 == 0:
        die(
            f"`--rounds {args.rounds}` must be odd and at least 3. The statistic "
            "is the median: with an even count it becomes the mean of the two "
            "middle rounds, which a single slow round drags, and with one round "
            "there is no majority behind it."
        )
    baselines = [f"{BASELINE_PREFIX}{n}" for n in range(1, args.rounds + 1)]

    classes = sorted(lines)
    measured = ratios(args.criterion_home, classes, baselines)

    print(f"### `{args.tier}` shipped-probe timing, on `{args.host}`\n")
    print(
        "`memspan` over the plain scalar `position` loop, both timed in the same "
        f"process, **median of {args.rounds} rounds**. The reference line is the "
        "geometric midpoint between the worst ratio recorded at the shipped "
        "width and the nearest one recorded at a chunk width, both on an AMD "
        "EPYC 7763.\n"
    )
    print(
        "**This is evidence, not a verdict.** Whether the probe is still narrow "
        "is decided from the source by `ci/check_probe_width.py`; a row over its "
        "reference line here is annotated and does not fail this job. Two of the "
        "four monitored cells sit legitimately above 1.00, so no line is at "
        "parity and none answers a parity question.\n"
    )
    header = " | ".join(f"`r{n}`" for n in range(1, args.rounds + 1))
    print(f"| class | {header} | median | spread | reference | |")
    print("|" + "---|" * (args.rounds + 5))

    over: list[tuple[str, float, float]] = []
    for name in classes:
        rounds = measured[name]
        median = statistics.median(rounds)
        breached = median > lines[name]
        if breached:
            over.append((name, median, lines[name]))
        print(
            f"| `{name}` | "
            + " | ".join(f"{value:.3f}" for value in rounds)
            + f" | **{median:.3f}** | {spread(rounds) * 100:.1f}% | "
            + f"{lines[name]:.2f} | "
            + ("**over**" if breached else "ok")
            + " |"
        )

    if over:
        detail = ", ".join(
            f"`{name}` has a median of {median:.3f} against a reference line of "
            f"{line:.2f}"
            for name, median, line in over
        )
        print(f"\n::warning::on `{args.tier}` ({args.host}), {detail}")
        print(
            f"\n**Over the reference line: {detail}.**\n\n"
            "This is worth reading and is not a verdict. If `probe-width` is "
            "green then `CLASS_PROBE` is unchanged and this is not a reverted "
            "probe -- the remaining explanations are a different host from the "
            "AMD EPYC 7763 these lines were placed on, a change elsewhere in the "
            "kernel, or a change in the bench corpus. `runs-on: ubuntu-latest` "
            "selects a generic hosted x64 VM and pins no CPU model, and a model "
            "shift moves every round together, so the median does not absorb "
            "it.\n\n"
            "It is also not a statement about scalar parity. Two monitored cells "
            "are above 1.00 at the shipped width and always were."
        )
        return 0

    print(
        f"\nEvery monitored class on `{args.tier}` kept its median inside its "
        "reference line. That is consistent with an unreverted probe; it is not "
        "what establishes it, and it says nothing about whether these kernels "
        "beat the scalar loop."
    )
    return 0


# ── selftest ─────────────────────────────────────────────────────────────────


def doc_rows(header: str) -> list[list[str]]:
    """The body rows of the docstring table under `header`, cells stripped."""
    wanted = " ".join(header.split())
    lines = (__doc__ or "").splitlines()
    for index, line in enumerate(lines):
        if " ".join(line.split()) != wanted:
            continue
        rows: list[list[str]] = []
        for follow in lines[index + 2 :]:
            if not follow.strip().startswith("|"):
                break
            rows.append(
                [cell.strip().strip("`") for cell in follow.strip().strip("|").split("|")]
            )
        return rows
    return []


REFERENCE_HEADER = (
    "| class | reference | worst shipped | nearest chunk | over shipped | under chunk |"
)
SPREAD_HEADER = "| class | `sse42` spread | `avx2` spread |"


def check_documented_tables() -> list[str]:
    """Rederive every cell of both docstring tables from the recorded data."""
    wrong: list[str] = []
    lines = reference_lines()

    rows = doc_rows(REFERENCE_HEADER)
    if not rows:
        wrong.append(
            "the reference-line table is missing from the module docstring, so "
            "the derivation this file documents is undocumented"
        )
    elif [row[0] for row in rows] != monitored():
        wrong.append(
            f"the reference-line table covers {[row[0] for row in rows]}, but the "
            f"recorded observations cover {monitored()}"
        )
    else:
        for row in rows:
            name = row[0]
            low, high = worst_shipped(name), nearest_chunk(name)
            derived = [
                f"{lines[name]:.2f}",
                f"{low:.3f}",
                f"{high:.3f}",
                f"{(lines[name] / low - 1) * 100:.1f}%",
                f"{(1 - lines[name] / high) * 100:.1f}%",
            ]
            if row[1:] != derived:
                wrong.append(
                    f"the documented reference row for `{name}` says {row[1:]}, "
                    f"but the recorded observations derive {derived}"
                )

    rows = doc_rows(SPREAD_HEADER)
    if not rows:
        wrong.append("the recorded-spread table is missing from the module docstring")
    elif [row[0] for row in rows] != monitored():
        wrong.append(
            f"the recorded-spread table covers {[row[0] for row in rows]}, but "
            f"the recorded observations cover {monitored()}"
        )
    else:
        for row in rows:
            name = row[0]
            derived = [
                f"{spread(SHIPPED_BY_TIER[tier][name]) * 100:.1f}%"
                for tier in ("sse42", "avx2")
            ]
            if row[1:] != derived:
                wrong.append(
                    f"the documented spread row for `{name}` says {row[1:]}, but "
                    f"the recorded observations derive {derived}"
                )

    # The rule the prose states, applied to the numbers the prose prints. A line
    # that fell outside the pair it separates would still round-trip through the
    # table above; this is the claim, not the arithmetic.
    for name, line in lines.items():
        low, high = worst_shipped(name), nearest_chunk(name)
        if not low < line < high:
            wrong.append(
                f"the reference line for `{name}` is {line:.2f}, which is not "
                f"between the worst shipped ratio {low:.3f} and the nearest "
                f"chunk-width ratio {high:.3f}. It separates nothing."
            )
    return wrong


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
                "--host",
                "selftest-host",
            ]
            + (extra or []),
            capture_output=True,
            text=True,
        )
        return proc.returncode == 0, proc.stdout + proc.stderr

    ident, start = "skip_ident", "skip_ident_start"
    shipped_start_avx2 = SHIPPED_AVX2[start]

    # (label, tree, extra args, expect rc 0, needle the output must contain)
    #
    # A timing that is over its line now exits 0, so the needle is what separates
    # "reported the breach" from "did not notice". Every case below whose needle
    # is a breach message therefore also asserts that the run passed.
    cases: list[tuple[str, str, list[str] | None, bool, str]] = [
        (
            "the recorded shipped SSE4.2 ratios report clean, 1.18 included",
            tree(SHIPPED_SSE42),
            None,
            True,
            "kept its median inside its reference line",
        ),
        (
            "the recorded shipped AVX2 ratios report clean, 1.11 included",
            tree(SHIPPED_AVX2),
            None,
            True,
            "kept its median inside its reference line",
        ),
        (
            "a chunk-width SSE4.2 probe is reported as over, and does not fail",
            tree(CHUNK_SSE42),
            None,
            True,
            "::warning::",
        ),
        (
            "a chunk-width SSE4.2 probe names the class and the median",
            tree(CHUNK_SSE42),
            None,
            True,
            "`skip_ident` has a median of 1.412",
        ),
        (
            "a chunk-width AVX2 probe is reported as over",
            tree(CHUNK_AVX2),
            None,
            True,
            "`skip_ident` has a median of 1.591",
        ),
        (
            "AVX2 `skip_ident_start` alone at chunk width is still reported, "
            "which one line over both classes could not have shown",
            tree({ident: SHIPPED_AVX2[ident], start: CHUNK_AVX2[start]}),
            None,
            True,
            "`skip_ident_start` has a median of 1.245",
        ),
        (
            "the lines are per class: 1.20 is inside `skip_ident` and over "
            "`skip_ident_start` in the same tree",
            tree({ident: (1.20,), start: (1.20,)}),
            None,
            True,
            "`skip_ident_start` has a median of 1.200",
        ),
        (
            "one fast round no longer hides a slower majority",
            tree({ident: (1.41, 1.41, 1.18, 1.41, 1.41), start: shipped_start_avx2}),
            None,
            True,
            "`skip_ident` has a median of 1.410",
        ),
        (
            "one slow round does not mark a clean tree",
            tree(
                {
                    ident: (1.186, 1.181, 1.44, 1.178, 1.183),
                    start: shipped_start_avx2,
                }
            ),
            None,
            True,
            "kept its median inside its reference line",
        ),
        (
            "three slow rounds out of five do",
            tree(
                {ident: (1.186, 1.44, 1.44, 1.44, 1.183), start: shipped_start_avx2}
            ),
            None,
            True,
            "`skip_ident` has a median of 1.440",
        ),
        (
            "exactly at the line is inside it; the comparison is strict",
            tree({ident: (1.29,), start: (1.18,)}),
            None,
            True,
            "kept its median inside its reference line",
        ),
        (
            "one thousandth over the line is reported",
            tree({ident: (1.291,), start: (1.18,)}),
            None,
            True,
            "`skip_ident` has a median of 1.291",
        ),
        (
            "the lines are load-bearing in the report: at 1.00 the shipped tree "
            "is marked, so they are not a parity claim",
            tree(SHIPPED_SSE42),
            ["--selftest-reference", f"{ident}=1.00,{start}=1.00"],
            True,
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
            ["--selftest-reference", f"{ident}=1.29,skip_never_benched=1.29"],
            False,
            "no `memspan` result for `skip_never_benched`",
        ),
        (
            "a `--selftest-reference` entry that is not `class=ratio` is refused",
            tree(SHIPPED_SSE42),
            ["--selftest-reference", "skip_ident"],
            False,
            "is not `class=ratio`",
        ),
        (
            "an empty `--selftest-reference` is refused",
            tree(SHIPPED_SSE42),
            ["--selftest-reference", ","],
            False,
            "`--selftest-reference` is empty",
        ),
    ]

    failures: list[str] = []
    try:
        for label, root, extra, expect_ok, needle in cases:
            ok, output = run(root, extra)
            if ok != expect_ok:
                failures.append(
                    f"{label}: expected {'exit 0' if expect_ok else 'failure'}, "
                    f"got {'exit 0' if ok else 'failure'}\n{indent(output)}"
                )
            elif needle and needle not in output:
                failures.append(
                    f"{label}: {needle!r} not in the output\n{indent(output)}"
                )
            else:
                print(f"  {'reports' if expect_ok else 'refuses'}  {label}")
    finally:
        for root in roots:
            shutil.rmtree(root, ignore_errors=True)

    # The clean tree must not merely lack a breach message; it must carry no
    # annotation at all, or a reader scanning for `::warning::` learns nothing.
    ok, output = run(tree(SHIPPED_AVX2))
    if "::warning::" in output:
        failures.append(
            "a clean tree emitted a `::warning::` annotation\n" + indent(output)
        )
    else:
        print("  reports  a clean tree emits no `::warning::` annotation")

    documented = check_documented_tables()
    failures.extend(documented)
    if not documented:
        print(
            "  derives  both docstring tables rederive from the recorded "
            "observations"
        )

    if failures:
        print(f"\n{len(failures)} selftest case(s) wrong:\n")
        for failure in failures:
            print(f"* {failure}")
        return 1

    print(f"\nall {len(cases) + 2} selftest cases behave as claimed")
    return 0


def indent(text: str) -> str:
    return "\n".join(f"      {line}" for line in text.strip().splitlines())


if __name__ == "__main__":
    sys.exit(main())
