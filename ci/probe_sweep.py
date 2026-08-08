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

KERNEL_RE = re.compile(r"skip_ascii_class!\(\s*([A-Za-z0-9_]+)\s*,", re.S)
SWEPT_RE = re.compile(
    r'one\(\s*c\s*,\s*"lexer_sweep"\s*,\s*&buf\s*,\s*"([A-Za-z0-9_]+)"', re.S
)


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
    """Classes the bench files under the `lexer_sweep` group."""
    names = set(SWEPT_RE.findall(read(repo_root, BENCH_SOURCE)))
    if not names:
        die(
            f"found no `lexer_sweep` call sites in `{BENCH_SOURCE}`. The sweep would "
            "have nothing to report and would otherwise exit cleanly."
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

    # A scanner in the swept group that this backend does not generate through
    # `skip_ascii_class!` does not read `CLASS_PROBE`, so the sweep cannot move
    # it and scoring it would be a category error.
    stowaways = sorted(expected - kernels)
    if stowaways:
        die(
            "the `lexer_sweep` bench group contains "
            + ", ".join(f"`{s}`" for s in stowaways)
            + f", which `{BACKEND_SOURCES[args.tier]}` does not generate with "
            "`skip_ascii_class!` and which therefore never reads `CLASS_PROBE`. "
            "Move it to the `generic_sweep` group."
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
        + " | best | noise |"
    )
    print("|" + "---|" * (len(probes) + 3))

    for name in sorted(expected):
        cells = [
            f"{ratios[name][p]:.2f} ({'/'.join(f'{r:.2f}' for r in per_round[name][p])})"
            for p in probes
        ]

        # The bar is *this row's* worst round-to-round spread, not the whole
        # table's. A single noisy cell anywhere would otherwise set the bar for
        # every class and bury every real signal — the same way an unaffected
        # scanner in the group would, one level in. The 10% floor keeps a
        # suspiciously quiet run from promoting a 1% difference to a finding.
        row_spread = max(
            (max(seen) - min(seen)) / ratios[name][p]
            for p, seen in per_round[name].items()
            if ratios[name][p]
        )
        row_threshold = max(0.10, row_spread)

        best = min(ratios[name], key=lambda p: ratios[name][p])
        shipped = ratios[name].get(args.default_probe)
        if shipped and ratios[name][best] < shipped * (1 - row_threshold):
            mark = f"**{best}**"
        else:
            mark = str(args.default_probe)
        print(
            f"| `{name}` | "
            + " | ".join(cells)
            + f" | {mark} | +/-{row_spread * 100:.0f}% |"
        )

    print(
        f"\nEach cell shows the mean and then the individual rounds. A width is "
        "marked in *best* only if it beats the shipped one by more than that "
        "row's own noise column (floored at 10%); a plain number means the "
        "shipped width is within noise of the best. Across the whole table the "
        f"round-to-round spread was max {worst_spread * 100:.1f}%, median "
        f"{median_spread * 100:.1f}% — if that max is large, the runner was busy "
        "and the marks are worth less than usual."
    )

    # Not a failure: the bench samples the predicate-complexity range rather than
    # every class, because the sweep's cost is linear in the number of classes.
    # It is printed so the sample stays a visible choice instead of becoming an
    # invisible assumption.
    unswept = sorted(kernels - expected)
    if unswept:
        print(
            f"\n{len(unswept)} of this backend's {len(kernels)} `skip_ascii_class!` "
            "kernels are not in the sweep: "
            + ", ".join(f"`{u}`" for u in unswept)
            + ". The swept set spans the predicate-complexity range that drives the "
            "effect, from single-term to three-term; widen it in "
            f"`{BENCH_SOURCE}` if a specific class needs its own number."
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
