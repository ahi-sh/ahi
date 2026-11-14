# ahi-durable

Shared execution records for ahi. This workspace crate currently owns:

- `ToolRecord`: the facts recorded for file writes, commands, skill reads, and delegation.
- `Ledger`: tool counts, outcome records, and `none` / `some` / `unknown` effect classification in turn results.

The tool executor produces records; the session service aggregates them. The crate depends on neither the executor nor the service. The existing JSON result format is unchanged.

This is the first extraction, not a crash-recovery implementation. Session storage, run admission, approval persistence, and restart recovery have not moved here yet. Tool execution, policy enforcement, provider adapters, and transport remain in ahi.

The crate is not published. Before publishing an ahi release that depends on it, review this API and enable and arrange dependency publication.

## Verification

```sh
cargo test -p ahi-durable
cargo test -p ahi --test exec
```

The unit tests cover effect accounting and content-free skill records. The executable integration tests exercise result documents, policy-gated tools, delegation, sessions, and audit records using local fixtures.
