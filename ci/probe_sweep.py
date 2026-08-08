#!/usr/bin/env python3
"""Render the probe-length sweep produced by `.github/workflows/probe-sweep.yml`.

Reads the criterion estimates written by `benches/short_run.rs` under a set of
`--save-baseline` names of the form ``<probe>_r<round>`` and prints a markdown
table comparing each probe width against the plain scalar loop the bench
measures alongside it.

Two properties this deliberately keeps:

* **Both rounds are printed, not just their mean.** These run on shared CI
  runners whose noise is far larger than a quiet laptop's, and a reader needs
  to see the spread to know how much of a difference to believe.
* **Nothing here fails the build.** A sweep has no before/after to regress
  from, so any pass/fail threshold would be invented rather than measured. The
  script only *marks* a row where some non-default probe beats the shipped one
  by more than the spread observed in that same run.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

# The group the sweep reads. `lexer_sweep` varies the run length from call to
# call, which is the only population in which the probe width has ever shown a
# difference; `short_run` repeats one length and predicts every branch.
# Criterion flattens `/` in a group name into `_` on disk, so the directory
# for group `lexer_sweep/skip_ident` is `lexer_sweep_skip_ident`.
GROUP_PREFIX = "lexer_sweep_"


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


def group_of(key: str) -> tuple[str, str] | None:
    """Split `lexer_sweep_skip_x/impl/param` into (group, impl)."""
    parts = key.split(os.sep)
    if len(parts) < 3:
        return None
    return os.sep.join(parts[:-2]), parts[-2]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-home", required=True)
    parser.add_argument("--tier", required=True, help="sse42 | avx2 | avx512 | neon")
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
    rounds = range(1, args.rounds + 1)

    # data[probe][round][group] -> {impl: ns}
    data: dict[int, dict[int, dict[str, dict[str, float]]]] = {}
    for probe in probes:
        data[probe] = {}
        for rnd in rounds:
            per_group: dict[str, dict[str, float]] = {}
            for key, value in load(args.criterion_home, f"{probe}_r{rnd}").items():
                if not key.startswith(GROUP_PREFIX):
                    continue
                split = group_of(key)
                if split is None:
                    continue
                group, impl = split
                per_group.setdefault(group, {})[impl] = value
            data[probe][rnd] = per_group

    groups = sorted({g for p in probes for r in rounds for g in data[p][r]})
    if not groups:
        print("No `lexer_sweep` results found — the bench did not run.")
        return 0

    print(f"### `{args.tier}` probe-length sweep\n")
    print(
        "Each cell is the scanner's time divided by the plain scalar "
        "`position` loop measured in the same run, so it is immune to drift "
        "between runs. Lower is better; `1.00` means parity with scalar.\n"
    )
    # First pass: every ratio, and the spread between rounds for each cell.
    # The spread has to be known before anything is marked, otherwise the
    # threshold would depend on the order the rows happen to be rendered in.
    ratios: dict[str, dict[int, float]] = {}
    per_round_ratios: dict[str, dict[int, list[float]]] = {}
    spreads: list[float] = []
    for group in groups:
        ratios[group] = {}
        per_round_ratios[group] = {}
        for probe in probes:
            rounds_seen = []
            for rnd in rounds:
                impls = data[probe][rnd].get(group, {})
                span, scalar = impls.get("memspan"), impls.get("scalar")
                if span and scalar:
                    rounds_seen.append(span / scalar)
            if not rounds_seen:
                continue
            mean = sum(rounds_seen) / len(rounds_seen)
            ratios[group][probe] = mean
            per_round_ratios[group][probe] = rounds_seen
            if mean:
                spreads.append((max(rounds_seen) - min(rounds_seen)) / mean)

    spreads = spreads or [0.0]
    worst_spread = max(spreads)
    median_spread = sorted(spreads)[len(spreads) // 2]
    # A width is only worth calling out if it beats the shipped one by more
    # than this runner's own round-to-round noise. The floor keeps a
    # suspiciously quiet run from marking a 1% difference as a finding.
    threshold = max(0.10, worst_spread)

    header = (
        "| class | "
        + " | ".join(
            f"probe {p}{' (shipped)' if p == args.default_probe else ''}"
            for p in probes
        )
        + " | best |"
    )
    print(header)
    print("|" + "---|" * (len(probes) + 2))

    for group in groups:
        cells = []
        for probe in probes:
            if probe not in ratios[group]:
                cells.append("n/a")
                continue
            seen = per_round_ratios[group][probe]
            cells.append(
                f"{ratios[group][probe]:.2f} ({'/'.join(f'{r:.2f}' for r in seen)})"
            )

        mark = ""
        if ratios[group]:
            best = min(ratios[group], key=lambda p: ratios[group][p])
            shipped = ratios[group].get(args.default_probe)
            if shipped and ratios[group][best] < shipped * (1 - threshold):
                mark = f" **{best}**"
            else:
                mark = f" {args.default_probe}"
        name = group.removeprefix(GROUP_PREFIX)
        print(f"| `{name}` | " + " | ".join(cells) + f" |{mark} |")

    print(
        f"\nRound-to-round spread on this runner: max {worst_spread * 100:.1f}%, "
        f"median {median_spread * 100:.1f}%; a width is marked only if it beats "
        f"the shipped one by more than {threshold * 100:.1f}%. A **bold** width "
        "in the *best* column is worth acting on; a plain number means the "
        "shipped width is within noise of the best."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
