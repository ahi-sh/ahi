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
`/mode` opens the numbered chooser, with the selected policy path. The draft and
conversation stay intact when cycling. No denied action is automatically retried.

- **Read-write**: the explicitly trusted workflow, without a custom policy.
- **Read-only**: reads, searches, and skill/supporting-file reads only. Commands
  (including bundled scripts), writes, and delegation are refused even if the
  model guesses their calls or an existing policy approves them. An existing
  policy still restricts which reads are available and which need approval.
- **Policy**: exactly the rules and approval prompts of `ahi.tools.yaml` or the
  file selected with `--tools`. An existing policy chooses this mode at startup,
  even in a remembered trusted workspace. Invalid policies fail closed.

With a policy, Shift+Tab initially cycles Policy/Read-only. Selecting Read-write
in `/mode` asks for explicit confirmation that it bypasses the custom policy,
including deny rules and approval prompts, for this session. Only after that
confirmation does Read-write enter the cycle. This does not edit the policy or
save a permanent bypass. A newly created workspace policy takes effect on the
next turn; read-only remains read-only. A removed policy must be restored or
chat reopened to review trust, rather than silently granting broader access.

Changes during an active turn are shown as **next turn**. Each admitted turn
keeps an immutable mode snapshot; switching cannot stop an already running
command. Use Esc to cancel the current turn separately. The next submitted
prompt uses the queued mode. `/mode revoke` removes remembered workspace trust,
clears the session bypass, and selects Read-only for the next turn.

## Service and headless boundaries

Standalone and service-backed chat submit the same typed per-turn mode. The
service enforces restrictions, not just the UI's tool list. A TUI requesting a
non-default mode refuses older services that do not advertise mode support.
`turn.submit` accepts `mode: "policy" | "read-only" | "read-write"`; omission
means Policy. Explicit API Read-write is an authorization decision by the caller.
The result records the mode; reusing a run ID with another mode is a conflict.

`ahi exec` remains noninteractive and policy-driven. It neither prompts for nor
inherits the interactive workspace trust record. Existing `ahi tools init`
presets remain available for advanced policy authoring. Read-write is not a
promise that an unconfigured worker or a script unsupported on the platform can
run; existing connector, executable, and platform requirements still apply.

## Local verification

```sh
cargo build --locked
uv run --no-project scripts/smoke-model-picker.py --workspace-modes-only
```

This uses disposable state and a loopback gateway, never real inference. It
checks both engines: trust/cycle/draft preservation, blocked read-only writes,
read-write effects, policy bypass confirmation, revocation, and mode changes
queued while a turn is still streaming.
