#!/usr/bin/env python3
"""Fail when a backend's shipped `CLASS_PROBE` is not the width it means to ship.

Run with `python3 ci/check_probe_width.py` from `.github/workflows/ci.yml`'s
`probe-width` job on every pull request; `--selftest` proves it bites.

# The claim, and why source is the exact instrument for it

One claim: **every SIMD backend still ships the probe width recorded below, and
the narrowing on SSE4.2, AVX2 and NEON has not been reverted.**

`CLASS_PROBE` is a compile-time constant per backend. Whether it has been
widened is therefore a property of the source, decidable by reading it, and
nothing about a CPU, a runner or a benchmark enters into deciding it. An earlier
attempt guarded this by timing the shipped width on a hosted runner and failing
outside a band; that is a noisy proxy for something that can simply be read, and
its verdict moved with the hardware GitHub happened to allocate. The timing
survives as evidence -- `ci/report_probe_timing.py`, which reports and never
fails on a ratio -- and this check is what decides.

# Why this is not the constant checked against itself

A bare `assert CLASS_PROBE == 8` inside the backend would be tautological: the
same edit that widens the constant satisfies the assertion again. The table
below is not in the backend. It is a second, independently-editable record of
the width each backend intends, held against the constant the backend actually
declares -- the same shape as `default_probe` in the sweep matrix, which
`ci/check_probe_matrix.py` catches drifting for exactly this reason, and which
had already gone stale once.

So a revert fails here. Making it pass again means editing this table, in the
same commit, to say that the crate now intends a chunk-width probe -- a claim a
reviewer sees and can weigh against the measurements in the backend's own doc
comment. That is the whole difference between a constant protected by a commit
message and one protected by a check.

# What it refuses to guess

The backends are discovered, not listed: every `src/skip/*.rs` that declares a
`const CLASS_PROBE` must appear in the table and every table entry must be a
file that declares one. A subset test would pass on a backend added without an
intended width, which is a backend whose probe nothing holds; and it would pass
on a table entry whose file was deleted, which is a check quietly holding
nothing.

The parse is `ci/check_probe_matrix.py`'s, imported rather than rewritten. It
strips comments before reading anything, resolves a symbolic argument against a
`const NAME: usize = N;` in the same file, applies `class_probe`'s clamp exactly
as the function applies it, and treats anything it cannot resolve as an error --
including a declaration written in a shape it cannot read, which is reported as
unreadable rather than counted as absent.

# What a source check cannot see, and why timing could not see it either

`--cfg memspan_class_probe="N"` overrides the default at build time. Two of the
three places that could set it for *every* build of this repository --
`build.rs`, and `rustflags` in `.cargo/config.toml` -- are checked below,
because either would change the shipped width with the source still reading `8`.

Both refusals are blanket: after comments are stripped, *any* mention of the cfg
name in either file fails. That is deliberately wider than "a line that emits
it". `build.rs` prints its cfgs through a `use_feature(feature)` helper, so the
line carrying `rustc-cfg` carries no cfg name and the line carrying the cfg name
says nothing about `rustc-cfg`; a rule wanting both on one line saw neither, and
the next helper would break any rule written by following this one. Neither file
has a legitimate reason to name this cfg -- `Cargo.toml` is where it is declared
to `check-cfg`, and `src/skip/mod.rs` is where it is documented and read.

The third is an ad-hoc `RUSTFLAGS` on one build. That is out of reach here, and
it is out of reach of any check: it is the documented measurement hook, the
probe sweep uses it deliberately on every leg, and it changes one build rather
than what the crate ships. The claim is about the width this repository ships by
default, and for that claim this file is exact. Timing on a runner would not
have covered the gap either -- it measures whatever RUSTFLAGS built it.

# It reports a breach loudly; it does not stop a merge

This repository has no branch protection rule and no ruleset, so a failure here
turns the check red on the pull request and a maintainer can still merge or push
directly past it. Making it required is a repository setting, not something a
workflow can assert about itself.
"""

