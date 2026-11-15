# ahi-durable

Shared execution records for ahi. This workspace crate currently owns:

- `ToolRecord`: the facts recorded for file writes, commands, skill reads, and delegation.
- `Ledger`: tool counts, outcome records, and `none` / `some` / `unknown` effect classification in turn results.
- `ToolDetail` and `ToolPresentation`: serialized human-facing tool metadata and duration, distinct from model-facing content. Details larger than 512 KiB become an explicit omission marker.

The tool executor produces records; the session service aggregates them. The crate depends on neither the executor nor the service. The effect-ledger JSON format is unchanged. Ahi session tool messages can carry an optional `presentation` object with a tagged `detail` and an `elapsed` duration expressed as seconds and nanoseconds.

This is the first extraction, not a crash-recovery implementation. Session storage, run admission, approval persistence, and restart recovery have not moved here yet. Tool execution, policy enforcement, provider adapters, and transport remain in ahi.

The crate has not been published yet. It participates in workspace packaging so Cargo can verify ahi against the local staged dependency. Publication remains a separate release operation; review this API and publish dependencies before ahi.

## Verification

```sh
cargo test -p ahi-durable
cargo test -p ahi --test exec
```

The unit tests cover effect accounting and content-free skill records. The executable integration tests exercise result documents, policy-gated tools, delegation, sessions, and audit records using local fixtures.
