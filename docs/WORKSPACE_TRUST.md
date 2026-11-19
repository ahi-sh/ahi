# Workspace trust and execution modes

TUI chat asks **Trust this workspace?** when neither a tool policy nor remembered
trust exists. Trust enables read-write: ordinary tools, commands, configured
worker delegation, skills, and bundled scripts without routine approval prompts.
Commands run with the turn runner's account access. This is not containment or a
sandbox. Builtin protected-path checks and existing tool limits still apply.

Choose **t** to trust and continue, **r** to continue read-only without remembering
trust, or **Esc** to cancel. The trust record is keyed to the canonical working
directory in `<state>/workspace-trust/`, outside the checkout. A trust decision
covers that workspace's future contents, not a pinned revision. No policy is
created in the checkout. Separate `--state-dir` development environments have
separate trust records. On a remote connector, the per-turn mode applies on the
server; trust does not sandbox the remote account or authorize a connector.

## Switching modes

The mode is always shown below the draft. **Shift+Tab** cycles available modes;
`/mode` opens a searchable picker. Type to filter, use the arrows to highlight,
Enter to select, or Esc to dismiss. The draft and
conversation stay intact when cycling. No denied action is automatically retried.

- **Read-write**: the explicitly trusted workflow, without a custom policy.
- **Read-only**: reads, searches, and skill/supporting-file reads only. Commands
  (including bundled scripts), writes, and delegation are refused even if the
  model guesses their calls or an existing policy approves them. An existing
  policy is validated but does not narrow this built-in mode’s skill catalog or reads.
- **Policy**: exactly the rules and approval prompts of `ahi.modes.yaml` or the
  file selected with `--tools`. An existing policy chooses this mode at startup,
  even in a remembered trusted workspace. Invalid policies fail closed.

With a v1 policy, Shift+Tab cycles **Policy → Read-only → Read-write → Policy**.
With a v2 file, each named mode appears individually; chat starts in `default_mode`.
Selecting Read-write directly, with Shift+Tab or `/mode`, is the explicit choice
to bypass the custom policy, including deny rules and approval prompts, for this
session. There is no additional confirmation dialog. This does not edit the
policy or save a permanent bypass. Startup still selects Policy.

Both input bars indicate the selected mode: cyan for Policy, blue for Read-only,
and yellow/amber for permissive Read-write. Text labels remain visible so color
is not the only indicator. While a change is queued, the bar uses the next-turn
color and the label shows both the active and next-turn modes.

A newly created workspace policy takes effect on the next turn; read-only remains read-only. A removed policy must be restored or
chat reopened to review trust, rather than silently granting broader access.

Changes during an active turn are shown as **next turn**. Each admitted turn
keeps an immutable mode snapshot; switching cannot stop an already running
command. Use Esc to cancel the current turn separately. The next submitted
prompt uses the queued mode. `/mode revoke` removes remembered workspace trust
and selects Read-only for the next turn.

## Writing a compact workspace mode

`ahi.modes.yaml` version 2 declares a `default_mode` and a `modes` mapping.
Headless Policy uses `default_mode`; interactive chat can select another named entry. Each mode may group permissions under
`allow`, `ask`, and `deny`. Unlisted actions are denied. For example:

```yaml
version: 2
default_mode: review
modes:
  review:
    allow:
      tools: [read_file, list_dir, find_files, search_files]
      skills: ["operator/*"]
    ask:
      commands:
        - argv_prefixes:
            - [cargo, check]
            - [cargo, test]
          working_dirs:
            - crates/ahi-widgets
            - crates/ahi-unix
```

Use `programs` for whole programs or `argv_prefixes` for token prefixes (not
shell strings). A group matches any listed prefix **and** any listed working
directory or its descendants. Omit `working_dirs` to allow any workspace
directory; it never selects the process directory or confines filesystem
access. The model still supplies `run_command.path`. Use `files` groups with `tools` and workspace-relative `paths` for file scopes;
`workers` lists allowed delegate labels. `skill_scripts` accepts groups with
`skills`, `scripts`, and optional `working_dirs` for bundled script calls.
Unknown tools and unsupported match combinations are rejected. `write_files: true` covers edit and write;
`skill_scripts` grants scripts under `scripts/**` independently of skill reads.
See the checkout's `ahi.modes.yaml` for shared command environment settings.

Version 1 policies still work, including under the old `ahi.tools.yaml` name.
Upgrade ahi before renaming a workspace file; older binaries do not recognize
the new filename. Do not leave both files present: ahi refuses to choose.

## Service and headless boundaries

Standalone and service-backed chat submit the same typed per-turn mode. The
service enforces restrictions, not just the UI's tool list. A TUI requesting a
non-default mode refuses older services that do not advertise mode support.
`turn.submit` accepts `policy`, `read-only`, `read-write`, or a workspace mode name; omission
means Policy. Explicit API Read-write is an authorization decision by the caller.
The result records the mode; reusing a run ID with another mode is a conflict.

`ahi exec` remains noninteractive and policy-driven. It neither prompts for nor
inherits the interactive workspace trust record. Existing `ahi tools init`
presets remain available for advanced policy authoring. Read-write is not a
promise that an unconfigured worker or a script unsupported on the platform can
run; existing connector, executable, and platform requirements still apply.
The OpenAI browser-login Responses transport still does not execute tools;
execution modes do not change that transport boundary.

## Local verification

```sh
cargo build --locked
uv run --no-project scripts/smoke-model-picker.py --workspace-modes-only
```

This uses disposable state and a loopback gateway, never real inference. It
checks both engines: trust/cycle/draft preservation, blocked read-only writes,
read-write effects and commands, direct policy bypass cycling, revocation, and mode changes
queued while a turn is still streaming.