from __future__ import annotations

import argparse
import os
import sys

import check_probe_matrix

# The intent of a backend that deliberately scans a whole vector chunk. Written
# as a sentinel rather than as `64`/`16`, so "a full chunk" and "a width that
# happens to equal the chunk today" are different entries. It also shapes what a
# revert has to look like: widening SSE4.2 back means replacing `8` with `CHUNK`
# here, not editing one integer into another.
CHUNK = "chunk"

# The width each backend intends to ship, and the only copy of it outside the
# backends themselves.
#
# | backend    | intended | chunk | why                                       |
# |------------|----------|-------|-------------------------------------------|
# | `sse42`    | 8        | 16    | narrowed; measured on an EPYC 7763        |
# | `avx2`     | 8        | 32    | narrowed; measured on an EPYC 7763        |
# | `neon`     | 8        | 16    | narrowed; measured on aarch64             |
# | `avx512`   | chunk    | 64    | never measured -- no hosted runner has it |
# | `simd128`  | chunk    | 16    | never measured -- no workflow times wasm  |
#
# Each backend's `CLASS_PROBE` doc comment carries the sweep table that selects
# its width, including the classes the narrowing did not fix. The two chunk-width
# entries are deliberate abstentions: an untuned constant is worth more than one
# tuned against no measurement.
INTENDED: dict[str, int | str] = {
    "sse42": 8,
    "avx2": 8,
    "neon": 8,
    "avx512": CHUNK,
    "simd128": CHUNK,
}

BACKEND_DIR = "src/skip"
BUILD_SCRIPT = "build.rs"

# Both spellings cargo accepts, so a rename does not slip the check.
CARGO_CONFIGS = (".cargo/config.toml", ".cargo/config")

# The override cfg. `Cargo.toml` declares it under `check-cfg` and
# `src/skip/mod.rs` documents it; neither sets it, and neither is scanned.
OVERRIDE_CFG = "memspan_class_probe"

# A file is a backend if it declares the constant at all, comments excluded.
# This is the parser's own pattern rather than a second one, so "declares
# `CLASS_PROBE`" means one thing here and in `ci/check_probe_matrix.py`, and it
# is deliberately looser than that script's `class_probe(...)` shape: a file
# that declares `CLASS_PROBE` in a shape the parser cannot read must reach the
# parser and be refused there, not vanish from the discovered set and be
# silently unchecked.
DECLARES_RE = check_probe_matrix.DECLARATION_RE


def die(message: str) -> None:
    print(f"\n**probe-width check failed:** {message}\n")
    sys.exit(1)


def discovered(repo_root: str) -> dict[str, str]:
    """`{backend: relative path}` for every `src/skip/*.rs` declaring the constant."""
    directory = os.path.join(repo_root, BACKEND_DIR)
    try:
        names = sorted(os.listdir(directory))
    except OSError as err:
        die(f"cannot list `{BACKEND_DIR}`: {err}")

    found: dict[str, str] = {}
    for name in names:
        if not name.endswith(".rs"):
            continue
        relative = f"{BACKEND_DIR}/{name}"
        source = check_probe_matrix.read(repo_root, relative, die)
        code = check_probe_matrix.strip_comments(source, blank_strings=True)
        if DECLARES_RE.search(code):
            found[name[: -len(".rs")]] = relative

    if not found:
        die(
            f"no file in `{BACKEND_DIR}` declares a `const CLASS_PROBE`, so this "
            "check would hold nothing against anything. Either the backends moved "
            "and this script is now stale, or the constant is gone -- both are "
            "findings, neither is a pass."
        )
    return found


