# Shared skill directories

Skills now use `.agents/skills/<name>/SKILL.md` in the workspace and
`~/.agents/skills/<name>/SKILL.md` for the operator. The home directory is
resolved by the OS (typically `%USERPROFILE%` on Windows); ahi's XDG and
`--state-dir` overrides do not relocate shared skills.

Move repository skills from `.ahi/skills` to `.agents/skills`, and operator
skills from `<ahi config>/skills` to `~/.agents/skills`. Old locations are no
longer loaded. `ahi doctor` reports a deprecated repository directory if present.
No files are moved automatically. Operator skills retain precedence over
repository skills of the same name. Existing tool-policy approvals and symlink
restrictions remain unchanged; this does not introduce a new skill allow/deny
list or implement `AGENTS.md` instruction loading.

# Migrating to explicit primitive names

After the 0.17.0 release, ahi began reorganizing its `src/` modules around the
primitives it is aiming to provide: model-callable tools, explicit execution
policy, a precommit runner, terminal interfaces, durable sessions, skills, and
development environments. The goal is to make each responsibility easy to find
and keep its boundaries clear—not to turn every module into a separate crate
or introduce a plugin system.

The distinctions matter. A tool implements an operation; tool policy determines
whether a requested operation is authorized; and the CLI lets an operator manage
that policy. Precommit runs repository checks, while Git hooks are entry points
that invoke it. Durable records retain execution facts, while application code
decides how to present them. These responsibilities should not be hidden inside
terminal chat code or share a name merely because they currently share helpers.

This first refactoring establishes the `tools`, `tool_policy`, `precommit`, and
corresponding CLI module boundaries, and separates durable omission reasons from
their display wording. It also makes `ahi precommit` the canonical command
namespace, with temporary deprecated aliases for existing scripts and workflows.
Tool names and authorization semantics remain unchanged.

The broader structure is still a direction, not a claim of completion. Separating
the agent runtime from providers and the TUI, moving session storage into
`ahi-durable`, and defining execution and containment boundaries remain follow-up
work. The new `terminal_host` module interfaces with external terminal
multiplexers: Rex is primary and hc is an explicit alternative. It does not own
ahi's TUI, external UI clients, or agent orchestration. Native adapters and an
operator CLI are implemented with fixture coverage, not a claim of verified
live interoperability. The migration steps below cover the changes implemented
now; the [module structure proposal](docs/MODULE_STRUCTURE.md) describes the
remaining work.

## Terminal hosting and delegation

