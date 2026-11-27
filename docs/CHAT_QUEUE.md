# Chat queue

The queue holds prompts submitted while a chat turn is active. Each pending prompt starts a separate turn after the previous turn completes. For example, submit `Inspect the parser`, then `Explain the error handling`, then `Suggest a smaller interface`. The second and third prompts wait in that order. Each receives the conversation produced by the preceding turns.

The panel above the composer shows the next three prompts and the number of remaining prompts. Press **Alt+Q** to open the manager. The manager suspends automatic dispatch. The active turn can continue while the panel is open. The transcript and composer draft remain visible.

## Controls

| Key | Action |
| --- | --- |
| Up / Down | Select a pending prompt. |
| Home / End | Select the first or last prompt. |
| Page Up / Page Down | Move selection by one visible page. |
| Alt+Up / Alt+Down | Move the selected prompt earlier or later. Selection follows that prompt. |
| Enter | Edit the saved prompt. Enter saves; Esc discards the edit. |
| Alt/Option+Enter | Start a blank replacement for the selected prompt. The saved prompt stays intact until Enter saves the replacement. |
| Shift+Enter | Insert a newline in the editor. Alt+Enter also inserts a newline once editing has started. |
| Delete | Remove the selected prompt. |
| c, then y | Remove all pending prompts. Any other confirmation key cancels. |
| p | Pause automatic dispatch. The active turn continues. |
| r | Resume the queue and close the manager. |
| ? | Show controls. Up/Down and page keys scroll the help; ? or Esc returns to the queue. |
| Esc / Alt+Q | Close the manager without changing the pause state. In the editor, Esc first discards the edit. |

The panel uses at most half the terminal height. Long queues scroll to keep the selection visible. Small terminals use a compact list and footer. Previews show the first nonblank line of each prompt. Open the editor to inspect the full text. Invalid saves show a reason and keep the edit open.

## Dispatch and recovery

New prompts join the end of any pending queue, including a paused queue. Slash commands remain local actions and never join it. Editing or reordering a prompt retains its selected skills. Dispatch uses the execution mode selected when the prompt leaves the queue. It waits while a mode dialog is open so that the operator can finish that selection.

Dispatch rechecks workspace policy availability and the session budget. A refused admission retains the pending prompt and pauses the queue. Restore the policy or start a new session with `/clear` if the session budget is exhausted. Review pending work before resuming.

A failed or cancelled turn pauses pending prompts. After an eligible connection failure, blank Enter retries the current prompt even when pending prompts exist. That retry keeps the queue paused. Press **Alt+Q**, then **r** to resume pending work. A partial answer or returned tool result does not offer the same automatic resend action.

There are at most 64 pending prompts, each limited to 64 KiB of UTF-8 text. An edit must contain nonblank text and must not begin with a slash command. Rejected admission preserves the composer draft. Rejected edits preserve the saved prompt. Pending text enters model history and the session record only when dispatched.

The queue exists only in the current chat process. Closing chat loses pending prompts. `/clear` removes pending prompts with the conversation. Clearing pending prompts in the manager preserves the conversation and composer draft.

## Verification

From the checkout, build the binary and run the focused terminal workflow:

```sh
cargo build --locked --bin ahi
uv run scripts/smoke-model-picker.py \
  --binary target/debug/ahi \
  --queue-manager-only \
  --capture-directory target/chat-queue-captures
```

The workflow uses disposable session data and a loopback HTTP fixture. It exercises both standalone and service-backed chat. It verifies the actual model request order and saved operator messages after edits and reordering. Terminal captures cover 80×24 and 40×12 layouts. No external model service or personal credentials are required. The host must permit local HTTP listeners, Unix sockets, and pseudo-terminals.

Queue interaction and drawing are in [queue.rs](../src/chat/queue.rs). Admission, turn completion, and dispatch are in [chat/mod.rs](../src/chat/mod.rs). The terminal workflow is maintained in [smoke-model-picker.py](../scripts/smoke-model-picker.py).