def check_no_ambient_override(repo_root: str) -> list[str]:
    """Refuse anything that sets the override cfg for every build. Returns what it read."""
    scanned: list[str] = []

    build_script = os.path.join(repo_root, BUILD_SCRIPT)
    if os.path.exists(build_script):
        scanned.append(BUILD_SCRIPT)
        source = check_probe_matrix.read(repo_root, BUILD_SCRIPT, die)
        # Comments go, strings stay: the cfg name a build script emits lives
        # inside a string literal, which is the only thing this has to see.
        code = check_probe_matrix.strip_comments(source)
        original = source.splitlines()
        named = [
            (number, original[number - 1].strip())
            for number, line in enumerate(code.splitlines(), 1)
            if OVERRIDE_CFG in line
        ]
        if named:
            die(
                f"`{BUILD_SCRIPT}` names `{OVERRIDE_CFG}`:\n\n"
                + "\n".join(f"    {number}: {text}" for number, text in named)
                + "\n\nA build script cfg applies to every build of this crate, "
                "including a consumer's, so the shipped probe would be that "
                "width while every backend's source still reads its own. That "
                "is the one way the constants below can be true and the shipped "
                "width still wrong.\n\n"
                "The refusal is the bare name anywhere outside a comment, not a "
                f"`rustc-cfg` line that also carries it. `{BUILD_SCRIPT}` "
                "already routes its emission through `use_feature(feature)`, "
                "whose printing line carries `rustc-cfg` and no cfg name at "
                "all, so a rule wanting both on one line would pass "
                f'`use_feature("{OVERRIDE_CFG}=\\"32\\"")` while it overrode '
                "every build. Following each helper is an enumeration the next "
                "helper breaks.\n\n"
                f"Nothing in `{BUILD_SCRIPT}` has a reason to name that cfg. "
                "The one shape that could argue for it, a "
                "`cargo:rustc-check-cfg` declaration, is refused too: "
                "`Cargo.toml` already declares this cfg under "
                "`lints.rust.unexpected_cfgs.check-cfg`, and telling a "
                "declaration from an emission means parsing the emission again, "
                "which is the thing that failed. If a real reason appears, "
                "widen this deliberately and say what it is."
            )

    for relative in CARGO_CONFIGS:
        if not os.path.exists(os.path.join(repo_root, relative)):
            continue
        scanned.append(relative)
        source = check_probe_matrix.read(repo_root, relative, die)
        for line in source.splitlines():
            code = line.split("#", 1)[0]
            if OVERRIDE_CFG in code:
                die(
                    f"`{relative}` sets `{OVERRIDE_CFG}`:\n\n    {line.strip()}\n\n"
                    "Cargo applies that file's rustflags to every build in this "
                    "workspace, so the width the backends declare is not the "
                    "width they compile to."
                )

    return scanned


