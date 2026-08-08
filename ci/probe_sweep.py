#!/usr/bin/env python3
"""Render the probe-length sweep produced by `.github/workflows/probe-sweep.yml`.

Reads the criterion estimates written by `benches/short_run.rs` under a set of
`--save-baseline` names of the form ``<probe>_r<round>`` and prints a markdown
table comparing each probe width against the plain scalar loop the bench
measures alongside it.

Three properties this deliberately keeps:

* **Only scanners the constant can actually move are scored.** `CLASS_PROBE`
  sizes the scalar probe in `skip_ascii_class!` and nowhere else; `skip_while`
  and `skip_until` still probe a whole chunk. A row that cannot respond to the
  sweep contributes nothing but noise, and that noise would both compete for the
  "best" column and widen the spread the report thresholds against — burying a
  real signal from a scanner that *is* affected. The bench keeps those scanners
  in a separate `generic_sweep` group and this script refuses to score anything
  outside the class list.

* **Both lists are derived, never typed.** The classes come from the
  `skip_ascii_class!` invocations that generate the kernels and from the
  `lexer_sweep` call sites in the bench. A hand-maintained list of what a sweep
  covers is the first thing to go stale, and it goes stale silently.

* **A missing result is an error, not an omission.** A reporter that quietly
  drops a row looks exactly like a clean run. Every expected class x probe x
  round cell must be present, or this exits non-zero and names what is missing.

Nothing here judges a *regression*: a sweep has no before/after, only a set of
widths, so any pass/fail threshold on the timings themselves would be invented
rather than measured. The failures above are all structural.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

# Criterion flattens `/` in a group name into `_` on disk, so the directory for
# group `lexer_sweep/skip_ident` is `lexer_sweep_skip_ident`.
GROUP_PREFIX = "lexer_sweep_"

# Where each backend's kernels are generated. The sweep's notion of which
# scanners exist is read from these files rather than restated here.
BACKEND_SOURCES = {
    "neon": "src/skip/neon.rs",
    "sse42": "src/skip/sse42.rs",
    "avx2": "src/skip/avx2.rs",
    "avx512": "src/skip/avx512.rs",
    "simd128": "src/skip/simd128.rs",
}

BENCH_SOURCE = "benches/short_run.rs"

# Written by the bench beside the criterion results: what the sweep corpus
# actually made each scanner do. See `require_non_vacuous`.
PROFILE_FILE = "corpus-profile.json"

KERNEL_RE = re.compile(r"skip_ascii_class!\(\s*([A-Za-z0-9_]+)\s*,", re.S)
SWEPT_RE = re.compile(r'\(\s*"(skip_[a-z0-9_]+)"\s*,\s*skip::', re.S)


def die(message: str) -> None:
    print(f"\n**probe-sweep failed:** {message}\n")
    sys.exit(1)


def read(repo_root: str, relative: str) -> str:
    path = os.path.join(repo_root, relative)
    try:
        with open(path) as handle:
            return handle.read()
    except OSError as err:
        die(f"cannot read `{relative}`: {err}")
        raise  # unreachable, keeps type checkers happy


def kernels_for(repo_root: str, tier: str) -> set[str]:
    """ASCII-class scanners this backend generates, from the macro call sites."""
    source = BACKEND_SOURCES.get(tier)
    if source is None:
        die(f"unknown tier `{tier}`; known tiers: {', '.join(sorted(BACKEND_SOURCES))}")
    names = set(KERNEL_RE.findall(read(repo_root, source)))
    if not names:
        die(
            f"found no `skip_ascii_class!` invocations in `{source}`. Either the "
            "backend stopped generating its kernels with that macro, in which case "
            "this script is now reading the wrong place, or the path is wrong."
        )
    return names


def swept_classes(repo_root: str) -> set[str]:
    """Classes listed in the bench's `sweep_classes!` invocation."""
    names = set(SWEPT_RE.findall(read(repo_root, BENCH_SOURCE)))
    if not names:
        die(
            f"found no swept classes in `{BENCH_SOURCE}`. The sweep would have "
            "nothing to report and would otherwise exit cleanly."
        )
    return names


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
        with open(estimates) as handle:
            out[rel] = json.load(handle)["mean"]["point_estimate"]
    return out


# A width `w` only changes behaviour for runs of at least `w` bytes: below that
# the scalar probe answers and every compared width behaves identically. A row
# whose corpus rarely produces such a run is not evidence about width even
# though its timings are complete and its scanner plainly did work.
#
# 5% is the floor. Below it, 95% or more of the measured time comes from calls
# in which the compared widths are the same code, so any difference the table
# shows is dominated by variation that has nothing to do with the question.
MIN_REACH_FRACTION = 0.05


