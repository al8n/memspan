#!/usr/bin/env python3
"""Prove `ci/test_probe_sweep.py` actually fails when the reporter is broken.

Run with `python3 ci/check_fixtures_bite.py`. Exits non-zero if any mutation
below survives the fixture suite.

# Why this exists as a script and not as a transcript

A passing test suite says nothing on its own; what matters is whether it would
fail. That was checked by hand for several rounds, and hand-checking rotted
twice in ways worth naming:

* a mutation anchored on a string that had since been reformatted, so the edit
  silently did nothing and the suite's green was read as proof of coverage;
* a mutation whose anchor matched more than once, which in a sibling project
  rewrote an unrelated site and invalidated three earlier results.

So every mutation here asserts its anchor occurs **exactly once** and that the
file hash actually changed before the suite is run, and the file is restored
from an in-memory copy afterwards. If an anchor drifts, this fails loudly
instead of reporting a mutation that never landed.

Two of these mutations were added only after they were found to survive: the
suite's first version passed with a 100-call floor put back, and passed again
when the reported reach was taken from the wrong pair. A mutation that survives
is a gap in the fixtures, not a curiosity.
"""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPORTER = os.path.join(HERE, "probe_sweep.py")
FIXTURES = os.path.join(HERE, "test_probe_sweep.py")

# (label, anchor, replacement, what breaking it would mean)
MUTATIONS = [
    (
        "reach quantified over every width instead of each pair",
        "            threshold = min(default_probe, candidate)",
        "            threshold = max(default_probe, candidate)",
        "throws away the runs that distinguish the widths best",
    ),
    (
        "reach reported from the wrong pair",
        "        hits, share, threshold = reach[name][best]",
        "        hits, share, threshold = min(reach[name].values())",
        "licenses a gain with a count about a different comparison",
    ),
    (
        "a chosen floor put back under the zero deduction",
        "            if hits == 0:",
        "            if hits < 100:",
        "refuses rows on an underived threshold",
    ),
    (
        "zero-reach refusal disabled",
        "            if hits == 0:",
        "            if False:",
        "publishes a comparison that provably cannot exist",
    ),
    (
        "a verdict marker reintroduced",
        '            + f" | {gain * 100:+.0f}% @ {best}"',
        '            + (" | _noisy_" if pair_spread > 0.10 else f" | {gain * 100:+.0f}% @ {best}")',
        "asserts a judgement the design cannot ground",
    ),
]


def sha(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()[:12]


def fixtures_pass() -> bool:
    proc = subprocess.run(
        [sys.executable, FIXTURES], capture_output=True, text=True
    )
    return proc.returncode == 0


def main() -> int:
    with open(REPORTER) as handle:
        pristine = handle.read()
    baseline = sha(pristine)

    if not fixtures_pass():
        print("FAILED: fixtures do not pass against the unmodified reporter")
        return 1
    print(f"baseline: fixtures pass, reporter sha {baseline}\n")

    survived: list[str] = []
    try:
        for label, anchor, replacement, meaning in MUTATIONS:
            occurrences = pristine.count(anchor)
            if occurrences != 1:
                print(f"FAILED: anchor for '{label}' occurs {occurrences} times, not 1")
                print("        the mutation would edit the wrong site or nothing at all")
                return 1

            mutated = pristine.replace(anchor, replacement)
            if sha(mutated) == baseline:
                print(f"FAILED: mutation '{label}' did not change the file")
                return 1

            with open(REPORTER, "w") as handle:
                handle.write(mutated)

            caught = not fixtures_pass()
            print(
                f"  {'caught  ' if caught else 'SURVIVED'} {label}\n"
                f"           sha {baseline} -> {sha(mutated)}; {meaning}"
            )
            if not caught:
                survived.append(label)
    finally:
        with open(REPORTER, "w") as handle:
            handle.write(pristine)

    with open(REPORTER) as handle:
        if sha(handle.read()) != baseline:
            print("\nFAILED: could not restore the reporter to its original contents")
            return 1
    print(f"\nrestored: reporter sha {baseline}")

    if survived:
        print(f"\n{len(survived)} mutation(s) survived the fixtures:")
        for label in survived:
            print(f"  * {label}")
        print("\nEach is a gap: the fixtures cannot tell that behaviour from the real one.")
        return 1

    print(f"all {len(MUTATIONS)} mutations caught")
    return 0


if __name__ == "__main__":
    sys.exit(main())