def check(repo_root: str) -> int:
    backends = discovered(repo_root)

    untabled = sorted(set(backends) - set(INTENDED))
    if untabled:
        die(
            "no intended width is recorded for "
            + ", ".join(f"`{name}`" for name in untabled)
            + ". A backend that declares `CLASS_PROBE` and is absent from the "
            "table in this file has its probe held against nothing, which is the "
            "state this check exists to end. Add it, with the measurement that "
            "chose its width or an explicit abstention."
        )

    missing = sorted(set(INTENDED) - set(backends))
    if missing:
        die(
            "the table in this file records a width for "
            + ", ".join(f"`{name}`" for name in missing)
            + ", but no such backend declares `CLASS_PROBE` under "
            f"`{BACKEND_DIR}`. An entry with no file behind it is a check "
            "quietly holding nothing; delete it, or restore the backend."
        )

    scanned = check_no_ambient_override(repo_root)

    rows: list[tuple[str, int, int, str]] = []
    for name in sorted(backends):
        relative = backends[name]
        source = check_probe_matrix.read(repo_root, relative, die)
        default, chunk = check_probe_matrix.probe_parts(source, relative, die)
        shipped = min(default, chunk)

        intent = INTENDED[name]
        if intent == CHUNK:
            expected, label = chunk, f"chunk ({chunk})"
        else:
            expected, label = int(intent), str(intent)
            if expected == chunk:
                die(
                    f"the table in this file records `{name}` as shipping "
                    f"{expected}, which is its whole {chunk}-byte chunk. A "
                    "chunk-width probe is written `CHUNK` here, so that "
                    "abstaining from a narrowing is a visible edit rather than "
                    "an integer that quietly grew into the chunk."
                )
            if expected > chunk:
                die(
                    f"the table in this file records `{name}` as shipping "
                    f"{expected}, which is wider than its {chunk}-byte chunk. "
                    "`class_probe` would clamp it, so the recorded intent could "
                    "never be the shipped width."
                )

        if shipped != expected:
            die(
                f"`{relative}` ships `CLASS_PROBE == {shipped}`, but this crate "
                f"intends {label} for `{name}`.\n\n"
                "No test in this repository fails when that constant moves: the "
                "differential suite in `tests/short_run_differential.rs` is a "
                "correctness oracle and passes at every width by design. The "
                "width is a performance decision, and the measurements that made "
                f"it are in `{relative}`'s `CLASS_PROBE` doc comment.\n\n"
                "If the change is intended, change the table in "
                "`ci/check_probe_width.py` in the same commit and say why there."
            )
        rows.append((name, shipped, chunk, label))

    print("### shipped ASCII-class probe widths\n")
    print("| backend | `CLASS_PROBE` | chunk | intended | |")
    print("|---|---|---|---|---|")
    for name, shipped, chunk, label in rows:
        narrow = "narrowed" if shipped < chunk else "full chunk"
        print(f"| `{name}` | {shipped} | {chunk} | {label} | {narrow} |")
    print(
        f"\nEvery backend under `{BACKEND_DIR}` declaring `CLASS_PROBE` ships the "
        "width this crate intends, and "
        + (
            "nothing in " + ", ".join(f"`{s}`" for s in scanned) + " overrides it"
            if scanned
            else "there is no build script or cargo config to override it"
        )
        + ". This is a source-level fact and says nothing about how fast any of "
        "them is; see `ci/report_probe_timing.py` for that."
    )
    return 0


# ── selftest ─────────────────────────────────────────────────────────────────

GOOD_SOURCES = {
    "src/skip/sse42.rs": (
        "const CHUNK: usize = 16;\n"
        "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
    ),
    "src/skip/avx2.rs": (
        "const CHUNK: usize = 32;\n"
        "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
    ),
    "src/skip/neon.rs": (
        "const NEON_CHUNK_SIZE: usize = 16;\n"
        "const CLASS_PROBE: usize = super::class_probe(8, NEON_CHUNK_SIZE);\n"
    ),
    # The symbolic shape: both arguments are the same named constant.
    "src/skip/avx512.rs": (
        "const CHUNK: usize = 64;\n"
        "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
    ),
    "src/skip/simd128.rs": (
        "const CHUNK: usize = 16;\n"
        "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
    ),
    # No `CLASS_PROBE`: discovery must not turn it into a backend.
    "src/skip/mod.rs": "pub(crate) const fn class_probe(d: usize, c: usize) -> usize {}\n",
    "build.rs": 'fn main() { println!("cargo:rerun-if-changed=build.rs"); }\n',
}


