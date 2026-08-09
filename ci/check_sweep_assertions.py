#!/usr/bin/env python3
"""Drive the probe-sweep workflow's two measurement assertions through every state.

Run with `python3 ci/check_sweep_assertions.py`.

# Why this exists

`.github/workflows/probe-sweep.yml` ends each tier with an assertion that the
tier actually produced a table, and ends the whole matrix with a job that holds
the run's artifacts against the tiers that had to produce one. Both were added
because the workflow spent its entire life reporting absence as success: when a
runner lacked the CPU flag, *every* step of that tier -- build, differential,
sweep, render and upload -- was skipped by one shared `available == 'true'`
condition, and the leg exited green having measured nothing.

Neither assertion can be exercised by a normal run. Every hosted x86 runner has
SSE4.2 and AVX2, so the failing branch never executes; no hosted runner has
AVX-512, so the passing branch of that leg never executes either. A green
workflow is therefore no evidence at all that these two would fail when they
should -- which is precisely the mistake being corrected. This script supplies
the evidence instead, by pulling the shipped `run:` text out of the workflow,
substituting what the runner would substitute, and driving it.

# What it pins beyond the exit codes

It asserts that the per-tier step still carries `if: always()`, and that the
aggregation job does too. That is the whole mechanism: a check guarded by the
same condition as the steps it audits would skip alongside them, and the
condition is exactly what went wrong. Re-adding a guard is the one edit that
would silently restore the original defect, so it fails here rather than
passing quietly.

It also runs the shipped text rather than a copy of it. A transcript of a
hand-check goes stale against an edited script and reports the stale result as
coverage; the extraction below fails loudly if a step is renamed out from under
it, and refuses any `${{ }}` placeholder it was not taught to substitute.
"""

from __future__ import annotations

import os
import re
import stat
import subprocess
import sys
import tempfile

try:
    import yaml
except ImportError:  # pragma: no cover - exercised only on a host without PyYAML
    print(
        "\n**sweep-assertion check failed:** PyYAML is not importable, so the "
        "workflow cannot be read. Install it (`python3 -m pip install pyyaml`).\n"
    )
    sys.exit(1)

WORKFLOW = ".github/workflows/probe-sweep.yml"
TIER_STEP = "Assert this tier produced a measurement"
MATRIX_STEP = "Assert every tier that had to measure uploaded its sweep"


def die(message: str) -> None:
    print(f"\n**sweep-assertion check failed:** {message}\n")
    sys.exit(1)


def substitute(text: str, values: dict[str, str], where: str) -> str:
    for key, value in values.items():
        text = text.replace("${{ " + key + " }}", value)
    leftover = sorted(set(re.findall(r"\$\{\{.*?\}\}", text)))
    if leftover:
        die(
            f"the {where} script uses {', '.join(leftover)}, which this check "
            "does not know how to substitute. It would otherwise run text the "
            "runner never runs, and report the result as coverage."
        )
    return text


def load(repo_root: str) -> tuple[str, str]:
    """The two shipped scripts, with their unconditionality asserted."""
    try:
        with open(os.path.join(repo_root, WORKFLOW)) as handle:
            doc = yaml.safe_load(handle)
    except (OSError, yaml.YAMLError) as err:
        die(f"cannot read `{WORKFLOW}`: {err}")

    tier_script = None
    for step in doc["jobs"]["sweep"]["steps"]:
        if step.get("name") != TIER_STEP:
            continue
        if step.get("if") != "always()":
            die(
                f"the `{TIER_STEP}` step has `if: {step.get('if')!r}` instead of "
                "`always()`.\n\nThat condition is the entire mechanism. Every "
                "measuring step in this job is guarded by "
                "`available == 'true'`, and the defect being corrected is that "
                "when the guard was false all of them agreed to skip and the "
                "leg exited green. An assertion that shares any guard skips "
                "with them and audits nothing."
            )
        if step.get("shell") != "bash":
            die(f"the `{TIER_STEP}` step lost `shell: bash`, so `pipefail` is off.")
        tier_script = step["run"]

    matrix_job = doc["jobs"].get("require-measurements")
    if matrix_job is None:
        die(
            "`.github/workflows/probe-sweep.yml` has no `require-measurements` "
            "job. Without it, a tier whose leg never ran at all leaves nothing "
            "behind to assert anything and the run is green."
        )
    if matrix_job.get("if") != "always()":
        die(
            f"`require-measurements` has `if: {matrix_job.get('if')!r}` instead "
            "of `always()`, so it would not run after a failed or skipped leg -- "
            "the case it exists for."
        )
    if matrix_job.get("needs") != "sweep":
        die("`require-measurements` must `needs: sweep` to see the matrix result.")

    matrix_script = None
    for step in matrix_job["steps"]:
        if step.get("name") == MATRIX_STEP:
            matrix_script = step["run"]

    if tier_script is None:
        die(f"no `{TIER_STEP}` step in the sweep job -- this check is now stale.")
    if matrix_script is None:
        die(f"no `{MATRIX_STEP}` step -- this check is now stale.")
    return tier_script, matrix_script


def run_bash(script: str, env: dict[str, str], cwd: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", "--noprofile", "--norc", "-c", script],
        capture_output=True,
        text=True,
        env=env,
        cwd=cwd,
    )