def require_widths_exercised(
    criterion_home: str, expected: set[str], probes: list[int]
) -> dict[str, dict[int, tuple[int, float]]]:
    """Refuse to score a class whose corpus cannot distinguish the compared widths.

    Non-vacuity is not enough, and that was the earlier mistake here. A corpus
    with many calls and advances of 0/1/2/5 has plenty of work in it and is
    still silent about a comparison between 8 and 64: every call returns from
    the scalar probe under both. The predicate cannot be a property of the
    distribution in the abstract — it has to be a count relative to the widths
    actually under comparison, which is why the probe list is a parameter.

    Returns the reach counts so the report can print them, because a reader
    should be able to see how much of each row's evidence was on-topic.
    """
    path = os.path.join(criterion_home, PROFILE_FILE)
    if not os.path.exists(path):
        die(
            f"no `{PROFILE_FILE}` beside the results. The bench writes it while "
            "building each class's corpus, so its absence means the sweep ran a "
            "bench that does not record what it measured — the rows cannot be "
            "trusted even if they look complete."
        )
    try:
        with open(path) as handle:
            profile = json.load(handle)
    except (OSError, ValueError) as err:
        die(f"cannot read `{PROFILE_FILE}`: {err}")

    unprofiled = sorted(expected - set(profile))
    if unprofiled:
        die(
            "no corpus profile for "
            + ", ".join(f"`{u}`" for u in unprofiled)
            + ". Every scored class must record what its corpus made it do."
        )

    reach: dict[str, dict[int, tuple[int, float]]] = {}
    failures: list[str] = []
    for name in sorted(expected):
        entry = profile[name]
        calls = entry["calls"]
        advances = {int(k): v for k, v in entry["advances"].items()}
        if calls <= 1:
            failures.append(
                f"`{name}`: {calls} call over {entry['buf_len']} bytes — the whole "
                "corpus is in the class, so this measures one long tail rather "
                "than a lexer's run lengths"
            )
            continue

        reach[name] = {}
        thin: list[str] = []
        for width in probes:
            hits = sum(count for length, count in advances.items() if length >= width)
            fraction = hits / calls
            reach[name][width] = (hits, fraction)
            if fraction < MIN_REACH_FRACTION:
                thin.append(
                    f"width {width} reached by {hits}/{calls} calls "
                    f"({fraction * 100:.1f}%)"
                )
        if thin:
            failures.append(f"`{name}`: " + "; ".join(thin))

    if failures:
        die(
            f"{len(failures)} scored row(s) cannot distinguish the widths this run "
            f"compares. A width only changes behaviour for runs at least that "
            f"long, and fewer than {MIN_REACH_FRACTION * 100:.0f}% of calls reach "
            "the widths named below, so those rows would report a timing that is "
            "silent about the question:\n\n"
            + "\n".join(f"* {f}" for f in failures)
            + f"\n\nEither lengthen `RUN_SCHEDULE` in `{BENCH_SOURCE}` so the "
            "corpus produces runs at those widths, or compare narrower widths in "
            "the workflow matrix. Do not drop the row: the set checks require "
            "every kernel to be swept."
        )
    return reach


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-home", required=True)
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--tier", required=True, help=" | ".join(sorted(BACKEND_SOURCES)))
    parser.add_argument(
        "--default-probe",
        required=True,
        type=int,
        help="the width this backend ships with, marked in the table",
    )
    parser.add_argument(
        "--probes",
        required=True,
        help="comma-separated widths, in the order they were run",
    )
    parser.add_argument("--rounds", default=2, type=int)
    args = parser.parse_args()

    probes = [int(p) for p in args.probes.split(",")]
    rounds = list(range(1, args.rounds + 1))

    kernels = kernels_for(args.repo_root, args.tier)
    expected = swept_classes(args.repo_root)

    # The swept set and the generated set must be *equal*, not merely
    # compatible. Each list is recovered by a regular expression over source
    # text, and a regular expression recognises rather than decides: on its own,
    # a row deleted from the bench, renamed, or written in a shape the pattern
    # misses would simply shrink `expected`, and the sweep would report a
    # shorter table and exit 0 with the most interesting class gone. Requiring
    # the two independently-recovered sets to agree turns every such miss into a
    # mismatch, because the other list still has the name.
    stowaways = sorted(expected - kernels)
    if stowaways:
        die(
            "the `lexer_sweep` bench group contains "
            + ", ".join(f"`{s}`" for s in stowaways)
            + f", which `{BACKEND_SOURCES[args.tier]}` does not generate with "
            "`skip_ascii_class!` and which therefore never reads `CLASS_PROBE`. "
            "Move it to the `generic_sweep` group."
        )

    unswept = sorted(kernels - expected)
    if unswept:
        die(
            f"`{BACKEND_SOURCES[args.tier]}` generates "
            + ", ".join(f"`{u}`" for u in unswept)
            + " with `skip_ascii_class!`, but the `sweep_classes!` invocation in "
            f"`{BENCH_SOURCE}` does not sweep "
            + ("them" if len(unswept) > 1 else "it")
            + ". The sweep covers every kernel the constant can move or it "
            "covers none: a partial table cannot be told apart from a complete "
            "one by anybody reading the report. Add the missing "
            + ("entries" if len(unswept) > 1 else "entry")
            + " to the table, or move the kernel out of `skip_ascii_class!`."
        )

    # measured[probe][round][class] -> {impl: ns}
    measured: dict[int, dict[int, dict[str, dict[str, float]]]] = {}
    for probe in probes:
        measured[probe] = {}
        for rnd in rounds:
            per_class: dict[str, dict[str, float]] = {}
            for key, value in load(args.criterion_home, f"{probe}_r{rnd}").items():
                parts = key.split(os.sep)
                if len(parts) < 3 or not parts[0].startswith(GROUP_PREFIX):
                    continue
                per_class.setdefault(parts[0].removeprefix(GROUP_PREFIX), {})[
                    parts[-2]
                ] = value
            measured[probe][rnd] = per_class

    # Third independent source: what actually ran. A group present in the
    # results but absent from the bench table means the table regex missed a row
    # that criterion did execute — the one way a source-text miss could have
    # cancelled out above.
    produced = {
        name
        for probe in probes
        for rnd in rounds
        for name in measured[probe][rnd]
    }
    surprises = sorted(produced - expected)
    if surprises:
        die(
            "the sweep produced results for "
            + ", ".join(f"`{s}`" for s in surprises)
            + f", which are not in the `sweep_classes!` invocation in "
            f"`{BENCH_SOURCE}`. The table is parsed from source text, so this "
            "means the parse missed a row that the bench really ran."
        )

    # Set agreement proves a kernel was named and produced a cell. It cannot
    # prove the cell measured the property this report is about, because a
    # scanner that never advances past byte 0 produces a perfectly ordinary
    # `memspan` and `scalar` pair. A case can be present, counted, and vacuous.
    #
    # The corpus profile is the bench's record of what it actually walked, and
    # it is computed with the scalar predicate rather than with the library, so
    # a broken kernel cannot make a dead corpus look alive.
    reach = require_widths_exercised(args.criterion_home, expected, probes)

    # Every expected cell must exist, with both series the ratio needs. A filter
    # that matched nothing, a build that silently produced no bench, or a run
    # that died halfway all land here instead of rendering a shorter table.
    missing: list[str] = []
    for name in sorted(expected):
        for probe in probes:
            for rnd in rounds:
                impls = measured[probe][rnd].get(name)
                if impls is None:
                    missing.append(f"{name} @ probe {probe}, round {rnd}: no result")
                    continue
                for series in ("memspan", "scalar"):
                    if series not in impls:
                        missing.append(
                            f"{name} @ probe {probe}, round {rnd}: no `{series}`"
                        )
    if missing:
        die(
            f"{len(missing)} expected result(s) absent — the sweep did not measure "
            "what it claims to report:\n\n"
            + "\n".join(f"* {m}" for m in missing[:40])
            + ("\n* ..." if len(missing) > 40 else "")
        )

    print(f"### `{args.tier}` probe-length sweep\n")
    print(
        "Each cell is the scanner's time divided by the plain scalar "
        "`position` loop measured in the same run, so it is immune to drift "
        "between runs. Lower is better; `1.00` means parity with scalar.\n"
    )

    # First pass: every ratio, and the spread between rounds. The spread has to
    # be known before anything is marked, otherwise the threshold would depend
    # on the order the rows happen to be rendered in.
    ratios: dict[str, dict[int, float]] = {}
    per_round: dict[str, dict[int, list[float]]] = {}
    spreads: list[float] = []
    for name in sorted(expected):
        ratios[name] = {}
        per_round[name] = {}
        for probe in probes:
            seen = [
                measured[probe][rnd][name]["memspan"]
                / measured[probe][rnd][name]["scalar"]
                for rnd in rounds
            ]
            mean = sum(seen) / len(seen)
            ratios[name][probe] = mean
            per_round[name][probe] = seen
            if mean:
                spreads.append((max(seen) - min(seen)) / mean)

    worst_spread = max(spreads)
    median_spread = sorted(spreads)[len(spreads) // 2]

    print(
        "| class | "
        + " | ".join(
            f"probe {p}{' (shipped)' if p == args.default_probe else ''}"
            for p in probes
        )
        + " | beats shipped | reach | noise |"
    )
    print("|" + "---|" * (len(probes) + 4))

    def spread_of(name: str, probe: int) -> float:
        """Round-to-round spread of one cell, relative to its own mean."""
        seen = per_round[name][probe]
        mean = ratios[name][probe]
        return (max(seen) - min(seen)) / mean if mean else 0.0

    for name in sorted(expected):
        cells = [
            f"{ratios[name][p]:.2f} ({'/'.join(f'{r:.2f}' for r in per_round[name][p])})"
            for p in probes
        ]

        shipped_mean = ratios[name][args.default_probe]
        shipped_spread = spread_of(name, args.default_probe)

        # Each candidate is judged against the shipped width using only those
        # two cells' noise. Taking the row's worst spread instead would let one
        # bad round for an unrelated probe raise the bar for every comparison in
        # the row and veto a genuine winner — the same contamination as scoring
        # an unaffected scanner, one level further in. The 10% floor keeps a
        # suspiciously quiet run from promoting a rounding difference.
        winners: list[tuple[float, int]] = []
        best_gain = float("-inf")
        best_blocked_by_noise = False
        deciding_noise = shipped_spread
        for probe in probes:
            if probe == args.default_probe:
                continue
            pair_noise = max(shipped_spread, spread_of(name, probe))
            bar = max(0.10, pair_noise)
            gain = (shipped_mean - ratios[name][probe]) / shipped_mean
            if gain > bar:
                winners.append((ratios[name][probe], probe))
            if gain > best_gain:
                # Whether the *strongest* candidate was stopped by measurement
                # noise or by the floor is the difference between "we could not
                # tell" and "the shipped width held". Only the first is noise.
                best_gain = gain
                best_blocked_by_noise = pair_noise > 0.10
                deciding_noise = pair_noise

        # Every candidate that cleared its own bar is listed. Picking the
        # lowest point estimate among them would report a pairwise result — "is
        # faster than the shipped width" — as an exact optimum, and two winners
        # can differ from each other by less than either differs from shipped.
        # This column answers only the question the comparison actually asked.
        if winners:
            mark = ", ".join(f"**{p}**" for _, p in sorted(winners, key=lambda w: w[1]))
        elif best_blocked_by_noise:
            mark = "_noisy_"
        else:
            mark = "none"

        worst_reach = min(reach[name][p][1] for p in probes)
        print(
            f"| `{name}` | "
            + " | ".join(cells)
            + f" | {mark} | {worst_reach * 100:.0f}% | +/-{deciding_noise * 100:.0f}% |"
        )

    print(
        "\nEach cell is the mean followed by the individual rounds. *beats "
        "shipped* lists **every** width that beat the shipped one by more than "
        "the noise of those two cells alone, floored at 10%; it is a pairwise "
        "result and deliberately not a ranking, because two listed widths may "
        "differ from each other by less than either differs from shipped. "
        "`none` means no width cleared that bar. `_noisy_` means the bar was set "
        "by noise rather than by the floor, so this run could not decide — it is "
        "**not** evidence for the shipped width. *reach* is the smallest "
        "fraction of calls that reached any compared width; below "
        f"{MIN_REACH_FRACTION * 100:.0f}% the row is refused outright. Across the "
        f"whole table the spread was max {worst_spread * 100:.1f}%, median "
        f"{median_spread * 100:.1f}%."
    )

    print(
        "\n> **Scope.** Every row above was measured on one shared synthetic "
        "run-length schedule, not on that class's real distribution. The "
        "schedule's shape was derived from two classes (`skip_ident`, "
        "`skip_alpha`) profiled on a PromQL corpus and then reused for all "
        "fifteen, and it is deliberately longer-running than lexer input so that "
        "the wider compared widths are reachable at all. A row is evidence about "
        "**this schedule**; it is not a measurement of how that class behaves in "
        "any caller.\n>\n> For the same reason these rows **must not be added up**. "
        "Counting how many classes prefer a width would report the schedule's "
        "weighting back as a property of the classes, and that weighting is "
        "decision-sensitive: an equal-weight version of it reversed the "
        "preference on fourteen of fifteen rows. Treat each row as one pairwise "
        "comparison under stated conditions, and treat any cross-class "
        "conclusion as unsupported by this table."
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
