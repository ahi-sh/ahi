# What the operator saw, what ran, what the model received

Five follow-ups to the [live GLM sweep](glm-flash-tool-sweep.md). These regressions use deterministic local fixtures: they can inspect a provider request or lose a reply at a chosen point without relying on a model to narrate what happened.

| Boundary | Contract | Regression evidence |
|---|---|---|
| Subprocess vs. turn | A program can exit unsuccessfully and the model can still complete the turn. New command results carry the process code (−1 for a signal); a process that ran and exited 126 is **not** a policy denial. Existing recordings aren't rewritten. | `a_child_exit_is_not_a_successful_command_or_a_policy_denial` runs two real programs, checks progress, stored session, result ledger and the correlated diagnostic in the next HTTP request. |
| Lost response vs. repeatable action | Reconnect may retry **setup**, not `turn.submit`. A submit has a caller-chosen run ID before the service sees it. A lost reply is not permission to start another run. After tool results, blank Enter does not replay the prompt in either engine or on resume. This is not automatic recovery of an unfinished response. | `losing_the_submit_reply_never_reconnects_and_submits_again` drops a Unix-socket reply. The `--post-tool-failure-only` PTY scenario breaks the HTTP response *after* a real file creation, verifies the write and correlated continuation request, then presses Enter in live and reopened chat. |
| Capture vs. model input | The model gets bounded `result.content`, not the terminal preview or rich detail. A local capture can contain text the model never got. Its header labels a turn-budget cutoff; expanding it does not resend anything. | `the_turn_output_bound_is_a_cap_including_diagnostics` checks beginning, middle and end markers on the actual wire, including a UTF-8 cutoff; `local_command_capture_is_labelled_even_when_collapsed_after_a_model_budget_cut` checks both display states. |
| Source vs. framing | A verbatim file can begin with `[lines 1-1 of 7]` and end with `… 2 more lines`. Those are numbered **source** lines. Page headers, continuation and preview-omission rows are not. Where truncated data makes a byte-paged range uncertain, show the preview without guessed line numbers. | `a_verbatim_file_can_contain_both_kinds_of_notices`, the paged gutter test, and wrapped-line copy tests. |
| Search text vs. clickable location | A result for `notes:2:old.txt` at line 7 must not open `notes` at line 2. New ambiguous path spellings are JSON-quoted; an ambiguous unquoted historical row has no click target. | `search_emits_unambiguous_paths_for_filenames_containing_colons` and `search_links_to_the_quoted_path_not_its_existing_prefix`. |

From the branch checkout:

```sh
cargo test --locked --workspace --all-features
cargo build --locked
uv run --offline scripts/smoke-model-picker.py --binary target/debug/ahi --post-tool-failure-only
```

The focused PTY check requires POSIX; it creates its own provider, socket, policy, session store and workspace. It does not use the operator’s model or credentials. The tests establish local behavior, not exactly-once execution by an external provider or automatic crash recovery.