`ahi terminal-host` is a new operator-only command; it does not replace `ahi tui`,
`ahi --session`, `ahi serve`, or `ahi connect`. Its `--host-session` identifies a
native Rex session or hc core namespace, never a saved ahi conversation.
See the [README glossary and command guide](README.md#ahi-terminal-host) for
explicit target selection, native behavior differences, limits, exit codes,
and side-effect-free `--dry-run` examples. No backend installation, live service
access, terminal creation, or OSC 7501 interoperability check was part of the
fixture verification. Existing session files and policy grants are unchanged.

**Delegation now requires `AHI_DELEGATE_VIA`.** The removed default,
`hc connect --worker {worker}`, was not a working contract: hc 0.1.21 has no
`--worker` option, and `hc connect` carries hc's protocol rather than ahi's.
Changing that flag to `--session` would not fix the protocol mismatch.

Configure a connector that speaks ahi's session protocol, for example:

```sh
export AHI_DELEGATE_VIA='ssh -- {worker} ahi connect'
ahi doctor
```

This example deliberately maps worker labels to operator-managed SSH aliases.
If labels are not SSH aliases, provide an explicit routing wrapper instead.
The template is split into argv without a shell, then `{worker}` is substituted.
Unset or blank configuration fails before starting a connector; existing valid
explicit connectors continue to work. This changes the invalid delegation
default, not tool authorization or the meaning of a worker's result. No label
registry or automatic transport bridge has been introduced.

## Precommit commands

The pre-commit-compatible runner now has an explicit `ahi precommit` namespace.
The old spellings remain temporary deprecated aliases. They call the same
implementation, accept the same arguments, produce the same stdout as their
canonical equivalents, and preserve exit status. A deprecated invocation adds
one warning to stderr. Scripts must not treat an empty stderr stream as the
success criterion; use the exit status.

| Deprecated invocation | Canonical invocation |
| --- | --- |
| `ahi run …` | `ahi precommit run …` |
| `ahi list-checks …` | `ahi precommit list-checks …` |
| `ahi explain …` | `ahi precommit explain …` |
| `ahi migrate …` | `ahi precommit migrate …` |
| `ahi hooks install` | `ahi precommit install` |
| `ahi hooks uninstall` | `ahi precommit uninstall` |

Global options such as `--config`, `--state-dir`, and `--cache-dir` remain usable
before or after the command. Aliases are omitted from top-level help, but their
own `--help` still works and identifies the replacement. Canonical commands do
not print deprecation warnings. Check descriptions and generated configuration
suggestions now refer to canonical commands; report formats are unchanged.

### Support window

Aliases remain supported through at least two subsequent minor releases after
the first release containing this change. Removing them requires a separate
breaking-change release and an explicit removal-version announcement in an
earlier release's notes. There is no date-triggered or version-triggered runtime
expiry. These aliases are introduced in **0.18.0** and remain supported through
at least **0.20.x**. No removal version has been scheduled; removal requires
the separate announcement and breaking-change release described above.

### Update an existing repository

1. Install or build a version with `ahi precommit --help` before changing callers.
   For a checkout build, use `target/debug/ahi` explicitly or put `target/debug`
   first on `PATH`; the shipped shim prefers an `ahi` found on `PATH` over its
   checkout fallback.
2. Review CI commands, shell scripts, editor tasks, documentation, and Git shims
   for the deprecated invocations above. Update the executable invocation, not
   arbitrary occurrences of words such as `run` or `hooks`.
3. Keep existing arguments and quoting. For example:

   ```sh
   # Before
   exec ahi run --fix

   # After
   exec ahi precommit run --fix
   ```

4. Find the active Git hook directory using `git rev-parse --git-path hooks` and
   check `git config --get core.hooksPath` for a configured override. Review the
   hook scripts before editing; preserve other tools and repository-specific
   logic. Do not assume a shim is owned by ahi merely because it invokes ahi.
5. Run the canonical command in a disposable or appropriate working copy. Begin
   with `ahi precommit run --all-files` without `--fix` when validating behavior.
   Approved local hooks can still have side effects; inspect their policy first.
6. Stop relying on the deprecated spellings before the announced removal release.

The binary upgrade does not rewrite repositories or activate hooks. This PR
updates the repository's shipped shims and examples, not a user's Git
configuration. Git shim install/uninstall remain unimplemented and return failure
under both spellings; the new namespace does not claim they now work.

`ahi precommit migrate` previews a **configuration** suggestion and writes only
with `--write` (and `--force` when replacing a file). It does not rewrite CLI
invocations in scripts. No automatic script or hook migration is introduced.

Keep `.pre-commit-config.yaml` and `ahi.hooks.yaml` unchanged. They remain the
check configuration and local hook-execution authorization files.

## Tools versus tool policy

`ahi tools init`, `list`, and `examples` retain their public spellings and
behavior. Internally, model-callable implementations live in `src/tools/`, their
operator-authored authorization rules in `src/tool_policy/`, and policy command
handling in `src/cli/tool_policy.rs`.

There is one implementation module per tool name. Tool names and policy action
names are distinct: `file_write`, for example, can govern both `write_file` and
`edit_file`. Tool names, schemas, policy files, and permission semantics are not
renamed by this change.

## Rust module paths

This is also a source-level organization change for consumers of the `ahi`
library. The runner moves from `ahi::hooks` to `ahi::precommit`, and built-in
checks from `hooks::check` to `precommit::checks`. The old `ahi::tools` module was
CLI policy-rendering code, not the tool executor; those implementation helpers
are now private to the CLI module. No compatibility re-exports of the old Rust
paths are retained. The temporary alias policy above applies to the public CLI,
not to Rust implementation paths.

The four existing supporting crates retain their names. No new crate or plugin
API is introduced.

## Persisted tool omission reasons

New omitted-detail records store a typed reason:

```json
{"kind":"omitted","reason":{"kind":"detail_too_large"}}
```

The application supplies the display wording. `ahi-durable` stores facts, not
terminal styling or newly generated English omission messages.

Sessions with older string reasons remain readable, including arbitrary
historical wording. They are represented internally as a legacy reason and keep
their original string representation when serialized, including through
import/export:

```json
{"kind":"omitted","reason":"Historical preview unavailable"}
```

New readers accept both shapes; older binaries that require a string reason
cannot read the new object shape. Do not downgrade a binary that needs to read sessions containing typed
reasons. No automatic on-disk session rewrite, database migration, approval
restoration, or crash recovery is introduced. Model-facing result content and
execution duration remain separate and unchanged.

See [the module structure proposal](docs/MODULE_STRUCTURE.md) for the broader
architecture and explicitly deferred work.
