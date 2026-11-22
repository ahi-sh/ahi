# Shared prompt: wire contract and workflow evaluation (#125)

**Offline contract.** `src/system_prompt.rs` is the single source of instructions. `cargo test --locked --workspace --all-features` inspects HTTP bodies on loopback: initial and tool-result requests have one system message (Chat Completions), `instructions` (gateway and browser-login Responses), or `system` (Anthropic). A result stays attached to its original call ID. These instructions are request framing, not an operator message, skill load, tool approval or durable session entry. The full `smoke-model-picker.py` PTY checks cover initial/resumed history and exact byte limits with the prompt included.

**Workflow harness, not a model benchmark.** Build a pre-prompt binary from the merge of #124 (`d44f2f0`) and the candidate binary. On Unix:

```sh
python3 scripts/eval-core-prompt.py --baseline /path/to/pre-prompt/ahi \
  --candidate target/debug/ahi --out target/prompt-eval-offline
```

By default two real `ahi exec --standalone` binaries talk to a **scripted loopback provider**. The scripted comparison passed all four cases on both binaries (repository fact, scoped edit, catalog without loads, explicit preload). This verifies the apparatus, not that a model answers better. No paid inference was used for these results.

To run the same comparison against a model, explicitly add `--live-url <gateway-base-url> --model <served-id>` and set `AHI_EVAL_API_KEY` if needed. Use HTTPS except on loopback. Use a fresh `--out` directory; the runner will not overwrite a previous comparison. No real skill bodies, operator configuration or prior sessions are loaded. Only the comparison process can see the upstream credential; neither ahi nor its tools inherit it.

Each arm gets fresh workspace bytes, state, and synthetic HOME at the same absolute paths. It uses the same model and requested thinking setting (`off`), tools, prompts, markers, per-request cap (768 tokens), and ceiling of five forwarded requests per case (40 total). There is no evaluation retry. The script compares initial request controls including schemas; it does not change serving limits. An ephemeral script is the only permitted command and exits 7 with a fresh diagnostic. The scorer checks correlated tool output, the answer, the workspace bytes including an unrelated change, the catalog actually advertised, and the explicit preload's successful result. For unambiguous automatic scoring the edit report and loaded IDs are requested as JSON; other prose or tool order is not scored.

Private local artifacts are one outbound body and one field-separated choice stream per round (`*.request.json`, `*.delta.jsonl`), plus a score with the answer and synthetic tool results. Inspect failures, including formatting misses, before interpreting scores. Do not post raw model streams or sessions in issues. Even a passing live pair would be a sample, not proof of deterministic behavior or an improvement due to the prompt.
