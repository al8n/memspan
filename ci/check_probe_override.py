#!/usr/bin/env python3
"""Fail when a build of this crate is handed `--cfg memspan_class_probe`.

Run with `python3 ci/check_probe_override.py` from `.github/workflows/ci.yml`'s
`probe-override` job on every pull request; `--selftest` proves it bites.

# What decides the probe width, and why it is not this

Each backend pins the width it ships with a compile-time assertion next to the
constant:

    const _: () = assert!(super::CLASS_PROBE_OVERRIDE != 0 || CLASS_PROBE == 8);

rustc evaluates that on every build of the crate -- SSE4.2, AVX2 and AVX-512 in
every x86 job, NEON in `cross` and the aarch64 `miri` legs, wasm `simd128` in
`test-wasm-simd128` -- so a widened probe is a failed build rather than a red
script. Four review rounds were spent teaching a Python parser what the compiler
already knew (strip comments, resolve a symbolic argument, re-implement the
clamp, ignore `cfg`-inactive items, ignore `macro_rules!` bodies), and each round
found another way for the text to differ from the compiled constant. The parser
is gone; `src/skip/mod.rs`'s `CLASS_PROBE_OVERRIDE` documents the assertion.

# The one gap that leaves, which is this file

The assertion is skipped when the override is set, because a sweep leg sets it
deliberately and a different width is the whole point of that build. So the one
thing the compiler cannot report is the case where the cfg is set *and nobody
meant it to be*: the width moves and the assertion that would have noticed
stands down. `build.rs`, `.cargo/config.toml`, `[build] rustflags`, a
target-specific `rustflags` table and an ambient `RUSTFLAGS` can each do that.

Rather than enumerate those five and parse each one -- the mistake this file
exists to stop repeating -- it runs a build and reads the flags rustc was
actually handed. Every one of the five arrives as a `--cfg` argument on the
crate's own `rustc` invocation, which `cargo check -vv` prints verbatim, so one
observation covers all of them and covers whatever the sixth turns out to be.
Two of them were measured on this repository before this check was written:
`RUSTFLAGS='--cfg memspan_class_probe="16"'` and a `println!("cargo:rustc-cfg=
memspan_class_probe=\\"16\\"")` in a build script both surface as
`--cfg 'memspan_class_probe="16"'` there.

# Why it parses the invocation instead of searching it

`memspan_class_probe` appears on every clean `rustc` line already, inside the
`--check-cfg 'cfg(memspan_class_probe, values("4", "8", ...))'` argument
`Cargo.toml` declares. A substring search for the name is therefore red on a
correct build, and a search for `--cfg memspan_class_probe` misses
`--cfg=memspan_class_probe="16"`. The invocation is shell-quoted by cargo, so it
is split with `shlex` and only the value after a `--cfg` token -- never
`--check-cfg` -- is read. `--selftest` pins that distinction directly.

# Absence is not a pass

The observation is worthless if nothing compiled, and cargo prints no `rustc`
line for a unit it considers fresh. Each observation therefore runs in its own
empty target directory, and refuses to return a verdict unless it saw this
crate's library compiled. A green tick over a build that never happened is the
defect this repository shipped once already, in the sweep workflow.

# What it still cannot see

It observes the builds it runs: `cargo check --lib`, with default features and
with `--all-features`, in this job on this runner. That closes every source that
applies to *any* build in the workspace, which is where an override would have
to live to affect what the crate ships. It does not see a `RUSTFLAGS` exported
inside some other job -- that is one build rather than what the crate ships, and
the sweep and `probe-timing` legs set exactly that on purpose -- and it does not
see a consumer's build, which is the documented measurement hook working as
intended.

# It reports a breach loudly; it does not stop a merge

This repository has no branch protection rule and no ruleset, so a failure here
turns the check red on the pull request and a maintainer can still merge or push
directly past it. Making it required is a repository setting, not something a
workflow can assert about itself.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import os
import re
import shlex
import subprocess
import sys
import tempfile

# The cfg that moves the probe width. `Cargo.toml` declares it under
# `lints.rust.unexpected_cfgs.check-cfg` and `src/skip/mod.rs` reads it; no
# build of this crate is supposed to set it except a sweep leg.
OVERRIDE_CFG = "memspan_class_probe"

CRATE = "memspan"

# The feature sets observed. A build script can emit a cfg conditionally on a
# feature -- this one already emits `tarpaulin` that way -- so observing only
# the default build would leave a build the crate can be published under
# unobserved.
CONFIGURATIONS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("default features", ()),
    ("--all-features", ("--all-features",)),
)

# `Running \`<shell-quoted command>\`` as `cargo -vv` prints it.
RUNNING_RE = re.compile(r"^\s*Running `(.*)`\s*$")


def die(message: str) -> None:
    print(f"\n**probe-override check failed:** {message}\n")
    sys.exit(1)


def invocations(output: str) -> list[list[str]]:
    """Every command `cargo -vv` reported running, split as the shell would."""
    found: list[list[str]] = []
    for line in output.splitlines():
        matched = RUNNING_RE.match(line)
        if not matched:
            continue
        try:
            found.append(shlex.split(matched.group(1)))
        except ValueError as err:
            die(f"cannot split a command cargo reported running: {err}\n\n    {line.strip()}")
    return found


def cfg_values(tokens: list[str]) -> list[str]:
    """The value of every `--cfg` argument in one invocation.

    `--check-cfg` is a different argument and is not one of these; that is the
    whole reason this reads arguments rather than searching text.
    """
    values: list[str] = []
    for index, token in enumerate(tokens):
        if token == "--cfg":
            if index + 1 >= len(tokens):
                die(f"a `--cfg` with no value in: {shlex.join(tokens)}")
            values.append(tokens[index + 1])
        elif token.startswith("--cfg="):
            values.append(token[len("--cfg=") :])
    return values


def compiles_this_crate(tokens: list[str]) -> str | None:
    """The unit name if this invocation is rustc building part of this crate."""
    for index, token in enumerate(tokens):
        if token == "--crate-name" and index + 1 < len(tokens):
            name = tokens[index + 1]
            if name in (CRATE, "build_script_build"):
                return name
    return None


def observe(repo_root: str, extra: tuple[str, ...], target_dir: str) -> dict[str, list[str]]:
    """`{unit: cfg values}` for every unit of this crate cargo compiled.

    `target_dir` is a parameter rather than a temporary directory created here
    so that `--selftest` can hand the same one to two runs and drive the
    nothing-was-compiled branch, which is unreachable otherwise.
    """
    environment = dict(os.environ)
    environment["CARGO_TARGET_DIR"] = target_dir
    environment["CARGO_TERM_COLOR"] = "never"

    command = ["cargo", "check", "-vv", "--lib", *extra]
    try:
        completed = subprocess.run(
            command,
            cwd=repo_root,
            env=environment,
            capture_output=True,
            text=True,
        )
    except OSError as err:
        die(f"cannot run `{shlex.join(command)}`: {err}")

    if completed.returncode != 0:
        die(
            f"`{shlex.join(command)}` failed, so nothing was observed. A build "
            "that does not compile cannot answer what flags it was built with; "
            "fix the build and run this again.\n\n"
            + indent(completed.stdout[-2000:] + completed.stderr[-2000:])
        )

    units: dict[str, list[str]] = {}
    for tokens in invocations(completed.stdout + completed.stderr):
        unit = compiles_this_crate(tokens)
        if unit is not None:
            units.setdefault(unit, []).extend(cfg_values(tokens))
    return units


def require_compiled(units: dict[str, list[str]], extra: tuple[str, ...]) -> None:
    """Refuse a verdict drawn from a build that compiled nothing."""
    if CRATE in units:
        return
    die(
        f"`{shlex.join(['cargo', 'check', '-vv', '--lib', *extra])}` compiled no "
        f"`{CRATE}` library, so this check observed nothing. Cargo prints no "
        "`rustc` line for a unit it considers fresh, and a verdict from a build "
        "that never ran is exactly the shape of green tick this repository has "
        "already shipped once.\n\n"
        f"Units seen: {sorted(units) or 'none'}."
    )


def check(repo_root: str) -> int:
    rows: list[tuple[str, str, int]] = []

    for label, extra in CONFIGURATIONS:
        with tempfile.TemporaryDirectory(prefix="memspan-probe-override-") as target_dir:
            units = observe(repo_root, extra, target_dir)

        require_compiled(units, extra)

        for unit, values in sorted(units.items()):
            named = [value for value in values if value.split("=", 1)[0].strip() == OVERRIDE_CFG]
            if named:
                die(
                    f"building `{CRATE}` with {label} passes rustc "
                    + ", ".join(f"`--cfg {value}`" for value in named)
                    + f" when compiling `{unit}`.\n\n"
                    "That overrides every backend's scalar-probe width, and it "
                    "also disables the compile-time assertion that would "
                    "otherwise pin the shipped width -- see "
                    "`CLASS_PROBE_OVERRIDE` in `src/skip/mod.rs`. So the crate "
                    "would ship a width no source file states and nothing else "
                    "would notice.\n\n"
                    "It arrives from one of `RUSTFLAGS`, "
                    "`CARGO_ENCODED_RUSTFLAGS`, a `rustflags` key in "
                    "`.cargo/config.toml`, or a `cargo:rustc-cfg` line in "
                    "`build.rs`. The sweep in "
                    "`.github/workflows/probe-sweep.yml` is the one place that "
                    "is meant to set it, one build at a time."
                )
            rows.append((label, unit, len(values)))

    print("### probe-override cfg\n")
    print("| build | unit | `--cfg` arguments | `memspan_class_probe` |")
    print("|---|---|---|---|")
    for label, unit, count in rows:
        print(f"| {label} | `{unit}` | {count} | not set |")
    print(
        f"\nNo build observed here hands rustc `--cfg {OVERRIDE_CFG}`, so every "
        "backend compiles at the width its source states and every backend's "
        "shipped-width assertion was live while it did. The widths themselves "
        "are pinned by those assertions, not by this check; this only rules out "
        "the flag that would stand them down."
    )
    return 0


# ── selftest ─────────────────────────────────────────────────────────────────


def selftest() -> int:
    """Drive every state this check claims to decide, and require each verdict."""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    script = os.path.abspath(__file__)
    failures: list[str] = []

    def run(label: str, environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
        print(f"  {label} ...", flush=True)
        return subprocess.run(
            [sys.executable, script],
            cwd=repo_root,
            env=environment,
            capture_output=True,
            text=True,
        )

    clean = dict(os.environ)
    clean.pop("RUSTFLAGS", None)
    clean.pop("CARGO_ENCODED_RUSTFLAGS", None)

    # 1. A clean tree passes. Without this the other cases would be satisfied by
    #    a check that fails unconditionally.
    passed = run("a clean build passes", clean)
    if passed.returncode != 0:
        failures.append(
            "a clean build was rejected:\n" + indent(passed.stdout + passed.stderr)
        )

    # 2. The flag this check exists for, arriving the way the reviewer described:
    #    ambient `RUSTFLAGS`, which no source scan can see at all.
    overridden = dict(clean)
    overridden["RUSTFLAGS"] = f'--cfg {OVERRIDE_CFG}="16"'
    caught = run("an ambient RUSTFLAGS override fails", overridden)
    if caught.returncode == 0:
        failures.append(
            "an ambient `RUSTFLAGS` override was accepted:\n"
            + indent(caught.stdout + caught.stderr)
        )
    elif OVERRIDE_CFG not in caught.stdout:
        failures.append(
            "an ambient `RUSTFLAGS` override failed without naming the cfg:\n"
            + indent(caught.stdout + caught.stderr)
        )

    # 3. The `--check-cfg` trap. A clean build's rustc line already contains the
    #    cfg name, inside the `--check-cfg` argument `Cargo.toml` declares, so a
    #    check that searched the text would be red above and this pins that the
    #    distinction is the reason it is not.
    with tempfile.TemporaryDirectory(prefix="memspan-probe-override-selftest-") as target_dir:
        environment = dict(clean)
        environment["CARGO_TARGET_DIR"] = target_dir
        environment["CARGO_TERM_COLOR"] = "never"
        raw = subprocess.run(
            ["cargo", "check", "-vv", "--lib"],
            cwd=repo_root,
            env=environment,
            capture_output=True,
            text=True,
        )
        print("  the cfg name appears in --check-cfg and is not counted ...", flush=True)
        text = raw.stdout + raw.stderr
        if f"--check-cfg 'cfg({OVERRIDE_CFG}" not in text:
            failures.append(
                "a clean build no longer declares the cfg under `--check-cfg`, so "
                "this case is no longer testing the trap it was written for. "
                "Either `Cargo.toml` stopped declaring it -- which would make "
                "`unexpected_cfgs` fire on `src/skip/mod.rs` -- or cargo changed "
                "how it prints the argument."
            )
        else:
            counted = [
                value
                for tokens in invocations(text)
                if compiles_this_crate(tokens)
                for value in cfg_values(tokens)
                if value.split("=", 1)[0].strip() == OVERRIDE_CFG
            ]
            if counted:
                failures.append(
                    "the `--check-cfg` declaration was read as a `--cfg`: "
                    f"{counted}"
                )

    # 4. Nothing compiled. Cargo prints no `rustc` line for a fresh unit, so a
    #    second observation in the same target directory sees no invocation --
    #    the state where the check must refuse rather than report success.
    print("  a build that compiled nothing fails ...", flush=True)
    with tempfile.TemporaryDirectory(prefix="memspan-probe-override-selftest-") as target_dir:
        first = observe(repo_root, (), target_dir)
        if CRATE not in first:
            failures.append(
                "the first observation into an empty target directory compiled no "
                f"`{CRATE}` library, so case 4 cannot test what it claims."
            )
        second = observe(repo_root, (), target_dir)
        if CRATE in second:
            failures.append(
                "a repeated build into a warm target directory still reported a "
                f"`{CRATE}` compilation, so the branch that refuses an "
                "observation of nothing is unreachable and untested here."
            )
        else:
            # Drive the refusal itself, not just the state that reaches it: an
            # empty observation has to exit non-zero rather than fall through to
            # a table with no rows in it.
            try:
                # Its refusal goes to stdout; swallow it so a passing selftest
                # does not print something that reads like a failure.
                with contextlib.redirect_stdout(io.StringIO()):
                    require_compiled(second, ())
            except SystemExit as exit_code:
                if exit_code.code != 1:
                    failures.append(
                        "an observation of nothing exited "
                        f"{exit_code.code}, not 1."
                    )
            else:
                failures.append(
                    "an observation of nothing returned normally instead of "
                    "failing the check."
                )

    if failures:
        print("\n**probe-override selftest failed:**\n")
        for failure in failures:
            print(f"  * {failure}\n")
        return 1

    print("\nprobe-override selftest: every state produced the required verdict.")
    return 0


def indent(text: str) -> str:
    return "\n".join(f"    {line}" for line in text.splitlines())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="drive every state this check decides and require each verdict",
    )
    arguments = parser.parse_args()

    if arguments.selftest:
        return selftest()
    return check(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


if __name__ == "__main__":
    sys.exit(main())
