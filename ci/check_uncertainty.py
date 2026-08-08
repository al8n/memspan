#!/usr/bin/env python3
"""Recompute the measurement that justifies `ci/probe_sweep.py` making no verdicts.

Run with `python3 ci/check_uncertainty.py`. Prints the table and exits non-zero
if the recorded data no longer supports the claim.

# The claim

`ci/probe_sweep.py` reports measurements and passes no judgement on them. One of
the three reasons given is that Criterion's own confidence interval cannot
ground a threshold for this benchmark design, because it describes variation
*within* one run and the dominant variation here is *between* runs.

That reason was originally written into a comment as a bare number — "10 of 15,
up to 16.5x" — with nothing behind it. A number in a comment is exactly the kind
of unverifiable claim the reporter was demoted for making, so the data it came
from is in `ci/reference/round-drift.json` and this script derives the number
from it. If the two ever disagree, this fails rather than the comment quietly
going stale.

The first thing it caught was that comment: the worst ratio is 23.4x, not the
16.5x written down. The 16.5 had been read off a listing truncated to its first
eight rows. The direction of the argument was unaffected, which is exactly why
nobody would have noticed.

The data is two rounds of the same benchmark at one probe width, taken back to
back on one host. For each benchmark it compares |round1 - round2| against the
half-width of round 1's confidence interval. A CI that captured the real
uncertainty would rarely be exceeded; one that is routinely exceeded is
describing something narrower than the question a threshold would need to answer.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "reference", "round-drift.json")

# The claim as stated in `ci/probe_sweep.py`. Kept here as the assertion, so the
# comment and this script cannot drift apart without a failure.
CLAIMED_EXCEEDING = 10
CLAIMED_TOTAL = 15
CLAIMED_WORST_RATIO = 23.4


def main() -> int:
    with open(DATA) as handle:
        data = json.load(handle)
    rows = data["measurements"]

    print(f"host:  {data['_host']}")
    print(f"bench: {data['_bench']}\n")
    print(f"{'benchmark':40s} {'|r1-r2|':>10s} {'CI half':>10s} {'ratio':>8s}")
    print("-" * 72)

    exceeding = 0
    worst = 0.0
    for name in sorted(rows):
        r1, r2 = rows[name]["round1"], rows[name]["round2"]
        drift = abs(r1["point"] - r2["point"])
        half = (r1["ci_hi"] - r1["ci_lo"]) / 2
        ratio = drift / half if half else float("inf")
        if drift > half:
            exceeding += 1
        worst = max(worst, ratio)
        flag = "  <-- exceeds" if drift > half else ""
        print(f"{name:40s} {drift:10.1f} {half:10.1f} {ratio:7.1f}x{flag}")

    total = len(rows)
    print(
        f"\nbetween-round drift exceeded the within-run CI half-width in "
        f"{exceeding}/{total} benchmarks, worst ratio {worst:.1f}x"
    )

    problems: list[str] = []
    if total != CLAIMED_TOTAL:
        problems.append(f"reference data has {total} benchmarks, claim says {CLAIMED_TOTAL}")
    if exceeding != CLAIMED_EXCEEDING:
        problems.append(
            f"{exceeding} benchmarks exceed the CI, claim in probe_sweep.py says "
            f"{CLAIMED_EXCEEDING}"
        )
    if abs(worst - CLAIMED_WORST_RATIO) > 0.05:
        problems.append(
            f"worst ratio is {worst:.1f}x, claim says {CLAIMED_WORST_RATIO}x"
        )

    # The conclusion the reporter rests on, restated as a property rather than
    # as the exact counts: a CI exceeded in most rows cannot ground a bar.
    if exceeding * 2 <= total:
        problems.append(
            "the CI was exceeded in a minority of rows, so it may in fact be an "
            "adequate uncertainty model — the reporter's stated reason for making "
            "no verdicts no longer holds and should be re-argued"
        )

    if problems:
        print("\nFAILED:")
        for p in problems:
            print(f"  * {p}")
        return 1

    print("claim in ci/probe_sweep.py matches the recorded data")
    return 0


if __name__ == "__main__":
    sys.exit(main())
