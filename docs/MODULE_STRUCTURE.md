# Proposed ahi module and workspace structure

Planning reference for [issue #98](https://github.com/iancleary/ahi/issues/98).
This describes a target layout, not an approved bulk migration.

## Implementation status

The first implementation slice establishes `tools`, `tool_policy`, `cli::tool_policy`, and `precommit`, adds the canonical `ahi precommit` CLI with temporary deprecated aliases, and stores typed tool-omission reasons with legacy-string read compatibility. Tool implementations have one file per tool name; common lexical argument preparation and schema construction remain centralized in `tools::mod` for now. CLI entry wiring has moved out of `main.rs`, with precommit-specific handling in `cli::precommit`.

The next slice implements `terminal_host::{rex,hc}` and an operator-only `ahi terminal-host` CLI, and removes delegation's invalid implicit hc connector. The adapters are tested with disposable native-CLI fixtures; live backend interoperability is not yet verified. Runtime/provider extraction, execution/containment restructuring, TUI consolidation, and session-store extraction remain proposals. Existing shared environment, rule-identifier, and audit helpers still reside under `precommit` and have non-precommit callers; the follow-up boundary work must assign those helpers a shared owner rather than duplicate them. See [MIGRATING.md](../MIGRATING.md) for public-command and stored-record compatibility.

## Design decisions

- Use Rust modules inside the main `ahi` crate by default. Extract a crate only for a demonstrated dependency, safety, or reuse boundary.
- Keep the existing `ahi-durable`, `ahi-widgets`, `ahi-unix`, and `ahi-win32` crates. Do not introduce a generic `ahi-core` or a crate for every subsystem.
- No plugin system, general workflow framework, or new coordinator/node binaries.
- Use temporary deprecated CLI aliases with actionable migration guidance when moving commands into the `precommit` namespace. Do not silently break existing automation or installed Git shims.
- `terminal_host` is the interface to external terminal multiplexers: Rex is primary, and hc is a supported alternative. External servers own terminal lifetime. This is not a TUI backend, UI-client manager, or agent orchestrator.
- Separate the agent runtime from terminal chat presentation.
- Name model-callable capabilities `tools`, their authorization subsystem `tool_policy`, and the operator-facing policy commands `cli::tool_policy`.
- Name the pre-commit-compatible runner `precommit`, not the ambiguous `hooks`. Git hooks are entry points that invoke that runner.
- Separate behavior-preserving moves from changes to public commands, persistent formats, authorization, recovery, and containment.

The tools, precommit, and terminal_host naming and temporary CLI deprecation policy are agreed. Other names and ownership boundaries below are proposals for review. Files identify responsibilities; a small module can remain a single `.rs` file until subdivision is useful. Do not create empty scaffolding merely to match this tree.

## Workspace

The repository root remains the main `ahi` package and workspace root. Its `src/` directory is the application source; no additional nested `ahi/` package is proposed.

```text
ahi/
├── Cargo.toml                         # Main library + binary; workspace members
├── Cargo.lock
├── src/                               # Main ahi crate; detailed below
├── crates/
│   ├── ahi-durable/
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session.rs             # Persistent session identity and metadata
│   │       ├── entry.rs               # Stored entries and format validation
│   │       ├── store.rs               # Read, append, validate, reopen
│   │       ├── ledger.rs
│   │       ├── tool_record.rs
│   │       └── tool_presentation.rs
│   ├── ahi-widgets/                   # Existing reusable terminal drawing crate
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ahi-unix/                      # Existing narrow Unix platform boundary
│   │   ├── Cargo.toml
│   │   └── src/
│   └── ahi-win32/                     # Existing narrow Windows platform boundary
│       ├── Cargo.toml
│       └── src/
├── tests/                             # Cross-module, CLI, service, storage tests
├── examples/                          # Reviewed configuration and usage examples
├── scripts/                           # Maintained verification/release tooling
└── docs/
    └── MODULE_STRUCTURE.md
```

Existing workspace files not shown are retained. This proposal does not redesign the internal layouts of the widget or platform crates.

`ahi-durable` currently contains the ledger, tool records, and tool-presentation types. Its session/entry/store files above are proposed extractions from `src/session/`, not existing functionality. Conversation resume is not crash recovery.

## Main crate: `src/`

```text
src/
├── main.rs                            # Parse, dispatch, map result to exit status
├── lib.rs                             # Deliberate public library surface
│
├── cli/                               # CLI adapters and application wiring
│   ├── mod.rs
│   ├── args.rs                        # Command definitions and options
│   ├── exec.rs                        # Headless turn command and output
│   ├── serve.rs                       # Server lifecycle/connect commands
│   ├── session.rs                     # Session list/show/import/export commands
│   ├── precommit.rs                   # Precommit operator commands
│   ├── tool_policy.rs                 # Inspect/create tool policy; not execution
│   ├── terminal_host.rs               # Operator adapter for native terminal hosting
│   ├── dev.rs                         # Development environment commands
│   └── update.rs                      # Update command adapter
│
├── agent/                             # UI-independent turn execution
│   ├── mod.rs
│   ├── turn.rs                        # Model/tool iteration and completion
│   ├── model.rs                       # Internal model request/response contract
│   ├── tool_call.rs                   # Runtime tool-call exchange, if needed
│   ├── events.rs                      # Typed execution events, not rendered rows
│   ├── approval.rs                    # Approval exchange and waiting
│   └── context.rs                     # Construct model-visible conversation
│
├── providers/                         # Concrete model-service integration
│   ├── mod.rs
│   ├── catalog.rs                     # Discovery and typed model metadata
│   ├── credentials.rs                 # Credential loading and auth lifecycle
│   ├── gateway.rs                     # Gateway integration
│   ├── openai.rs                      # OpenAI integration
│   ├── protocol/                      # Chat Completions/Responses encoding
│   │   └── mod.rs
│   └── http/                          # HTTP and stream parsing, including SSE
│       └── mod.rs
│
├── tools/                             # Compiled-in model-callable capabilities
│   ├── mod.rs                         # Definitions and dispatch
│   ├── read_file.rs
│   ├── list_dir.rs
│   ├── find_files.rs
│   ├── search_files.rs
│   ├── write_file.rs
│   ├── edit_file.rs
│   ├── run_command.rs
│   ├── delegate.rs
│   ├── load_skill.rs
│   └── helpers/                       # Shared workspace/file operations, not policy tools
│       ├── mod.rs
│       ├── workspace.rs               # Path rules and shared read-opening API
│       ├── walk.rs
│       ├── glob.rs
│       └── ignore.rs
│
├── tool_policy/                       # Authorization for model-requested actions
│   ├── mod.rs
│   ├── config.rs                      # Parse/validate ahi.tools.yaml
│   ├── evaluate.rs                    # Evaluate proposed actions
│   ├── grants.rs                      # Scoped session permissions
│   └── examples.rs                    # Shipped policy templates
│
├── precommit/                         # Pre-commit-compatible checks and runner
│   ├── mod.rs
│   ├── config.rs                      # .pre-commit-config.yaml
│   ├── checks/                        # Built-in validation and fixing checks
│   │   └── mod.rs
│   ├── runner.rs                      # Select/run checks and collect outcomes
│   ├── policy.rs                      # ahi.hooks.yaml; local command authority
│   ├── local_hook.rs                  # Configured local hook execution adapter
│   ├── git.rs                         # Repository and staged-file operations
│   ├── install.rs                     # Git shim install/remove, when implemented
│   ├── filter.rs                       # File and stage selection
│   ├── identify.rs                    # File type identification
│   ├── identify_tables.rs
│   ├── paths.rs                       # Precommit-specific path handling
│   ├── migrate.rs                     # Configuration migration/suggestions
│   ├── audit.rs                       # Hook execution audit records
│   ├── output.rs                      # Precommit report formatting
│   └── store.rs                       # Precommit-specific stored state
│
├── skills/                            # Resource discovery/validation/loading
│   └── mod.rs
│
├── session/                           # Application-side session integration
│   ├── mod.rs
│   ├── recording.rs                   # Execution events → persistent records
│   ├── restore.rs                     # Persistent records → runtime context
│   ├── protocol.rs                    # Session-service wire contract
│   ├── client.rs                      # Local/remote client, no server startup
│   └── transport.rs                   # Client socket/stdio connection mechanics
│
├── service/                           # Server implementation and process lifetime
│   ├── mod.rs
│   ├── server.rs                      # Requests, subscriptions, turn supervision
│   ├── lifecycle.rs                   # Start, discover, stop, shutdown
│   └── transport.rs                   # Listener and server connection handling
│
├── terminal_host/                     # Interface to external terminal multiplexers
│   ├── mod.rs                         # Targets, handles, operations, observed results
│   ├── rex.rs                         # Concrete Rex CLI/API mapping
│   ├── hc.rs                          # Concrete hc JSON-RPC mapping
│   └── transport.rs                   # Bounded native CLI calls, no retry/failover
├── program_status.rs                  # Backend-neutral advisory OSC 7501 reporting
│
├── execution/                         # Execution mechanics, not authorization
│   ├── mod.rs
│   ├── process.rs                     # Spawn, bounded output, timeout, cancellation
│   ├── environment.rs                 # Environment construction and scrubbing
│   └── sandbox.rs                     # Containment capabilities/checks
│
├── dev/                               # Provision and enter development environments
│   ├── mod.rs
│   ├── config.rs                      # ahi.dev.yaml
│   └── container.rs                   # Existing container launch integration
│
├── tui/                               # All application terminal UI
│   ├── mod.rs                         # Terminal UI startup and composition
│   ├── appearance.rs                  # Typed appearance used by the UI
│   ├── commands.rs                    # Shared Commands popup
│   ├── gateway.rs                     # Gateway onboarding UI
│   ├── login.rs                       # Login UI; auth mechanics stay in providers
│   └── chat/
│       ├── mod.rs                     # Screen state and event handling
│       ├── approval.rs                # Approval presentation and input
│       ├── command.rs                 # Chat slash commands
│       ├── header.rs
│       ├── input.rs
│       ├── key_binding.rs
│       ├── location.rs
│       ├── open.rs
│       ├── preferences.rs
│       ├── scroll_navigation.rs
│       ├── status.rs
│       ├── thinking.rs
│       ├── usage.rs
│       ├── selection/
│       │   └── mod.rs
│       └── transcript/
│           ├── mod.rs
│           ├── format.rs
│           ├── render.rs
│           └── theme.rs
│
├── platform/                          # Safe application platform adapters
│   ├── mod.rs
│   ├── terminal.rs
│   └── clipboard/
│       └── mod.rs
│
├── config/                            # Application locations/configuration support
│   ├── mod.rs
│   ├── directories.rs
│   └── yaml.rs                        # Shared safe YAML loading primitives
│
├── update.rs                          # Update implementation, separate from CLI
├── error.rs
└── util/                              # Small domain-independent helpers only
    ├── mod.rs
    ├── base64.rs
    ├── localtime.rs
    ├── rfc3339.rs
    ├── nofollow.rs
    └── shlex.rs
```

Unit tests remain beside their owners; the tree omits most test modules and individual check/provider implementations. Splitting client and server transport responsibilities does not require duplicating shared framing helpers: place any genuinely shared wire helpers with the session protocol.

## Vocabulary and ownership

### Tools and tool policy

- **Tool:** a compiled-in capability exposed to a model.
- **Tool call:** a request to invoke a tool with particular arguments.
- **Tool policy:** operator-authored rules governing those calls.
- **Authorization decision:** allow, deny, or require approval for a proposed action.
- **Approval:** a human response, potentially establishing a bounded session grant.

`tools` implements capabilities, with one implementation file per model-facing tool name. These are tool names, not policy action names: `file_write`, for example, can govern both `write_file` and `edit_file`. Each tool module owns its argument shape, validation, operation, and structured result. Shared authorization, path safety, atomic-write mechanics, and process handling remain shared rather than being duplicated in every tool.

`read_file` retrieves content; `write_file` creates or explicitly replaces a file; `edit_file` applies targeted replacements. Keep these implementations separate while sharing safe file-writing machinery where appropriate.

`tool_policy` decides whether proposed actions are authorized. `cli::tool_policy` lets the operator inspect or create policy. It does not implement tools or own the policy evaluator.

The current `src/tools.rs` is an operator-facing policy command implementation, not the tool engine. Its command/output handling belongs in `cli::tool_policy`; its reusable policy examples belong in `tool_policy::examples`. Actual tool implementations move from `src/chat/backend/tools/` to `src/tools/`.

Do not replace `tool_policy` with a broad `policy` directory that implies unified authority over tools, hooks, providers, and containers. Hook-specific policy remains under `precommit`.

### Precommit and Git hooks

- **Precommit:** ahi's pre-commit-compatible runner and checks.
- **Git hook:** a Git lifecycle entry point, such as `pre-commit` or `pre-push`.
- **Check:** a built-in validation or fixing operation.
- **Hook policy:** authorization for configured local hook commands.

The `precommit` name does not restrict the subsystem to Git's `pre-commit` stage. Supported stages and their argument/stdin contracts must be documented and tested; the name does not promise complete upstream compatibility.

### Runtime and interfaces

`agent` owns a turn's lifecycle, model/tool iteration, approval waits, cancellation, and typed events. It must not import the terminal UI or concrete HTTP adapters. `providers` and `tools` implement the internal contracts needed by the runtime; application wiring selects implementations. These are ordinary internal Rust interfaces, not extension/plugin APIs.

The CLI, TUI, and service are entry points to that runtime. They must not contain separate implementations of the turn loop. The TUI presents approval requests; policy owns the authority of the answer and the executor enforces it before effects occur.

### Persistence

`ahi-durable` owns persistent representations and storage. `ahi::session` owns conversion to and from application execution and the service-facing session API.

`tool_presentation.rs` contains a persisted data model, not a rendering adapter: structured tool details and execution duration, without widgets, colors, layout, or diff rendering. Details that cannot be retained use a typed omission reason; application presentation code supplies the human-readable wording. Historical string reasons remain readable and preserve their original text through an explicit legacy variant. Newly written typed reasons require a reader that supports the object representation; see the migration guide before downgrading.

The store receives its location from the application. It does not discover global configuration, contact a provider, or create TUI rows. Transcript, runtime context, and wire responses may be different projections of the same stored records; they must not become competing authoritative stores.

Session extraction does not choose a new database or introduce automatic replay, recovery, or restored approval grants. Those require separate format and behavior decisions.

### External terminal hosts

`terminal_host` requests and observes execution in external terminals. The TUI presents ahi; the terminal host provides the terminal in which ahi, a shell, or another program runs. Rex and hc do not have identical APIs, and neither is an ahi session-protocol connector.

Three boundaries stay separate:

1. **Terminal control:** `terminal_host::{rex,hc}` starts programs, inspects process evidence, captures bounded snapshots, sends input/keys, interrupts, and requests termination. Handles include the backend and execution namespace; native terminal IDs are distinct from ahi session IDs. Native Windows and WSL require separately selected targets. Local hc handles must not be moved to another machine's core.
2. **Structured agent communication:** delegation uses ahi's session API through an explicitly configured connector such as `ssh {worker} ahi connect`. `hc connect` carries hc JSON-RPC, not ahi's methods. Removing the nonexistent `hc connect --worker` default resolves the invalid assumption in #75; automatic label discovery remains unimplemented. The service client still lives under `service`; its proposed move is separate work.
3. **Advisory reporting:** `program_status` emits OSC 7501 without backend-specific adapters or negotiation. Reports do not authorize actions or prove process completion. Live receipt by either backend remains to be checked separately.

The implemented operator command defaults to Rex; hc requires `--backend hc`. Every call explicitly selects a server/core and session. There is no availability fallback or target configuration file in this slice. Calls do not retry, including after uncertain creation or partial submission. `--dry-run` validates and prints native requests without launching a process. Model tools and their authorization remain unchanged; terminal hosting is not exposed to the model.

| Responsibility | Rex mapping | hc mapping |
| --- | --- | --- |
| Start | `session.new_window`, one direct terminal block, no focus request, retain after completion | `terminal.run`, literal absolute executable argv, no parent/notifications, no keep/restart, retain record |
| Inspect | Native terminal `process`; no child and no exit is pending, not exited | `terminal.get`; lost-holder evidence remains unknown |
| Capture | CLI `capture --format=text`, screen and scrollback; completeness unknown | `terminal.read`, screen snapshot with native completeness metadata |
| Text / submit | CLI `send` with literal stdin; submit then sends Enter as a separate acknowledged call | `terminal.send` includes Enter and draft protection; raw text is explicitly unsupported |
| Keys / interrupt | Named keys; interrupt sends Ctrl+C without claiming exit | `terminal.keys`; `terminal.interrupt` chooses its stop key and reports an observation |
| Terminate | `block.close` for exactly one block, never session-wide `kill` | `terminal.kill` ends the program but retains its terminal record |

A Rex terminal needs a containing window; creating that resource is not a pane-layout or UI-client feature. The module does not attach graphical clients, arrange panes, request focus, install servers, manage host registries, or schedule agents. The hc CLI retains its own native core-start behavior when actually invoked; ahi does not add server lifecycle commands. Connector timeout cleanup is not containment of remote programs.

Contract sources: hc 0.1.21's installed `hc schema`; Rex's [CLI](https://www.superlogical.com/rex/docs/reference/cli) and [API](https://www.superlogical.com/rex/docs/reference/api) references, with native terminal payload shapes also inspected in the existing local Rex integration. Tests use synthetic responses matching those shapes, not live captures. Run `cargo test --locked --lib terminal_host` and `cargo test --locked --test terminal_host` to replay them. Live Rex beta/schema acceptance and hc checks on supported execution environments are remaining acceptance work, requiring explicit service access authorization.

### Execution and development environments

`execution` owns execution mechanics; it is not an alternate authorization system. Tool and precommit adapters supply appropriately authorized work and environment constraints. Factor genuinely shared process behavior without weakening either caller's trust boundary.

`dev` provisions and enters an environment. Containment governs what the executing process can access: files, processes, network, and credentials. A container launcher or in-process path check is not by itself a complete security boundary.

When tools run in `ahi serve`, that service must be inside the intended containment boundary. Isolating only its TUI client does not isolate tool execution. The proposed `sandbox` module does not imply that all these controls exist today.

### Configuration and presentation

Configuration schemas stay with their owners: tool policy with `tool_policy`, precommit configuration with `precommit`, development configuration with `dev`, and appearance types with `tui`. The root `config` module provides application locations and shared loading primitives, not a giant schema for every subsystem.

The application supplies appearance settings to the TUI; reusable widgets do not read application configuration. Terminal-independent CLI output must not import `tui::chat` for colors or formatting.

## Proposed public command organization

Rust module moves and public CLI changes are separate work.

```text
ahi tools init / examples / ...        → cli::tool_policy
ahi precommit run                     → cli::precommit
ahi precommit run --all-files          → cli::precommit
ahi precommit run --hook-stage ...     → cli::precommit
ahi precommit install                 → cli::precommit, when implemented
ahi precommit uninstall               → cli::precommit, when implemented
ahi precommit migrate                 → cli::precommit
```

- Keep the existing `ahi tools` public spelling for now; its internal owner need not share its name. A future `ahi tools policy` namespace is a separate decision.
- Make `ahi precommit` the canonical runner namespace. Keep existing `ahi run`, `ahi migrate`, and other relocated commands as temporary deprecated aliases, with the same arguments, behavior, stdout, and exit status. Both spellings must dispatch to the same implementation.
- Deprecated aliases print concise migration guidance to stderr, including the equivalent canonical invocation. Keep warnings out of stdout so pipelines and machine-readable output remain usable. New documentation, examples, and generated Git shims use the canonical commands.
- Publish the command mapping and removal schedule in migration documentation and release notes before removing aliases. Retain aliases through at least two subsequent minor releases after introduction, and announce an explicit removal version in an earlier release. Aliases are not permanent compatibility APIs; they do not expire automatically at runtime.
- Migration must cover scripts, CI configuration, and installed Git shims. Identify old invocations and explain their replacements. Any automated rewrite must be explicit, offer a dry-run preview, and avoid overwriting unrelated user content. Installing a new version must not silently rewrite repositories.
- Verify old/new command equivalence, noninteractive use, preserved stdout and exit status, actionable stderr warnings, and the migration path for any affected ahi-owned Git shims. Hook installation is currently unimplemented; do not assume such shims already exist.
- Keep `.pre-commit-config.yaml`, the compatibility contract.
- Keep `ahi.hooks.yaml` for now; it accurately names hook-execution policy. Do not silently rename configuration files during module moves.
- Keep Git hook installation separate from the rename. Current install/uninstall commands are unimplemented. Installation must honor Git's hook location, preserve unrelated hooks, forward supported stage arguments/input, preserve failure status, work noninteractively, and support idempotent operation and dry-run previews.

## Dependency rules

1. Headless execution and service execution do not import TUI modules.
2. The runtime depends on internal model/tool contracts, not concrete provider HTTP implementations.
3. Widgets know nothing about sessions, credentials, or execution policy.
4. Persistent records contain no terminal rendering types.
5. Provider adapters do not control the tool loop.
6. Tool execution has no model-accessible path around authorization and enforcement.
7. Skills, model output, and tool results cannot create authority.
8. The session client does not depend on server startup or the server implementation.
9. Terminal-host control, ahi session transport, and advisory program status remain separate; none grows into an embedded coordinator.
10. A new crate requires a demonstrated boundary, not naming symmetry.

## Migration assessment

| Current owner | Proposed owner |
| --- | --- |
| `src/chat/backend/tool_loop.rs` and runtime control | `src/agent/` |
| `src/chat/backend/http`, `protocol`, `sse.rs` | `src/providers/` |
| `src/harness/` | `src/providers/`, divided by responsibility |
| `src/chat/backend/tools/` | `src/tools/` |
| `src/tool_policy.rs` | `src/tool_policy/` |
| `src/tools.rs` | `src/cli/tool_policy.rs` plus reusable policy examples |
| `src/hooks/` | `src/precommit/` |
| Precommit dispatch and command logic in `main.rs` | `src/cli/precommit.rs` |
| `src/chat/` presentation and interaction | `src/tui/chat/` |
| `src/program_status.rs` | Remains backend-neutral; no terminal-host dependency |
| Implicit worker connector inside delegation | Removed; explicit ahi-protocol connector required |
| External terminal control | Implemented in `src/terminal_host/`, operator CLI in `src/cli/terminal_host.rs` |
| Persistent types/store in `src/session/` | `crates/ahi-durable/` |
| Session recorder and runtime conversion | `src/session/` |
| Service wire/client code | `src/session/protocol.rs`, `client.rs`, `transport.rs` |
| `src/serve.rs` | CLI adapter plus `src/service/lifecycle.rs` |
| Headless command behavior in `src/exec.rs` | CLI adapter; shared execution remains in `agent` |
| `src/sandbox.rs` | `src/execution/sandbox.rs` |
| `src/dev.rs` | `src/dev/`, with CLI handling separated |
| `src/term.rs`, `src/clipboard/` | `src/platform/` |

Sequence approved moves in small units, migrate callers directly, and verify each unit against existing behavior. Do not retain internal compatibility modules solely to preserve the old directory layout. Public Rust APIs, CLI contracts, and persistent formats need explicit compatibility assessment before removal.

## Open decisions before implementation

- Confirm the runtime's internal model/tool contracts and ownership of shared request/event types; avoid cycles between adapters and runtime implementations.
- Verify the native adapters against authorized live Rex/hc installations. Do not equate passing synthetic contracts with live compatibility.
- Consider target configuration or pre-operation availability selection only if needed; never fail over an operation already started. Focus, UI-client management, and orchestration remain out of scope.
- Decide whether the next development-environment milestone is reproducibility, enforced containment, or both.
- Assign shared environment construction, identifier validation, and execution-audit helpers to their actual owners; their current reuse from `precommit` is not the desired end-state layering.
- Define which persistent types and storage operations move into `ahi-durable` in the next extraction, without introducing new recovery semantics.
- Record the introduction and removal versions for the temporary deprecated CLI aliases when scheduling releases, honoring the documented minimum support window.
- Reconcile this module-first plan with the earlier crate-oriented proposal in issue #56. That issue's new crates, binaries, and versioning changes are not prerequisites here.
