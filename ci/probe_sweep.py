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
    """Classes listed in the bench's `LEXER_SWEEP_CLASSES` table."""
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


def require_non_vacuous(criterion_home: str, expected: set[str]) -> None:
    """Refuse to score a class whose corpus never exercised the probe.

    Three ways a row can exist and mean nothing:

    * **every advance is zero** — no byte of the corpus is in the class, so the
      scanner returns at the first byte on every call and never reaches the code
      the probe width governs;
    * **one all-match tail** — the whole corpus is in the class, so the sweep
      makes a single call and the run length is the buffer, not a lexer's token;
    * **no variation** — the run lengths barely differ, so every branch in the
      scanner is perfectly predicted and the mispredict cost the probe width
      exists to control cannot appear.

    All three produce complete, well-formed criterion output.
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

    vacuous: list[str] = []
    for name in sorted(expected):
        entry = profile[name]
        calls, distinct, longest = entry["calls"], entry["distinct"], entry["max"]
        if longest == 0:
            vacuous.append(
                f"`{name}`: every one of {calls} advances was 0 — no byte of its "
                "corpus is in the class, so the scanner never classified past "
                "byte 0 and the probe width cannot have affected the timing"
            )
        elif calls <= 1:
            vacuous.append(
                f"`{name}`: {calls} call covering {longest} bytes — the whole "
                "corpus is in the class, so this measures one long tail rather "
                "than a lexer's run lengths"
            )
        elif distinct < 3:
            vacuous.append(
                f"`{name}`: only {distinct} distinct run length(s) across "
                f"{calls} calls — with lengths this uniform every branch is "
                "predicted and the cost the probe width controls cannot appear"
            )
    if vacuous:
        die(
            f"{len(vacuous)} scored row(s) would report a timing in which the "
            "probe width can play no part:\n\n"
            + "\n".join(f"* {v}" for v in vacuous)
            + "\n\nFix the class's fill/miss bytes in the `sweep_classes!` "
            f"invocation in `{BENCH_SOURCE}`; do not drop the row, because the "
            "set checks require every kernel to be swept."
        )


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
            + " with `skip_ascii_class!`, but the `LEXER_SWEEP_CLASSES` table in "
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
            + f", which are not in the `LEXER_SWEEP_CLASSES` table in "
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
    require_non_vacuous(args.criterion_home, expected)

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

        if winners:
            mark = f"**{min(winners)[1]}**"
        elif best_blocked_by_noise:
            mark = "_noisy_"
        else:
            mark = str(args.default_probe)

        print(
            f"| `{name}` | "
            + " | ".join(cells)
            + f" | {mark} | +/-{deciding_noise * 100:.0f}% |"
        )

    print(
        "\nEach cell is the mean followed by the individual rounds. The noise "
        "column is the round-to-round spread of the comparison that decided the "
        "row — the worse of the shipped width's cell and the strongest "
        "candidate's — so it is the bar that verdict had to clear, not a "
        "table-wide figure. A candidate is marked in *best* only if it beats the "
        "shipped width by more than that pair's own noise, floored at 10%; "
        "nothing outside those two cells can raise or lower the bar. A plain "
        "number means the shipped width genuinely held. `_noisy_` means the bar "
        "was set by noise rather than by the floor, so this run could not "
        "decide — it is **not** evidence for the shipped width. Across the whole "
        f"table the spread was max {worst_spread * 100:.1f}%, median "
        f"{median_spread * 100:.1f}%."
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