def selftest() -> int:
    """Plant every drift this check claims to catch, and require each to fail."""
    import subprocess
    import tempfile

    def run(sources: dict[str, str]) -> tuple[bool, str]:
        with tempfile.TemporaryDirectory() as root:
            for relative, text in sources.items():
                path = os.path.join(root, relative)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w") as handle:
                    handle.write(text)
            proc = subprocess.run(
                [sys.executable, os.path.abspath(__file__), "--repo-root", root],
                capture_output=True,
                text=True,
            )
        return proc.returncode == 0, proc.stdout + proc.stderr

    # The revert this whole check exists for, in the two shapes it can take: back
    # to the chunk symbolically, and back to a wider literal.
    reverted_to_chunk = GOOD_SOURCES | {
        "src/skip/avx2.rs": (
            "const CHUNK: usize = 32;\n"
            "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
        )
    }
    reverted_to_literal = GOOD_SOURCES | {
        "src/skip/sse42.rs": (
            "const CHUNK: usize = 16;\n"
            "const CLASS_PROBE: usize = super::class_probe(16, CHUNK);\n"
        )
    }

    cases: list[tuple[str, dict[str, str], bool, str]] = [
        ("the shipped tree agrees with its recorded widths", GOOD_SOURCES, True, ""),
        (
            "AVX2 reverted to a whole chunk symbolically",
            reverted_to_chunk,
            False,
            "`src/skip/avx2.rs` ships `CLASS_PROBE == 32`",
        ),
        (
            "SSE4.2 reverted to a chunk-width literal",
            reverted_to_literal,
            False,
            "`src/skip/sse42.rs` ships `CLASS_PROBE == 16`",
        ),
        (
            "NEON reverted, on a backend no x86 timing leg could ever cover",
            GOOD_SOURCES
            | {
                "src/skip/neon.rs": (
                    "const NEON_CHUNK_SIZE: usize = 16;\n"
                    "const CLASS_PROBE: usize = "
                    "super::class_probe(NEON_CHUNK_SIZE, NEON_CHUNK_SIZE);\n"
                )
            },
            False,
            "`src/skip/neon.rs` ships `CLASS_PROBE == 16`",
        ),
        (
            "narrowed further than intended, which is also unrecorded",
            GOOD_SOURCES
            | {
                "src/skip/avx2.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(4, CHUNK);\n"
                )
            },
            False,
            "`src/skip/avx2.rs` ships `CLASS_PROBE == 4`",
        ),
        (
            "a chunk-width backend narrowed without the table saying so",
            GOOD_SOURCES
            | {
                "src/skip/avx512.rs": (
                    "const CHUNK: usize = 64;\n"
                    "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                )
            },
            False,
            "intends chunk (64) for `avx512`",
        ),
        (
            "a chunk-width backend whose chunk changed is not a false alarm",
            GOOD_SOURCES
            | {
                "src/skip/avx512.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(CHUNK, CHUNK);\n"
                )
            },
            True,
            "",
        ),
        (
            "a new backend with no recorded width",
            GOOD_SOURCES
            | {
                "src/skip/sve.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                )
            },
            False,
            "no intended width is recorded for `sve`",
        ),
        (
            "a backend deleted out from under its table entry",
            {k: v for k, v in GOOD_SOURCES.items() if k != "src/skip/neon.rs"},
            False,
            "no such backend declares `CLASS_PROBE`",
        ),
        (
            "the constant declared in a shape the parser cannot read",
            GOOD_SOURCES
            | {
                "src/skip/avx2.rs": (
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(CHUNK / 4, CHUNK);\n"
                )
            },
            False,
            "cannot read `src/skip/avx2.rs`'s `CLASS_PROBE` declaration",
        ),
        # A declaration the parser cannot read is worth nothing on its own: the
        # width it hides is supplied by a stale comment carrying the shape the
        # parser does recognise. Read as raw text, that pair parses as a backend
        # shipping 8 while the compiler ships a whole chunk.
        (
            "a commented-out declaration standing in for an unreadable real one",
            GOOD_SOURCES
            | {
                "src/skip/avx2.rs": (
                    "// const CLASS_PROBE: usize = super::class_probe(8, CHUNK);\n"
                    "const CHUNK: usize = 32;\n"
                    "const CLASS_PROBE: usize = super::class_probe(CHUNK / 1, CHUNK);\n"
                )
            },
            False,
            "cannot read `src/skip/avx2.rs`'s `CLASS_PROBE` declaration",
        ),
        (
            "a symbolic argument that resolves to nothing",
            GOOD_SOURCES
            | {
                "src/skip/avx2.rs": (
                    "const CLASS_PROBE: usize = super::class_probe(WIDTH, CHUNK);\n"
                )
            },
            False,
            "cannot resolve",
        ),
        (
            "every backend gone: no data is not a pass",
            {"src/skip/mod.rs": GOOD_SOURCES["src/skip/mod.rs"]},
            False,
            "would hold nothing against anything",
        ),
        (
            "the backend directory gone entirely",
            {"build.rs": GOOD_SOURCES["build.rs"]},
            False,
            f"cannot list `{BACKEND_DIR}`",
        ),
        (
            "a build script that overrides the width for every consumer",
            GOOD_SOURCES
            | {
                "build.rs": (
                    "fn main() {\n"
                    '  println!("cargo:rustc-cfg=memspan_class_probe=\\"32\\"");\n'
                    "}\n"
                )
            },
            False,
            "names `memspan_class_probe`",
        ),
        # The same override, routed through the helper this crate's `build.rs`
        # already has. The emitting line carries `rustc-cfg` and no cfg name; the
        # line that carries the cfg name never says `rustc-cfg`. A rule wanting
        # both on one line sees neither.
        (
            "a build script that hides the cfg behind its own helper",
            GOOD_SOURCES
            | {
                "build.rs": (
                    "fn main() {\n"
                    '  use_feature("memspan_class_probe=\\"32\\"");\n'
                    "}\n"
                    "\n"
                    "fn use_feature(feature: &str) {\n"
                    '  println!("cargo:rustc-cfg={}", feature);\n'
                    "}\n"
                )
            },
            False,
            "names `memspan_class_probe`",
        ),
        # The shipped build script's shape, which must keep passing: it routes a
        # different cfg through that same helper.
        (
            "a build script emitting some other cfg through the same helper",
            GOOD_SOURCES
            | {
                "build.rs": (
                    "fn main() {\n"
                    '  if std::env::var("CARGO_FEATURE_TARPAULIN").is_ok() {\n'
                    '    use_feature("tarpaulin");\n'
                    "  }\n"
                    "}\n"
                    "\n"
                    "fn use_feature(feature: &str) {\n"
                    '  println!("cargo:rustc-cfg={}", feature);\n'
                    "}\n"
                )
            },
            True,
            "",
        ),
        (
            "a build script that only mentions it in a comment",
            GOOD_SOURCES
            | {
                "build.rs": (
                    "fn main() {\n"
                    "  // rustc-cfg=memspan_class_probe is the sweep's hook, not ours.\n"
                    "}\n"
                )
            },
            True,
            "",
        ),
        (
            "a cargo config that overrides the width for every build",
            GOOD_SOURCES
            | {
                ".cargo/config.toml": (
                    "[build]\n"
                    'rustflags = ["--cfg", "memspan_class_probe=\\"32\\""]\n'
                )
            },
            False,
            "sets `memspan_class_probe`",
        ),
        (
            "the same, under cargo's older file name",
            GOOD_SOURCES
            | {".cargo/config": 'rustflags = ["--cfg", "memspan_class_probe"]\n'},
            False,
            "sets `memspan_class_probe`",
        ),
        (
            "a cargo config that only mentions it in a comment",
            GOOD_SOURCES
            | {".cargo/config.toml": "# memspan_class_probe belongs to the sweep\n"},
            True,
            "",
        ),
    ]

    failures: list[str] = []
    for label, sources, expect_ok, needle in cases:
        ok, output = run(sources)
        if ok != expect_ok:
            failures.append(
                f"{label}: expected {'pass' if expect_ok else 'failure'}, got "
                f"{'pass' if ok else 'failure'}\n{indent(output)}"
            )
        elif needle and needle not in output:
            failures.append(
                f"{label}: failed for the wrong reason -- {needle!r} not in the "
                f"message\n{indent(output)}"
            )
        else:
            print(f"  {'passes' if expect_ok else 'caught'}  {label}")

    if failures:
        print(f"\n{len(failures)} selftest case(s) wrong:\n")
        for failure in failures:
            print(f"* {failure}")
        return 1

    print(f"\nall {len(cases)} selftest cases behave as claimed")
    return 0


def indent(text: str) -> str:
    return "\n".join(f"      {line}" for line in text.strip().splitlines())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()
    return check(args.repo_root)


if __name__ == "__main__":
    sys.exit(main())
