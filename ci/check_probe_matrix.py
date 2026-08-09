#!/usr/bin/env python3
"""Hold the probe-sweep matrix against the constants and flags it restates.

Run with `python3 ci/check_probe_matrix.py`; `--selftest` proves it bites;
`--print-required-tiers` emits the tiers whose measurement is mandatory.

# What this exists to stop

`.github/workflows/probe-sweep.yml` restates three facts that live somewhere
else, and every one of them was, or still is, only a comment:

* `default_probe` must equal the `CLASS_PROBE` each backend actually ships.
  `ci/probe_sweep.py` uses that number three ways at once — as the "(shipped)"
  column label, as the baseline every gain is measured against, and as one half
  of the `min(shipped, candidate)` reach threshold. A stale value therefore
  publishes a table that is wrong in three places while looking exactly like a
  correct one, and the sweep exits 0. This was already half-caught once: the
  matrix carried `16`/`32` after the constants moved to `8`, and nothing
  noticed until a human read the diff.
* `isolate` pins the dispatcher to the tier the leg claims to measure, in two
  workflows at once — the sweep and `probe-timing` in `ci.yml`. If the SSE4.2
  leg of either lost `--cfg memspan_disable_avx2` it would time the AVX2 kernel
  and publish it as SSE4.2, under a green tick.

  This one is not checked by comparing the two copies. Comparing copies passes
  when both are wrong the same way, and an identical typo is the likely shape:
  `--cfg` flags are *defined* by being passed, so a misspelt
  `memspan_disable_avx22` disables nothing and the compiler says nothing —
  `check-cfg` lints the cfg names written in the source, not the ones handed to
  rustc. That was measured rather than assumed. So each `isolate` is instead
  derived from the dispatcher's own tier order in `src/utils.rs`, which prefers
  AVX-512 over AVX2 over SSE4.2: isolating a tier means disabling exactly the
  tiers above it, no more and no fewer, and every name used must be one
  `Cargo.toml` declares.
* `required_measurement` is what `require-measurements` reads to decide which
  legs are allowed to produce nothing. Hard-coding that list in the job would
  put the same fact in two places a third time.

# Why the sets have to be equal

Each check below compares two *sets*, not two overlapping lists. A subset test
passes when a tier is deleted, and a deleted tier is precisely the shape of
absence this repository keeps rediscovering — a sweep that covers two of three
backends renders a shorter table that nobody can tell from a complete one.

# What it refuses to guess

The first argument to `class_probe` may be a literal or a named constant, so a
symbolic argument is resolved against a `const NAME: usize = N;` in the same
file, and `class_probe`'s clamp to the chunk width is applied exactly as the
function applies it. Anything it cannot resolve — a missing constant, an
expression, two `CLASS_PROBE` definitions in one file — is an error rather than
a default. A parser that recognises one shape and silently accepts the rest is
the failure this script is supposed to prevent, not a smaller version of it.

Two properties carry that, and both were once missing:

* **Comments are removed before anything is parsed**, so only text the compiler
  reads can answer a question about what the crate compiles. Without that, a
  stale `// const CLASS_PROBE: usize = super::class_probe(8, CHUNK);` was a
  perfectly good match, and a backend whose real declaration the parser could
  not read reported the comment's width. That was measured, not imagined: the
  pair below parsed as `(8, 32)` while the compiler shipped 32.

      // const CLASS_PROBE: usize = super::class_probe(8, CHUNK);
      const CLASS_PROBE: usize = super::class_probe(CHUNK / 1, CHUNK);

* **A declaration is counted by its name, not by its shape.** The count and the
  parse ask different questions — "how many does this file declare" and "can
  this one be read" — so a declaration written in a shape the pattern misses is
  an unreadable declaration, reported as one, rather than an absence that some
  other text is free to stand in for.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tomllib
from collections.abc import Callable

try:
    import yaml
except ImportError:  # pragma: no cover - exercised only on a host without PyYAML
    # Reported by `matrix_include`, the only function that needs it, rather than
    # at import time. `ci/check_probe_width.py` imports the source parser below
    # and reads no YAML at all; killing that import here would make a check that
    # needs no dependency fail for want of one.
    yaml = None

SWEEP_WORKFLOW = ".github/workflows/probe-sweep.yml"
CI_WORKFLOW = ".github/workflows/ci.yml"
MANIFEST = "Cargo.toml"

# The x86 tiers in the order `src/utils.rs` and `src/skip/mod.rs` prefer them:
# the dispatcher takes the highest available, so isolating a tier means
# disabling every tier to its right.
TIER_ORDER = ("sse42", "avx2", "avx512")

# Where each of those tiers is generated. Derived from `TIER_ORDER` so the two
# cannot disagree: `required_isolate` indexes into that tuple, and a tier
# checked here but absent from it would raise instead of being checked.
#
# Not the reporter's `BACKEND_SOURCES`: `neon` and `simd128` are deliberately
# absent, because no leg of this workflow runs on aarch64 or wasm and a matrix
# entry for a tier that can never be measured would be worse than none.
X86_BACKENDS = {tier: f"src/skip/{tier}.rs" for tier in TIER_ORDER}

# Tiers whose hardware every GitHub-hosted x86 runner has, and which the
# pull-request timing report therefore must cover. AVX-512 is absent on purpose:
# hosted runners do not have it, which is why the sweep's AVX-512 leg has never
# produced a measurement.
TIMING_TIERS = {"sse42", "avx2"}

CLASS_PROBE_RE = re.compile(
    r"const\s+CLASS_PROBE\s*:\s*usize\s*=\s*super::class_probe\(\s*"
    r"([A-Za-z0-9_]+)\s*,\s*([A-Za-z0-9_]+)\s*\)"
)

# What counts as a declaration, and the statement it introduces. Both are
# deliberately looser than `CLASS_PROBE_RE`: the count must not fall when the
# declaration is written in a shape the parser cannot read, because a checker
# that skips what it cannot read is a checker holding nothing while reporting a
# width. `ci/check_probe_width.py` discovers backends with the same pattern, so
# "declares `CLASS_PROBE`" means one thing in this repository.
DECLARATION_RE = re.compile(r"\bconst\s+CLASS_PROBE\b")
STATEMENT_RE = re.compile(r"\bconst\s+CLASS_PROBE\b[^;]*;")

INT_RE = re.compile(r"\A(\d[\d_]*)(?:usize|u8|u16|u32|u64|usize)?\Z")

# Literal openers, for the scanner below. A raw string ends at a quote followed
# by its own number of hashes; a char literal is matched whole so that a
# lifetime (`&'a str`), which opens with the same character and never closes,
# is left alone.
RAW_STRING_RE = re.compile(r'(?:b|c)?r(#*)"')
STRING_RE = re.compile(r'(?:b|c)?"')
CHAR_RE = re.compile(r"b?'(?:\\(?:u\{[0-9a-fA-F_]{1,6}\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'")


def die(message: str) -> None:
    print(f"\n**probe-matrix check failed:** {message}\n")
    sys.exit(1)


# `read` and `probe_parts` are what `ci/check_probe_width.py` calls, so every
# function on that path takes the reporter to use. The default keeps this
# script's own prefix; the importer passes its own, because a message headed
# "probe-matrix check failed" from a job named `probe-width` sends the reader to
# the wrong file. Nothing else about the parse differs, which is the point:
# there is one `class_probe(...)` parser in this repository and both checks
# call it.
Fail = Callable[[str], None]


def read(repo_root: str, relative: str, fail: Fail = die) -> str:
    try:
        with open(os.path.join(repo_root, relative)) as handle:
            return handle.read()
    except OSError as err:
        fail(f"cannot read `{relative}`: {err}")
        raise  # unreachable, keeps type checkers happy


def blank(text: str) -> str:
    """`text` with every character but its line breaks replaced by a space."""
    return "".join("\n" if char == "\n" else " " for char in text)


def literal_end(source: str, index: int) -> int | None:
    """Where the string or char literal opening at `index` ends, or `None`."""
    if source[index] not in "\"'rbc":
        return None
    if index and (source[index - 1].isalnum() or source[index - 1] == "_"):
        # `r`, `b` and `c` are string prefixes only at the start of a token;
        # inside an identifier (`for`, `sub`) they are just letters.
        return None

    raw = RAW_STRING_RE.match(source, index)
    if raw:
        terminator = '"' + raw.group(1)
        end = source.find(terminator, raw.end())
        return len(source) if end < 0 else end + len(terminator)

    plain = STRING_RE.match(source, index)
    if plain:
        cursor = plain.end()
        while cursor < len(source):
            if source[cursor] == "\\":
                cursor += 2
                continue
            if source[cursor] == '"':
                return cursor + 1
            cursor += 1
        return len(source)

    char = CHAR_RE.match(source, index)
    return char.end() if char else None


def strip_comments(source: str, *, blank_strings: bool = False) -> str:
    """Blank every Rust comment, keeping the file's length and its line breaks.

    Only text the compiler reads can answer a question about what the crate
    compiles, so a commented-out declaration must not be one. The same rule
    already governs the `.cargo/config.toml` scan in `ci/check_probe_width.py`;
    applying it to Rust and not to TOML was an inconsistency a stale comment
    could be aimed at.

    Line breaks survive so that line numbers in the output still address the
    original file, and `blank_strings` blanks string and char literals as well.
    That is on for the source parse, where a declaration quoted inside a `&str`
    is no more a declaration than one inside a comment, and off for the
    `build.rs` scan, which is looking for a cfg name that lives in a string.
    """
    out: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        head = source[index : index + 2]
        if head == "//":
            end = source.find("\n", index)
            end = length if end < 0 else end
            out.append(blank(source[index:end]))
            index = end
            continue
        if head == "/*":
            # Rust nests block comments, so this counts rather than searching
            # for the first `*/`.
            depth, cursor = 1, index + 2
            while cursor < length and depth:
                if source.startswith("/*", cursor):
                    depth += 1
                    cursor += 2
                elif source.startswith("*/", cursor):
                    depth -= 1
                    cursor += 2
                else:
                    cursor += 1
            out.append(blank(source[index:cursor]))
            index = cursor
            continue

        end = literal_end(source, index)
        if end is not None:
            literal = source[index:end]
            out.append(blank(literal) if blank_strings else literal)
            index = end
            continue

        out.append(source[index])
        index += 1
    return "".join(out)


def const_value(source: str, name: str) -> int | None:
    """Resolve `const <name>: usize = <int>;` in one file, or `None`."""
    literal = INT_RE.match(name)
    if literal:
        return int(literal.group(1).replace("_", ""))

    pattern = re.compile(
        rf"const\s+{re.escape(name)}\s*:\s*usize\s*=\s*(\d[\d_]*)\s*(?:usize)?\s*;"
    )
    found = pattern.findall(source)
    if len(found) != 1:
        return None
    return int(found[0].replace("_", ""))


def shipped_probe(source: str, relative: str, fail: Fail = die) -> int:
    """The `CLASS_PROBE` this backend ships, clamp included."""
    default, chunk = probe_parts(source, relative, fail)

    # `skip::class_probe` clamps: `if probe > chunk { chunk } else { probe }`.
    # The matrix has to carry the width the crate ships, which is the clamped
    # one -- `avx512` writes `class_probe(CHUNK, CHUNK)` and ships 64.
    return min(default, chunk)


def probe_parts(source: str, relative: str, fail: Fail = die) -> tuple[int, int]:
    """`class_probe`'s two arguments, resolved: `(default, chunk)`.

    Split out from `shipped_probe` because `ci/check_probe_width.py` needs the
    chunk as well as the clamped result: it records "a full chunk" as an intent
    in its own right, and a backend that means to probe a whole vector must not
    be pinned to whatever integer that chunk happens to be today.
    """
    # Everything below reads `code`, never `source`: a commented-out declaration
    # is not a declaration, and a commented-out `const CHUNK` is not a chunk.
    code = strip_comments(source, blank_strings=True)

    # Counted by name and read by shape, in that order. A declaration the parser
    # cannot read must not lower this count: were it to, the file would look like
    # one that declares nothing, and any text that does match the shape -- a
    # stale comment, before they were stripped -- would answer for it.
    declarations = DECLARATION_RE.findall(code)
    if len(declarations) != 1:
        fail(
            f"`{relative}` has {len(declarations)} `const CLASS_PROBE` "
            "declarations outside comments, not 1. With none, the backend "
            "stopped declaring its probe where this script reads it and the "
            "width it ships is now checked against nothing; with more than one, "
            "there is no single shipped width to check against."
        )

    declared = DECLARATION_RE.search(code)
    statement = STATEMENT_RE.search(code)
    match = CLASS_PROBE_RE.search(statement.group(0)) if statement else None
    if match is None:
        # No statement at all means the declaration runs to the end of the file
        # without a `;`; quote what follows it so the message still shows the
        # reader what was rejected.
        start = declared.start()
        text = statement.group(0) if statement else code[start : start + 160]
        fail(
            f"cannot read `{relative}`'s `CLASS_PROBE` declaration:\n\n    "
            + " ".join(text.split())
            + "\n\nIt must read `const CLASS_PROBE: usize = "
            "super::class_probe(<width>, <chunk>);`, with each argument a "
            "literal or a bare constant name this script can resolve in the "
            "same file.\n\n"
            "An expression it cannot resolve is an error and never a skip. The "
            "alternative reads past what it cannot parse and reports a width it "
            "took from somewhere else -- a stale comment carrying the old shape "
            "is enough -- which is the same defect as a job that exits green "
            "having measured nothing. Widen the parser deliberately, or write "
            "the constant in the shape it reads."
        )

    default_arg, chunk_arg = match.groups()
    default = const_value(code, default_arg)
    chunk = const_value(code, chunk_arg)
    for label, arg, value in (
        ("default", default_arg, default),
        ("chunk", chunk_arg, chunk),
    ):
        if value is None:
            fail(
                f"cannot resolve the {label} argument `{arg}` of "
                f"`class_probe` in `{relative}` to an integer. It is neither a "
                "literal nor a uniquely-defined `const ...: usize = N;` in that "
                "file, so the shipped probe cannot be established and the "
                "matrix value cannot be checked. Resolve it by hand or teach "
                "this script the new shape -- do not leave the coupling "
                "unchecked."
            )

    return default, chunk


def declared_disable_cfgs(repo_root: str) -> set[str]:
    """The `memspan_disable_*` cfgs `Cargo.toml` tells rustc to expect."""
    try:
        with open(os.path.join(repo_root, MANIFEST), "rb") as handle:
            manifest = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as err:
        die(f"cannot read `{MANIFEST}`: {err}")

    entries = (
        manifest.get("lints", {})
        .get("rust", {})
        .get("unexpected_cfgs", {})
        .get("check-cfg", [])
    )
    names = {
        match.group(1)
        for entry in entries
        if (match := re.fullmatch(r"cfg\((memspan_disable_[a-z0-9_]+)\)", str(entry)))
    }
    if not names:
        die(
            f"`{MANIFEST}` declares no `cfg(memspan_disable_*)` under "
            "`lints.rust.unexpected_cfgs.check-cfg`. That list is the only "
            "record of which dispatcher-isolation flags exist, so without it "
            "an `isolate` string cannot be told from a typo."
        )
    return names


def required_isolate(tier: str) -> set[str]:
    """The cfgs that pin the dispatcher to `tier`: every tier above it."""
    return {
        f"memspan_disable_{higher}"
        for higher in TIER_ORDER[TIER_ORDER.index(tier) + 1 :]
    }


def parse_isolate(value: object, tier: str, relative: str) -> set[str]:
    """`--cfg a --cfg b` -> `{a, b}`, refusing anything else in the string."""
    if value is None:
        die(
            f"the `{tier}` leg in `{relative}` has no `isolate`. An absent "
            "value is not the same as an empty one: it usually means the key "
            "was renamed, and an unpinned leg measures whatever the runner's "
            "top tier happens to be."
        )
    tokens = str(value).split()
    names: set[str] = set()
    index = 0
    while index < len(tokens):
        if tokens[index] != "--cfg" or index + 1 >= len(tokens):
            die(
                f"the `{tier}` leg in `{relative}` has an `isolate` this script "
                f"cannot read: `{value}`. It must be a sequence of `--cfg NAME` "
                "pairs and nothing else, so that what it disables is decidable "
                "rather than guessed."
            )
        names.add(tokens[index + 1])
        index += 2
    return names


def check_isolation(
    entries: list[dict], relative: str, declared: set[str], label: str
) -> None:
    """Hold each leg's `isolate` against the dispatcher's tier order."""
    for entry in entries:
        tier = entry["tier"]
        used = parse_isolate(entry.get("isolate"), tier, relative)

        unknown = sorted(used - declared)
        if unknown:
            die(
                f"the `{tier}` leg of {label} in `{relative}` passes "
                + ", ".join(f"`--cfg {name}`" for name in unknown)
                + f", which `{MANIFEST}` does not declare.\n\n"
                "Nothing else would catch this: a `--cfg` flag is defined by "
                "being passed, so a misspelt name disables nothing, the "
                "compiler stays silent, and the leg times a tier above the one "
                "it reports."
            )

        wanted = required_isolate(tier)
        if used != wanted:
            die(
                f"the `{tier}` leg of {label} in `{relative}` disables "
                + (", ".join(f"`{n}`" for n in sorted(used)) or "nothing")
                + ", but pinning the dispatcher to `"
                + tier
                + "` means disabling exactly "
                + (", ".join(f"`{n}`" for n in sorted(wanted)) or "nothing")
                + ".\n\nThe dispatcher takes the highest available tier "
                f"({' < '.join(TIER_ORDER)}), so a missing flag measures a "
                "wider backend under this tier's name and an extra one "
                "measures a narrower backend under it."
            )


def matrix_include(workflow_text: str, relative: str, job: str) -> list[dict]:
    if yaml is None:
        die(
            "PyYAML is not importable, so the workflow matrix cannot be read. "
            "Install it (`python3 -m pip install pyyaml`) rather than skipping "
            "this check: the whole point of the script is that a coupling "
            "nobody verifies goes stale silently."
        )
    try:
        doc = yaml.safe_load(workflow_text)
    except yaml.YAMLError as err:
        die(f"`{relative}` is not valid YAML: {err}")
    try:
        include = doc["jobs"][job]["strategy"]["matrix"]["include"]
    except (KeyError, TypeError):
        die(
            f"`{relative}` has no `jobs.{job}.strategy.matrix.include`. Either "
            "the job was renamed or its matrix was restructured; this script "
            "reads that path and would otherwise check nothing."
        )
    if not isinstance(include, list) or not include:
        die(f"`jobs.{job}.strategy.matrix.include` in `{relative}` is empty.")
    for entry in include:
        if not isinstance(entry, dict) or "tier" not in entry:
            die(
                f"an entry of `jobs.{job}.strategy.matrix.include` in "
                f"`{relative}` has no `tier` key: {entry!r}"
            )
    return include


def check(repo_root: str) -> list[str]:
    """Every coupling, checked. Returns the required-measurement tiers."""
    sweep_text = read(repo_root, SWEEP_WORKFLOW)
    sweep = matrix_include(sweep_text, SWEEP_WORKFLOW, "sweep")

    tiers = [entry["tier"] for entry in sweep]
    duplicates = sorted({t for t in tiers if tiers.count(t) > 1})
    if duplicates:
        die(
            "the sweep matrix lists "
            + ", ".join(f"`{t}`" for t in duplicates)
            + " more than once, so one entry's `default_probe` would be checked "
            "and the other's would not."
        )

    unswept = sorted(set(X86_BACKENDS) - set(tiers))
    strangers = sorted(set(tiers) - set(X86_BACKENDS))
    if unswept:
        die(
            "the sweep matrix does not cover "
            + ", ".join(f"`{t}`" for t in unswept)
            + ". The matrix and the x86 backends must be the same set: a tier "
            "dropped from the matrix renders a shorter table that reads exactly "
            "like a complete one. Add the leg back, or delete the backend."
        )
    if strangers:
        die(
            "the sweep matrix has "
            + ", ".join(f"`{t}`" for t in strangers)
            + ", which is not an x86 backend this script knows how to check. "
            "Its `default_probe` is therefore held against nothing."
        )

    required: list[str] = []
    for entry in sweep:
        tier = entry["tier"]
        relative = X86_BACKENDS[tier]

        if "default_probe" not in entry:
            die(f"the sweep matrix entry for `{tier}` has no `default_probe`.")
        declared = entry["default_probe"]
        if not isinstance(declared, int) or isinstance(declared, bool):
            die(
                f"`default_probe` for `{tier}` is {declared!r}, not an integer. "
                "The reporter parses it as one."
            )

        actual = shipped_probe(read(repo_root, relative), relative)
        if declared != actual:
            die(
                f"the sweep matrix says `{tier}` ships `default_probe: "
                f"{declared}`, but `{relative}` ships `CLASS_PROBE == {actual}`.\n\n"
                "`ci/probe_sweep.py` uses that number as the `(shipped)` column "
                "label, as the baseline every gain is measured against, and as "
                "one half of the `min(shipped, candidate)` reach threshold, so "
                "leaving it stale publishes a table that is wrong in three "
                "places and still exits 0. Update the matrix, or the constant."
            )

        if "probes" not in entry:
            die(f"the sweep matrix entry for `{tier}` has no `probes`.")
        try:
            probes = [int(p) for p in str(entry["probes"]).split(",")]
        except ValueError:
            die(
                f"`probes` for `{tier}` is {entry['probes']!r}, which is not a "
                "comma-separated list of integers."
            )
        if declared not in probes:
            die(
                f"`default_probe: {declared}` for `{tier}` is not a member of "
                f"`probes: \"{entry['probes']}\"`. The reporter looks up the "
                "shipped width in the measured set to find its baseline, so it "
                "would fail on a missing cell after paying for the whole sweep."
            )

        if "required_measurement" not in entry:
            die(
                f"the sweep matrix entry for `{tier}` has no "
                "`required_measurement`. `require-measurements` reads that flag "
                "to decide which legs are allowed to produce nothing, and a "
                "missing flag would quietly excuse the leg."
            )
        flag = entry["required_measurement"]
        if not isinstance(flag, bool):
            die(
                f"`required_measurement` for `{tier}` is {flag!r}, not a boolean. "
                "A string `\"false\"` is truthy in YAML consumers that test it "
                "loosely, which is the wrong direction to be wrong in."
            )
        if flag:
            required.append(tier)

    if not required:
        die(
            "no sweep tier has `required_measurement: true`. Every leg would "
            "then be allowed to measure nothing, which is the state this flag "
            "was added to end."
        )

    # `probe-timing` in `ci.yml` pins the dispatcher for the same tiers. Both
    # matrices are held against the dispatcher's tier order rather than against
    # each other, so a flag that is wrong the same way in both still fails.
    ci_text = read(repo_root, CI_WORKFLOW)
    timing = matrix_include(ci_text, CI_WORKFLOW, "probe-timing")
    timing_tiers = {entry["tier"] for entry in timing}
    if timing_tiers != TIMING_TIERS:
        die(
            f"`probe-timing` in `{CI_WORKFLOW}` covers "
            + (", ".join(f"`{t}`" for t in sorted(timing_tiers)) or "nothing")
            + ", but every hosted x86 runner has "
            + ", ".join(f"`{t}`" for t in sorted(TIMING_TIERS))
            + " and the report must cover exactly those. A tier dropped from it "
            "is a tier whose shipped width goes unreported; the width itself is "
            "gated by `ci/check_probe_width.py`, which needs no runner."
        )

    declared = declared_disable_cfgs(repo_root)
    check_isolation(sweep, SWEEP_WORKFLOW, declared, "the sweep")
    check_isolation(timing, CI_WORKFLOW, declared, "`probe-timing`")

    return required


# Isolation strings the derived rule accepts, spelled out once so the selftest
# cases below differ from the shipped tree only in the thing each is testing.
ISO_SSE42 = "--cfg memspan_disable_avx512 --cfg memspan_disable_avx2"
ISO_AVX2 = "--cfg memspan_disable_avx512"
ISO_AVX512 = '""'

GOOD_MANIFEST = """\
[package]
name = "memspan"

