#!/usr/bin/env python3
"""Render the probe-length sweep produced by `.github/workflows/probe-sweep.yml`.

Reads the criterion estimates written by `benches/short_run.rs` under a set of
`--save-baseline` names of the form ``<probe>_r<round>`` and prints a markdown
table comparing each probe width against the plain scalar loop the bench
measures alongside it.

Three properties this deliberately keeps:

* **Only scanners the constant can actually move are scored.** `CLASS_PROBE`
  sizes the scalar probe in `skip_ascii_class!` and nowhere else; `skip_while`
  and `skip_until` still probe a whole chunk. A row the constant cannot move is
  not evidence about it, whatever its timings say. The bench keeps those
  scanners in a separate `generic_sweep` group and this script refuses to score
  anything outside the class list.

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
MACRO_OPEN = "sweep_classes!("
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
    source = read(repo_root, BENCH_SOURCE)

    # Scope the search to the `sweep_classes!` invocation before matching. The
    # bench has more than one macro that lists classes in the same shape —
    # `realistic_sweep` names some of the same ones — and a pattern applied to
    # the whole file silently unions them. That is not hypothetical: it happened
    # here, and the deleted-row guard went green with a row genuinely removed,
    # because the name it should have missed was still present in the other
    # invocation.
    start = source.find(MACRO_OPEN)
    if start < 0:
        die(
            f"found no `{MACRO_OPEN}` in `{BENCH_SOURCE}`. The sweep's class list "
            "is read from that invocation, so this script is now looking in the "
            "wrong place."
        )
    end = source.find("\n  );", start)
    if end < 0:
        die(f"`{MACRO_OPEN}` in `{BENCH_SOURCE}` is not closed by `\\n  );`.")

    names = set(SWEPT_RE.findall(source[start:end]))
    if not names:
        die(
            f"found no swept classes in the `{MACRO_OPEN}` invocation in "
            f"`{BENCH_SOURCE}`. The sweep would have nothing to report and would "
            "otherwise exit cleanly."
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


# What this script decides, and what it refuses to decide
# =======================================================
#
# It decides **structural** questions — did the run produce what it claims — and
# it renders everything else. Every structural check below is a deduction from a
# count of zero or one; none of them has a tunable number, because every tunable
# number this reporter has carried was an unmeasured claim that a later review
# had to withdraw. First a fraction ("below 5% reach, 95% of the measured time
# is from identical calls"), then a count ("100 calls"). Both were asserted, and
# the first would have rejected the evidence for a change that reproduces.
#
# Statistical questions — is this width faster, is that difference real — are
# reported as measurements and left to the reader, for three reasons that were
# checked rather than assumed:
#
# 1. Nothing consumes a verdict. The only machine reading this script is the
#    workflow step that runs it, the workflow is dispatch-and-schedule only, and
#    no merge or downstream job gates on it. Every consumer of the numbers is a
#    person deciding whether to tune a constant.
# 2. Criterion's own confidence interval cannot ground a threshold here. On the
#    reference run, between-round drift exceeded the within-run CI half-width in
#    10 of 15 benchmarks, by up to 23.4x, so a bar built on those intervals would
#    declare differences real in two thirds of rows where a re-run disagrees.
#    Recorded in `ci/reference/round-drift.json`; `ci/check_uncertainty.py`
#    rederives these counts and fails if they no longer hold.
# 3. Between-round variance cannot ground one either: the design is two rounds,
#    which leaves one degree of freedom. A credible power model needs many more
#    rounds, and that multiplies the job's cost by the same factor.
#
# So the table carries the gain against the shipped width and the spread of the
# pair that produced it, and stops there. A reader can apply whatever bar their
# decision deserves; the script does not pick one on their behalf.


def require_structure(
    criterion_home: str, expected: set[str], probes: list[int], default_probe: int
) -> dict[str, dict[int, tuple[int, float, int]]]:
    """Refuse to publish a row that provably cannot say anything.

    Two refusals, both deductive:

    * **zero reaching calls at a compared width.** A width `w` only changes
      behaviour for runs of at least `w` bytes; below that the scalar probe
      answers and every compared width is the same code. If no call reaches `w`,
      the widths executed identical instructions on every single call and the
      row's timing difference is unrelated variation. This is a count of zero,
      not a threshold.
    * **one call.** The bench's contract is a cursor stepping through a buffer;
      one call means the cursor never moved and there is no run-length
      distribution to speak of.

    A small but non-zero reaching population is **not** refused. It is reported,
    because whether few-but-expensive calls carry a difference is a judgement
    about the workload, not a fact about the run: `skip_ident` reaches width 8
    on 4.2% of its calls and those calls move its ratio from 1.92x to 1.08x.

    Returns the reach counts so the table can print them.
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

        # Reach is a property of a *pair*, not of a width. Two widths diverge on
        # runs of at least min(shipped, candidate): the narrower one has already
        # handed off to the vector loop while the wider one is still answering
        # from its scalar probe. Comparing 8 against 16, a run of 12 bytes is
        # therefore the most informative call there is, and requiring a call to
        # reach 16 would throw it away.
        #
        # An earlier version of this check quantified over every width instead
        # of over each pair. It rejected exactly the corpora that distinguish
        # the widths best, and it pushed toward manufacturing long runs to
        # satisfy the widest width alone — the corpus-realism problem this
        # workflow already has, arriving from the other side. Deleting a chosen
        # number does not remove the obligation to quantify over the right set.
        reach[name] = {}
        empty: list[str] = []
        for candidate in probes:
            if candidate == default_probe:
                continue
            threshold = min(default_probe, candidate)
            hits = sum(count for length, count in advances.items() if length >= threshold)
            reach[name][candidate] = (hits, hits / calls, threshold)
            if hits == 0:
                empty.append(
                    f"probe {candidate} vs shipped {default_probe} diverge only on "
                    f"runs of at least {threshold} bytes, and no call reaches that"
                )

        # Reject the row only when *no* pair can say anything. A single dead
        # pair is marked in its own cell and left to the reader.
        if empty and len(empty) == len(reach[name]):
            failures.append(f"`{name}`: " + "; ".join(empty))

    if failures:
        die(
            f"{len(failures)} scored row(s) provably cannot distinguish the widths "
            "this run compares, so publishing their timings would publish an "
            "empty comparison:\n\n"
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
    reach = require_structure(
        args.criterion_home, expected, probes, args.default_probe
    )

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
        + " | best gain vs shipped | reach | pair spread |"
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

        # The largest gain any candidate shows against the shipped width, with
        # the spread of exactly that pair beside it. Both are arithmetic on the
        # measurements. No bar is applied and no width is marked: a reader
        # decides whether a gain of this size, at this spread, on this many
        # distinguishing calls, is worth acting on.
        #
        # The previous version marked a winner and, when it could not, printed
        # `_noisy_`. That marker was asymmetric: it fired from the top
        # candidate's spread without requiring the candidate to be faster, so a
        # row where every width was slower than shipped and noisy was labelled
        # "not evidence for the shipped width" — destroying a shipped-wins
        # result rather than withholding an undecided one. A negative gain is
        # now simply reported as negative.
        # Only pairs with a distinguishing population can produce a gain. A
        # pair with none is marked in its own cell rather than silently
        # averaged into the row.
        live = [p for p in probes if p != args.default_probe and reach[name][p][0] > 0]
        cells = [
            cell
            if p == args.default_probe or reach[name][p][0] > 0
            else f"{cell} _(n/d)_"
            for p, cell in zip(probes, cells)
        ]

        best = max(live, key=lambda p: (shipped_mean - ratios[name][p]) / shipped_mean)
        gain = (shipped_mean - ratios[name][best]) / shipped_mean
        pair_spread = max(shipped_spread, spread_of(name, best))

        # The reach reported is the one belonging to the pair that produced the
        # gain shown, not the row's smallest — a number about a different
        # comparison would not license this one.
        hits, share, threshold = reach[name][best]

        print(
            f"| `{name}` | "
            + " | ".join(cells)
            + f" | {gain * 100:+.0f}% @ {best}"
            + f" | {hits} ({share * 100:.1f}%) >={threshold}b"
            + f" | +/-{pair_spread * 100:.0f}% |"
        )

    print(
        "\nEach cell is the mean followed by the individual rounds. *best gain vs "
        "shipped* is the largest improvement any non-shipped width shows over the "
        "shipped one, and which width that was; a **negative** value means every "
        "width tested was slower than shipped, which is a result and not a "
        "failure to find one. *pair spread* is the round-to-round spread of the "
        "two cells that gain came from. *reach* is the smallest number of calls "
        "that reached any compared width, with its share.\n\n"
        "**This table makes no pass/fail claim and marks no winner.** Whether a "
        "gain is worth acting on depends on how it compares to that pair's "
        "spread, on how many calls could distinguish the widths at all, and on "
        "which workload you care about — judgements this script cannot make for "
        "you, and whose thresholds it has twice got wrong by asserting them. "
        f"Across the whole table the spread was max {worst_spread * 100:.1f}%, "
        f"median {median_spread * 100:.1f}%."
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
        "Counting how many classes favour a width would report the schedule's "
        "weighting back as a property of the classes, and that weighting is "
        "decision-sensitive: an equal-weight version of it reversed the "
        "direction on fourteen of fifteen rows."
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
