---
name: benchmark-lab-harness
description: Set up and compare Pi and ahi on the same inference-lab chat model, from a no-tools turn to custom coding or firmware fixtures. Rehearse on loopback first. Use for headless workload measurements; use verify-live-harness for interactive UI evidence.
---

# Run the client side of a lab hill climb

The lab's `hill-climb-inference` skill owns model selection, server measurements and the keep/reject decision. This skill owns a repeatable **headless client trial**. Don't load the benchmarking instructions into the conversations under test. No global skill installation, model registration or personal config changes are needed.

## Rehearse without lab access

From the ahi checkout, with Python 3, the installed Pi CLI and the repository Rust toolchain:

```sh
cargo build --offline --locked --bin ahi
S="$PWD/.agents/skills/benchmark-lab-harness"
AHI="$PWD/target/debug/ahi"
PI=$(command -v pi)
python3 "$S/scripts/smoke.py" --ahi "$AHI" --pi "$PI"
```

The smoke starts a disposable loopback fixture, not a model. It checks that each preflight sends **zero** completion requests and each real CLI submits one no-tool prompt and returns the scripted `LAB_OK`. This tests the adapters, not model quality, latency or the TUI. If Pi lacks the isolation flags shown by `pi --help`, stop rather than silently using personal extensions or a fallback model.

## Stage one arm

Get the lab handoff: a private validated `models.json` chat export, exact `provider/model-id`, control/candidate, fixture and grader revision, limits and acceptance rule. Find `.agents/skills/hill-climb-inference/SKILL.md` in the inference-lab checkout for the scoped export command. Use only the reviewed **lab export**; Pi configuration from an arbitrary source can include executable key lookups. Don't copy a personal auth store. Use fresh private storage outside either checkout:

```sh
umask 077
RUN=$(mktemp -d)
# Set CATALOG to the absolute path to the validated export and MODEL to its exact provider/id.
python3 "$S/scripts/trial.py" pi  --binary "$PI"  --catalog "$CATALOG" --model "$MODEL" --out "$RUN/pi-check"
python3 "$S/scripts/trial.py" ahi --binary "$AHI" --catalog "$CATALOG" --model "$MODEL" --out "$RUN/ahi-check"
```

Preflight checks Pi's local listing and `ahi exec --check`; neither submits a prompt. Each arm gets its own state, work directory and prompt. Pi uses print mode, no project instructions/extensions/MCP/skills/session reuse; ahi uses standalone headless Policy mode and records its session. Both request thinking off. Built-in system prompts, schemas, provider defaults, truncation and retry behavior can differ: these are parts of the harness, not server performance. Pin binary versions and inspect effective request settings when attribution depends on them.

**Only with live inference authorization**, rerun the chosen arm with `--live` and a **new** output directory. This submits **one user prompt**, not necessarily one API request. `--timeout` bounds client wall time, not gateway tokens, backend retries or GPU memory. If the agreed request/token budget requires a hard ceiling, enforce it at the gateway or don't run. Never rerun a timed-out arm as if it were the first attempt.

```sh
# Example single authorized arm (default case: reply exactly LAB_OK, no tools).
python3 "$S/scripts/trial.py" ahi --binary "$AHI" --catalog "$CATALOG" --model "$MODEL" \
  --out "$RUN/ahi-01" --timeout 60 --live
```

`trial.json` records client-process wall seconds and exit, **not** a grade. For ahi inspect `result.json` (`status`, `text`, `usage`, tool counts, effects and ledger) and `state/sessions/` for call/result evidence. For Pi inspect JSON events in `stdout`: final assistant `message_end`, text, stop reason, usage and tool results; ignore streamed partials when scoring. Keep `stderr` with failures. For the base case require exactly `LAB_OK` after trimming whitespace, normal completion, no tool calls and no workspace changes. A zero exit alone does not pass.

## Measure a task, not a success sentence

Pass `--case "$S/assets/ticks"` to **both** preflight and live commands for a firmware-oriented C edit. The case starts with a timer that fails across 32-bit rollover. Only the seeded `work/` is copied as the task workspace; the runner doesn't execute any model-produced code or grader. For **tool-enabled** trials run the clients inside a disposable OS sandbox with no personal files, devices or credentials, only approved gateway egress, and keep held-out checks off the model's accessible filesystem. A case policy or temporary `HOME` is **not** containment; Pi's file tools aren't an ahi path policy.

After the run, inspect the recorded read/edit and the whole workspace diff against `assets/ticks/work/`. Only `timer.c` may change. In a separate disposable grader sandbox with the trial workspace and trusted `check.c`, use a host C compiler (no target board or flashing):

```sh
# ARM is the completed arm directory; S points to the skill in the grader.
cc -std=c99 -Wall -Werror "$ARM/work/timer.c" "$S/assets/ticks/check.c" -o "$ARM/check" && "$ARM/check"
```

Record build and test exit. The buggy seed must fail the vectors; a pass covers the tested boundary, not on-device timing. Run generated code only with external time/resource bounds. For a coding task substitute a small buggy module plus a held-out regression test; for a read-only design review score claims against a seeded spec and check for zero edits.

**Bring your own case:** supply a directory with `prompt.txt`, synthetic seed files under `work/`, `pi-tools.txt` (`none` or a comma-separated allowlist), and `ahi.tools.yaml` (default deny; approve only the equivalent operations). Keep verifier, expected results and initial hashes outside `work/`. Review all four inputs, use no symlinks, check the unchanged seed fails and a known-good result passes before spending turns. The model should not be able to edit the acceptance test. For a custom Pi tool, put its registered name in `pi-tools.txt` and pass `--pi-extension /absolute/path/to/reviewed-extension.ts`; preflight checks the path, **only a submitted arm loads the code**. For an ahi workflow, seed `work/.agents/skills/<name>/`, approve that exact `action: skill` in the case policy, and pass `--ahi-skill repo/<name>`; approve any actual tool effects separately. Check the extension's observed call or the ahi skill's recorded load before crediting it. Pin their revisions and vary only one addition per comparison. Label different tool authority or non-equivalent surfaces in the result. Use `verify-live-harness` for TUI confirmations/redraws; headless timing cannot verify them.

Return one row per arm to the lab with case/binary/model pins, actual tool effects, independent grade, client wall time, usage (unknown if absent), and local evidence path. Use the lab's interleaved control/candidate/control and stop rule. Do not mix the loopback rehearsal into the live ledger or publish private transcripts.
