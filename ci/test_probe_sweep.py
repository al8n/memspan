#!/usr/bin/env python3
"""Fixtures for `ci/probe_sweep.py`.

Run with `python3 ci/test_probe_sweep.py`. Exits non-zero on the first failure.

# Why every expectation here is a literal

Each expected row is a string typed out by hand, with the arithmetic done away
from the reporter and the result written down. Nothing in this file calls the
reporter to find out what it should say, and nothing recomputes a ratio the way
the reporter computes it.

That is deliberate. A sibling project's property suite encoded the very bug it
was meant to catch, because the checker derived its expectation from the
implementation: the generator had been producing failing inputs all along and
they passed, because subject and check agreed. Rounds of testing could not have
found it. A test that asks the reporter what the answer is can only ever confirm
that the reporter is self-consistent.

So the inputs below are written as raw criterion JSON rather than produced by
the bench, and the outputs are written as literal table rows. If the reporter's
formatting changes, these fail and someone re-derives them by hand — which is
the point.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
REPORTER = os.path.join(HERE, "probe_sweep.py")

# The fifteen kernels the bench sweeps. Listed here so a fixture is a complete
# run; the reporter requires the set to match the backend and the bench.
CLASSES = [
    "skip_binary",
    "skip_octal_digits",
    "skip_digits",
    "skip_hex_digits",
    "skip_alpha",
    "skip_alphanumeric",
    "skip_ident_start",
    "skip_ident",
    "skip_whitespace",
    "skip_lower",
    "skip_upper",
    "skip_ascii",
    "skip_non_ascii",
    "skip_ascii_graphic",
    "skip_ascii_control",
]

failures: list[str] = []


def write_estimate(home: str, group: str, series: str, baseline: str, ns: float) -> None:
    d = os.path.join(home, group, series, "65536", baseline)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "estimates.json"), "w") as handle:
        json.dump(
            {
                "mean": {
                    "point_estimate": ns,
                    "confidence_interval": {"lower_bound": ns, "upper_bound": ns},
                }
            },
            handle,
        )


def build(home: str, ratios: dict[str, dict[int, list[float]]], profile: dict) -> None:
    """Write a complete fixture run. `ratios[class][probe] = [round1, round2]`."""
    for cls in CLASSES:
        per_probe = ratios.get(cls, {8: [1.0, 1.0], 16: [1.0, 1.0]})
        for probe, rounds in per_probe.items():
            for i, ratio in enumerate(rounds, start=1):
                write_estimate(home, f"lexer_sweep_{cls}", "scalar", f"{probe}_r{i}", 1000.0)
                write_estimate(
                    home, f"lexer_sweep_{cls}", "memspan", f"{probe}_r{i}", 1000.0 * ratio
                )
    with open(os.path.join(home, "corpus-profile.json"), "w") as handle:
        json.dump(profile, handle)


def run(home: str) -> tuple[int, str]:
    proc = subprocess.run(
        [
            sys.executable, REPORTER,
            "--criterion-home", home,
            "--repo-root", REPO,
            "--tier", "neon",
            "--default-probe", "8",
            "--probes", "16,8",
            "--rounds", "2",
        ],
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


def check(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name}" + (f"\n       {detail}" if detail else ""))
        failures.append(name)


# A profile whose every class reaches both compared widths on 500 of 5000 calls.
# 4500 calls advance 2 bytes, 500 advance 40.
HEALTHY = {c: {"calls": 5000, "buf_len": 1048576, "advances": {"2": 4500, "40": 500}} for c in CLASSES}


def fixture_negative_gain() -> None:
    """A row where the shipped width wins must report a negative gain.

    Hand-derived: shipped (8) runs at 680/1000 = 0.68 in both rounds; probe 16
    at 720/1000 = 0.72 in both. Gain of the only candidate over shipped is
    (0.68 - 0.72) / 0.68 = -0.0588, which formats as -6%. Both cells are
    identical across rounds so each spread is 0 and the pair spread is 0%.
    Reach is 500 of 5000 = 10.0%.

    Before this revision the reporter printed `_noisy_` here whenever the pair
    spread happened to exceed 10%, asserting the run was "not evidence for the
    shipped width" in a row where the shipped width had plainly won.
    """
    home = tempfile.mkdtemp()
    try:
        build(home, {"skip_ident": {8: [0.68, 0.68], 16: [0.72, 0.72]}}, HEALTHY)
        code, out = run(home)
        expected = (
            "| `skip_ident` | 0.72 (0.72/0.72) | 0.68 (0.68/0.68) "
            "| -6% @ 16 | 500 (10.0%) | +/-0% |"
        )
        check("negative gain: exits 0", code == 0, out[-400:])
        check("negative gain: row is literal", expected in out,
              "expected:\n       " + expected)
        check("negative gain: no verdict words", "_noisy_" not in out and "beats shipped" not in out)
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_noisy_negative_gain() -> None:
    """The same, with a spread far above the old 10% bar.

    Hand-derived: shipped (8) rounds 0.60 and 0.80, mean 0.70, spread
    (0.80-0.60)/0.70 = 28.6%. Probe 16 rounds 0.90 and 0.90, mean 0.90. Gain is
    (0.70 - 0.90) / 0.70 = -0.2857, formatting as -29%. Pair spread is the
    larger of 28.6% and 0%, formatting as +/-29%.

    This is the exact shape the old marker destroyed: high spread, every
    candidate slower than shipped.
    """
    home = tempfile.mkdtemp()
    try:
        build(home, {"skip_lower": {8: [0.60, 0.80], 16: [0.90, 0.90]}}, HEALTHY)
        code, out = run(home)
        expected = (
            "| `skip_lower` | 0.90 (0.90/0.90) | 0.70 (0.60/0.80) "
            "| -29% @ 16 | 500 (10.0%) | +/-29% |"
        )
        check("noisy negative gain: exits 0", code == 0, out[-400:])
        check("noisy negative gain: row is literal", expected in out,
              "expected:\n       " + expected)
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_small_reach_is_reported_not_refused() -> None:
    """A few-but-real distinguishing population must render, not be refused.

    950 of 22793 calls is 4.2% — the shape of the evidence behind the merged
    NEON narrowing, and below both thresholds this reporter has since withdrawn.
    It must appear in the table with its count so a reader can weigh it.
    """
    home = tempfile.mkdtemp()
    try:
        profile = dict(HEALTHY)
        profile["skip_ident"] = {
            "calls": 22793,
            "buf_len": 1048576,
            "advances": {"0": 13000, "1": 4000, "4": 4843, "19": 950},
        }
        build(home, {}, profile)
        code, out = run(home)
        check("small reach: exits 0", code == 0, out[-400:])
        check("small reach: count and share shown", "950 (4.2%)" in out,
              "expected the substring '950 (4.2%)'")
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_tiny_reach_is_reported_not_refused() -> None:
    """A *tiny* non-zero population must still render — there is no floor but zero.

    12 of 9000 calls is 0.1%: far too few to conclude anything from, and that is
    the reader's problem to weigh, not the reporter's to pre-empt. This fixture
    exists because the 950-call one above cannot detect a reintroduced floor of
    100 — it sits above it. A fixture that cannot fail under the mutation it
    claims to guard against is not guarding anything, which is how the first
    version of this suite passed with a 100-call floor put back.
    """
    home = tempfile.mkdtemp()
    try:
        profile = dict(HEALTHY)
        profile["skip_upper"] = {
            "calls": 9000,
            "buf_len": 1048576,
            "advances": {"0": 5000, "1": 2500, "5": 1488, "40": 12},
        }
        build(home, {}, profile)
        code, out = run(home)
        check("tiny reach: exits 0", code == 0, out[-400:])
        check("tiny reach: count and share shown", "12 (0.1%)" in out,
              "expected the substring '12 (0.1%)'")
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_zero_reach_is_refused() -> None:
    """Zero reaching calls is a deduction, not a threshold: it must refuse."""
    home = tempfile.mkdtemp()
    try:
        profile = dict(HEALTHY)
        profile["skip_digits"] = {
            "calls": 9000,
            "buf_len": 1048576,
            "advances": {"0": 5000, "1": 2000, "2": 1500, "5": 500},
        }
        build(home, {}, profile)
        code, out = run(home)
        check("zero reach: exits 1", code == 1, f"got {code}")
        check("zero reach: names the class", "`skip_digits`" in out)
        check("zero reach: names the width", "width 16" in out or "width 8" in out)
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_single_call_is_refused() -> None:
    """One call means the cursor never moved: no distribution to report."""
    home = tempfile.mkdtemp()
    try:
        profile = dict(HEALTHY)
        profile["skip_ascii"] = {
            "calls": 1,
            "buf_len": 1048576,
            "advances": {"1048576": 1},
        }
        build(home, {}, profile)
        code, out = run(home)
        check("single call: exits 1", code == 1, f"got {code}")
        check("single call: names the class", "`skip_ascii`" in out)
    finally:
        shutil.rmtree(home, ignore_errors=True)


def fixture_missing_profile_is_refused() -> None:
    """The file that licenses the reach column must be present."""
    home = tempfile.mkdtemp()
    try:
        build(home, {}, HEALTHY)
        os.remove(os.path.join(home, "corpus-profile.json"))
        code, out = run(home)
        check("missing profile: exits 1", code == 1, f"got {code}")
    finally:
        shutil.rmtree(home, ignore_errors=True)


def main() -> int:
    print("probe_sweep fixtures")
    for fixture in (
        fixture_negative_gain,
        fixture_noisy_negative_gain,
        fixture_small_reach_is_reported_not_refused,
        fixture_tiny_reach_is_reported_not_refused,
        fixture_zero_reach_is_refused,
        fixture_single_call_is_refused,
        fixture_missing_profile_is_refused,
    ):
        print(f"\n{fixture.__name__}:")
        fixture()
    print()
    if failures:
        print(f"{len(failures)} fixture(s) failed: {', '.join(failures)}")
        return 1
    print("all fixtures passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