[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = [
  'cfg(memspan_force_scalar)',
  'cfg(memspan_disable_avx512)',
  'cfg(memspan_disable_avx2)',
  'cfg(memspan_disable_sse42)',
  'cfg(memspan_disable_simd128)',
] }
"""


def selftest() -> int:
    """Plant every drift this script claims to catch, and require each to fail."""
    import tempfile

    def workflow(entries: str, timing: str | None = None) -> tuple[str, str]:
        sweep_yaml = (
            "name: probe-sweep\n"
            "on:\n  workflow_dispatch:\n"
            "jobs:\n"
            "  sweep:\n"
            "    runs-on: ubuntu-latest\n"
            "    strategy:\n"
            "      matrix:\n"
            "        include:\n" + entries
        )
        timing_entries = timing if timing is not None else (
            f"          - tier: sse42\n            isolate: {ISO_SSE42}\n"
            f"          - tier: avx2\n            isolate: {ISO_AVX2}\n"
        )
        ci_yaml = (
            "name: CI\n"
            "on:\n  pull_request:\n"
            "jobs:\n"
            "  probe-timing:\n"
            "    runs-on: ubuntu-latest\n"
            "    strategy:\n"
            "      matrix:\n"
            "        include:\n" + timing_entries
        )
        return sweep_yaml, ci_yaml

    def entry(tier: str, probe: object, probes: str, isolate: str, req: str) -> str:
        return (
            f"          - tier: {tier}\n"
            f"            default_probe: {probe}\n"
            f'            probes: "{probes}"\n'
            f"            required_measurement: {req}\n"
            f"            isolate: {isolate}\n"
        )

    good_matrix = (
        entry("sse42", 8, "16,8,4", ISO_SSE42, "true")
        + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
        + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false")
    )
    good_sources = {
        "src/skip/sse42.rs": (
            "const CHUNK: usize = 16;\n"
            "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
        ),
        "src/skip/avx2.rs": (
            "const CHUNK: usize = 32;\n"
            "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
        ),
        # The symbolic shape: both arguments are the same named constant.
        "src/skip/avx512.rs": (
            "const CHUNK: usize = 64;\n"
            "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
        ),
    }

    cases: list[tuple[str, str, str | None, dict[str, str], bool, str]] = [
        (
            "the shipped tree agrees with itself",
            good_matrix, None, good_sources, True, "",
        ),
        (
            "matrix left stale after the constant narrowed",
            entry("sse42", 16, "16,8,4", ISO_SSE42, "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "`sse42` ships `default_probe: 16`",
        ),
        (
            "constant changed under an up-to-date-looking matrix",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(16, CHUNK);\n"
                )
            },
            False, "`CLASS_PROBE == 16`",
        ),
        (
            "clamp ignored: default above chunk still ships chunk",
            good_matrix, None,
            good_sources | {
                "src/skip/sse42.rs": (
                    "const CHUNK: usize = 16;\n"
                    "const CLASS_PROBE: usize = super::class_probe(64, CHUNK);\n"
                )
            },
            False, "`CLASS_PROBE == 16`",
        ),
        (
            "shipped width absent from the swept set",
            entry("sse42", 8, "16,4", ISO_SSE42, "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "is not a member of",
        ),
        (
            "a tier dropped from the sweep",
            entry("sse42", 8, "16,8,4", ISO_SSE42, "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true"),
            None, good_sources, False, "does not cover `avx512`",
        ),
        (
            "a tier the script cannot check added to the sweep",
            good_matrix + entry("neon", 8, "16,8", ISO_AVX512, "true"),
            None, good_sources, False, "not an x86 backend",
        ),
        (
            "the backend stopped declaring CLASS_PROBE where this reads it",
            good_matrix, None,
            good_sources | {"src/skip/avx2.rs": "const CHUNK: usize = 32;\n"},
            False, "has 0 `const CLASS_PROBE",
        ),
        (
            "two CLASS_PROBE definitions in one file",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                    "const CLASS_PROBE: usize = super::class_probe(4, CHUNK);\n"
                )
            },
            False, "has 2 `const CLASS_PROBE",
        ),
        (
            "a symbolic argument that resolves to nothing",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "const CLASS_PROBE: usize = super::class_probe(WIDTH, CHUNK);\n"
                )
            },
            False, "cannot resolve",
        ),
        # The evasion this parser was foolable by: the shape it recognises is
        # present only in a comment, and the declaration that actually compiles
        # is written so the pattern misses it. Reading raw text, the parser found
        # exactly one match, read `8` out of the comment, and passed a widened
        # backend.
        (
            "a commented-out declaration standing in for an unreadable real one",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "// const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(CHUNK / 1, CHUNK);\n"
                )
            },
            False, "cannot read `src/skip/avx2.rs`'s `CLASS_PROBE` declaration",
        ),
        (
            "the only declaration commented out",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "const CHUNK: usize = 32;\n"
                    "// const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                )
            },
            False, "has 0 `const CLASS_PROBE",
        ),
        # The other direction: removing comments must not invent a second
        # declaration out of one a doc comment quotes, or the fix would fail a
        # tree that is correct.
        (
            "a doc comment quoting the older, wider declaration",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "/// Was `const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);`\n"
                    "/// until the sweep chose 8.\n"
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                )
            },
            True, "",
        ),
        (
            "a block comment quoting it, and a chunk a comment disagrees with",
            good_matrix, None,
            good_sources | {
                "src/skip/avx2.rs": (
                    "/*\n"
                    "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
                    "const CHUNK: usize = 64;\n"
                    "*/\n"
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                )
            },
            True, "",
        ),
        (
            "required_measurement missing",
            "          - tier: sse42\n"
            "            default_probe: 8\n"
            '            probes: "16,8,4"\n'
            "            isolate: --cfg memspan_disable_avx512 --cfg memspan_disable_avx2\n"
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "has no `required_measurement`",
        ),
        (
            "required_measurement written as a string",
            entry("sse42", 8, "16,8,4", ISO_SSE42, '"false"')
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "not a boolean",
        ),
        (
            "every leg excused from measuring",
            entry("sse42", 8, "16,8,4", ISO_SSE42, "false")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "false")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "no sweep tier has",
        ),
        (
            "the timing report's SSE4.2 leg stopped disabling AVX2",
            good_matrix,
            f"          - tier: sse42\n            isolate: {ISO_AVX2}\n"
            f"          - tier: avx2\n            isolate: {ISO_AVX2}\n",
            good_sources, False, "means disabling exactly",
        ),
        (
            "the sweep's AVX2 leg stopped disabling AVX-512",
            entry("sse42", 8, "16,8,4", ISO_SSE42, "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX512, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "means disabling exactly",
        ),
        (
            "over-pinning: AVX2 also disabling the tier below it",
            entry("sse42", 8, "16,8,4", ISO_SSE42, "true")
            + entry("avx2", 8, "32,16,8,4",
                    "--cfg memspan_disable_avx512 --cfg memspan_disable_sse42", "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "means disabling exactly",
        ),
        # The case a copy-to-copy comparison cannot reach: both files wrong the
        # same way. rustc accepts an unknown `--cfg` silently, so nothing but
        # this catches it.
        (
            "the same typo in both workflows",
            entry("sse42", 8, "16,8,4",
                  "--cfg memspan_disable_avx512 --cfg memspan_disable_avx22", "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            "          - tier: sse42\n"
            "            isolate: --cfg memspan_disable_avx512 --cfg memspan_disable_avx22\n"
            f"          - tier: avx2\n            isolate: {ISO_AVX2}\n",
            good_sources, False, "does not declare",
        ),
        (
            "an isolate that is not `--cfg NAME` pairs",
            entry("sse42", 8, "16,8,4", "-C target-cpu=native", "true")
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "cannot read",
        ),
        (
            "the isolate key dropped altogether",
            "          - tier: sse42\n"
            "            default_probe: 8\n"
            '            probes: "16,8,4"\n'
            "            required_measurement: true\n"
            + entry("avx2", 8, "32,16,8,4", ISO_AVX2, "true")
            + entry("avx512", 64, "64,32,16,8", ISO_AVX512, "false"),
            None, good_sources, False, "has no `isolate`",
        ),
        (
            "a tier dropped from the timing report",
            good_matrix,
            f"          - tier: avx2\n            isolate: {ISO_AVX2}\n",
            good_sources, False, "must cover exactly those",
        ),
        (
            "the manifest stopped declaring the isolation cfgs",
            good_matrix, None,
            good_sources | {MANIFEST: '[package]\nname = "memspan"\n'},
            False, "declares no `cfg(memspan_disable_",
        ),
    ]

    failures: list[str] = []
    for label, entries, timing, sources, expect_ok, needle in cases:
        with tempfile.TemporaryDirectory() as root:
            sweep_yaml, ci_yaml = workflow(entries, timing)
            os.makedirs(os.path.join(root, ".github", "workflows"))
            os.makedirs(os.path.join(root, "src", "skip"))
            with open(os.path.join(root, SWEEP_WORKFLOW), "w") as handle:
                handle.write(sweep_yaml)
            with open(os.path.join(root, CI_WORKFLOW), "w") as handle:
                handle.write(ci_yaml)
            # Written first so a case can override it by listing it in `sources`.
            with open(os.path.join(root, MANIFEST), "w") as handle:
                handle.write(GOOD_MANIFEST)
            for relative, text in sources.items():
                with open(os.path.join(root, relative), "w") as handle:
                    handle.write(text)

            ok, output = run_isolated(root)

        if ok != expect_ok:
            failures.append(
                f"{label}: expected {'pass' if expect_ok else 'failure'}, "
                f"got {'pass' if ok else 'failure'}\n{indent(output)}"
            )
        elif not expect_ok and needle not in output:
            failures.append(
                f"{label}: failed for the wrong reason -- {needle!r} not in "
                f"the message\n{indent(output)}"
            )
        else:
            print(f"  {'passes ' if expect_ok else 'caught '} {label}")

    if failures:
        print(f"\n{len(failures)} selftest case(s) wrong:\n")
        for failure in failures:
            print(f"* {failure}")
        return 1

    print(f"\nall {len(cases)} selftest cases behave as claimed")
    return 0


def indent(text: str) -> str:
    return "\n".join(f"      {line}" for line in text.strip().splitlines())


def run_isolated(root: str) -> tuple[bool, str]:
    """Run `check` in a subprocess so its `sys.exit` does not end the selftest."""
    import subprocess

    proc = subprocess.run(
        [sys.executable, os.path.abspath(__file__), "--repo-root", root],
        capture_output=True,
        text=True,
    )
    return proc.returncode == 0, proc.stdout + proc.stderr


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument(
        "--print-required-tiers",
        action="store_true",
        help="after checking, print the tiers that must produce a measurement",
    )
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    required = check(args.repo_root)

    if args.print_required_tiers:
        for tier in required:
            print(tier)
        return 0

    print(
        "probe-sweep matrix agrees with the shipped constants, the swept sets, "
        "and the timing report's isolation flags; tiers that must measure: "
        + ", ".join(required)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
