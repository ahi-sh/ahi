# Worked example: command working directory

Use a fresh session and the binary built from the checkout under test. Do not use an existing conversation, change login, or grant shell access. Run at most one authorized model turn; if it fails, report the evidence rather than broadening permissions or retrying automatically.

### Prepare (offline)

From the repo root in a POSIX shell:

```sh
cargo build --locked --bin ahi
ahi_bin="$PWD/target/debug/ahi"
verify="$PWD/.agents/skills/verify-live-harness/scripts/verify-cwd.py"
probe=$(mktemp -d)
mkdir -p "$probe/work/crates/probe"
cat > "$probe/policy.yaml" <<'YAML'
version: 2
default_mode: probe
modes:
  probe:
    allow:
      commands:
        - programs:
            - pwd
          working_dirs:
            - crates/probe
YAML
```

This explicit policy exercises the compact `working_dirs` syntax. For a schema migration, keep exactly the same permission. Use a configured tool-capable model the operator authorizes; stop before inference otherwise.

### Exercise the interactive harness

```sh
(cd "$probe/work" && "$ahi_bin" tui --standalone --tools "$probe/policy.yaml")
```

Select the authorized model. Stay in Policy mode: do not bypass it with Read-write. Submit **one** natural-language prompt, without showing the model an argument template:

> Use run_command once to run pwd from the crates/probe subdirectory. Do not use a shell. Report the observed output.

Observe the live result, including whether it is duplicated or garbled. Exit normally, note the session ID and compare post-exit stdout with what was visible. When testing Rex, do this in the actual Rex pane. If no interactive terminal is available, hand these steps to the operator or use the exec alternative below; never report an exec run as a TUI test.

### Alternative: test the model/tool boundary without the TUI

Instead of the interactive turn, set `model` to the authorized gateway/model and run:

```sh
"$ahi_bin" exec --standalone --model "$model" --workdir "$probe/work" --tools "$probe/policy.yaml" --timeout 60 --result-file "$probe/result.json" -p 'Use run_command once to run pwd from the crates/probe subdirectory. Do not use a shell. Report the observed output.'
id=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["session"])' "$probe/result.json")
```

### Verify (offline)

For the TUI run, set `id` to the exit footer’s session ID. In the same shell:

```sh
"$ahi_bin" session show "$id" --json > "$probe/session.jsonl" &&
  python3 "$verify" "$probe/session.jsonl" "$probe/work"
```

Report model, binary version, surface (Rex TUI / local TUI / exec), session ID and verdict. The assistant repeating a plausible path is not proof. A passing recording with corrupt live text is a **tool pass and display failure**. Keep raw sessions local; remove the disposable fixture after inspection.
