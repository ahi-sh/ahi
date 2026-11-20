# Workspace trust and execution modes

New workspaces start in **Plan**. A workspace policy takes precedence at startup; without a policy, remembered trust selects **Trusted**. Trust is keyed to the canonical directory outside the checkout in `<state>/workspace-trust/`; it covers future contents, not just the current revision.

| Built-in mode | What the model can do |
| --- | --- |
| Plan | Read/search workspace files, load skill instructions, inspect Git, and read/write a temporary draft. No workspace writes, host commands, script execution or delegation. |
| Trusted | Use ordinary tools, host commands, configured workers and bundled scripts without routine approvals. This bypasses workspace rules but not built-in protected paths and tool limits. It is not a sandbox. |

`git_inspect` takes an operation, not a command: `status`, `diff` (optional `staged`) or `log` (first parent from HEAD, optional `count` 1–50). `path` is an optional literal workspace-relative path. These operations use libgit2, not a shell or arbitrary git arguments. Git history can disclose deleted material. `plan_scratch` reads or replaces one private temporary draft (64 KiB) per live session under the OS temporary directory. It accepts no filename. Switching modes never applies a draft. Temporary storage is not a durable plan, and tool requests/results may still appear in session history.

## Switching

`/mode` searches the built-in choices and available workspace policies; Shift+Tab cycles without losing the input draft. Selecting Trusted without remembered trust opens a prompt: **t** trusts the workspace, **r** selects Plan, and **Esc** cancels. `/mode revoke` forgets trust and selects Plan. A change during a turn is queued: cancel that turn separately to stop active work. Denied operations are not retried automatically.

## Workspace policies

**A workspace policy is an authorization source, not a request for trust.** Its default named mode can allow commands on first launch without the Trusted prompt. Review a checkout’s `ahi.modes.yaml` (or legacy policy) before opening chat there, or supply an operator-controlled `--tools` file. The trust prompt governs bypassing that policy, not actions the policy already approves.

A version 2 `ahi.modes.yaml` declares a `default_mode` (`plan` or a named mode) and optionally a `modes` mapping. Named modes can `allow`, `ask` or `deny` operations; unlisted actions are denied. Include `git_inspect` or `plan_scratch` in a named mode's tool grants only when appropriate. Use a `files` match for individual supported workspace file operations; `write_files: true` covers edit and write. See [scoped development](../examples/modes/scoped-development.yaml) for a complete example.

Command groups match a listed program or argv token prefix plus any specified `working_dirs`. These select the launch directory, not filesystem confinement. Skill scripts and worker delegation have separate permissions. Invalid policies fail closed. Version 1 policies continue to work, including under `ahi.tools.yaml`; do not leave both filenames present.

## Service and headless boundaries

The service enforces the mode snapshot on each submitted turn. Explicit `turn.submit` mode values include `plan`, `trusted`, `policy` or a workspace mode name. Legacy `read-only` and `read-write` are accepted; omission selects policy. An API caller choosing Trusted authorizes host execution. Local trust does not authenticate a connector or sandbox the remote account. `ahi exec` is noninteractive and policy-driven, and does not inherit workspace trust. The OpenAI browser-login transport does not execute tools.

Run `cargo build --locked` and `uv run --no-project scripts/smoke-model-picker.py --workspace-modes-only` for the local standalone and service-backed chat checks.