# (label, tier, required_measurement, summary contents or None, expected rc)
TIER_CASES = [
    ("required tier, nothing rendered", "sse42", "true", None, 1),
    ("required tier, an empty file", "sse42", "true", "", 1),
    (
        "required tier, the reporter refused into the same path",
        "sse42", "true",
        "**probe-sweep failed:** 3 scored row(s) provably cannot distinguish\n",
        1,
    ),
    (
        "required tier, a real table",
        "sse42", "true",
        "CPU: `x`\n\n### `sse42` probe-length sweep\n\n| class | probe 8 |\n",
        0,
    ),
    (
        "required tier, another tier's table under its name",
        "avx2", "true", "### `sse42` probe-length sweep\n", 1,
    ),
    ("the optional tier, nothing rendered", "avx512", "false", None, 0),
    (
        "the optional tier, on a host that could measure it",
        "avx512", "false", "### `avx512` probe-length sweep\n", 0,
    ),
]

# (label, artifact names the run has, sweep job result, expected rc)
MATRIX_CASES = [
    ("both required tiers uploaded, sweep green",
     ["probe-sweep-sse42", "probe-sweep-avx2"], "success", 0),
    ("both required tiers plus the optional one",
     ["probe-sweep-sse42", "probe-sweep-avx2", "probe-sweep-avx512"], "success", 0),
    ("a required tier uploaded nothing",
     ["probe-sweep-sse42"], "success", 1),
    ("only the optional tier uploaded",
     ["probe-sweep-avx512"], "success", 1),
    ("the run produced no artifacts at all", [], "success", 1),
    ("required tiers uploaded but a leg failed",
     ["probe-sweep-sse42", "probe-sweep-avx2"], "failure", 1),
    ("a leg was cancelled",
     ["probe-sweep-sse42", "probe-sweep-avx2"], "cancelled", 1),
    ("a near-miss artifact name does not count",
     ["probe-sweep-sse42", "probe-sweep-avx2-partial"], "success", 1),
]


def check_tier(script: str) -> list[str]:
    failures = []
    for label, tier, required, contents, expected in TIER_CASES:
        with tempfile.TemporaryDirectory() as workspace:
            os.makedirs(os.path.join(workspace, "criterion-sweep"))
            if contents is not None:
                path = os.path.join(
                    workspace, "criterion-sweep", f"summary-{tier}.md"
                )
                with open(path, "w") as handle:
                    handle.write(contents)

            summary_file = os.path.join(workspace, "step-summary.md")
            open(summary_file, "w").close()
            proc = run_bash(
                substitute(
                    script,
                    {
                        "github.workspace": workspace,
                        "matrix.tier": tier,
                        "matrix.cpu_flag": f"{tier}_flag",
                        "matrix.required_measurement": required,
                    },
                    "per-tier assertion",
                ),
                dict(os.environ, GITHUB_STEP_SUMMARY=summary_file),
                workspace,
            )
            with open(summary_file) as handle:
                summary = handle.read()

        if proc.returncode != expected:
            failures.append(
                f"per-tier / {label}: rc {proc.returncode}, wanted {expected}\n"
                f"{proc.stdout}{proc.stderr}"
            )
        elif expected == 1 and "::error" not in proc.stdout:
            failures.append(
                f"per-tier / {label}: failed without an ::error annotation, so "
                "the run log would not say which tier"
            )
        elif expected == 1 and not summary.strip():
            failures.append(
                f"per-tier / {label}: failed without writing to the step "
                "summary, where a reader looks first"
            )
        else:
            print(f"  ok  per-tier    {label}")
    return failures


def check_matrix(script: str, repo_root: str) -> list[str]:
    failures = []
    for label, artifacts, result, expected in MATRIX_CASES:
        with tempfile.TemporaryDirectory() as bindir:
            # Stub `gh`: the artifact list is the input under test, and the
            # real API cannot be asked to have not uploaded something.
            shim = os.path.join(bindir, "gh")
            with open(shim, "w") as handle:
                handle.write(
                    "#!/bin/sh\n" + "".join(f'echo "{n}"\n' for n in artifacts)
                )
            os.chmod(shim, os.stat(shim).st_mode | stat.S_IEXEC)

            summary_file = os.path.join(bindir, "step-summary.md")
            open(summary_file, "w").close()
            proc = run_bash(
                substitute(
                    script,
                    {
                        "github.repository": "al8n/memspan",
                        "github.run_id": "1",
                        "needs.sweep.result": result,
                    },
                    "aggregation",
                ),
                dict(
                    os.environ,
                    PATH=bindir + os.pathsep + os.environ["PATH"],
                    GITHUB_STEP_SUMMARY=summary_file,
                ),
                repo_root,
            )

        if proc.returncode != expected:
            failures.append(
                f"aggregation / {label}: rc {proc.returncode}, wanted "
                f"{expected}\n{proc.stdout}{proc.stderr}"
            )
        else:
            print(f"  ok  aggregation {label}")
    return failures


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    tier_script, matrix_script = load(repo_root)

    failures = check_tier(tier_script) + check_matrix(matrix_script, repo_root)
    if failures:
        print(f"\n{len(failures)} state(s) wrong:\n")
        for failure in failures:
            print(f"* {failure}")
        return 1

    print(
        f"\nall {len(TIER_CASES) + len(MATRIX_CASES)} states of the shipped "
        "assertions behave as claimed"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
