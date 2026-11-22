# GLM Flash tool and terminal acceptance — 2026-11-21

**30/30 behavior checks passed** in a single post-fix sweep: ten fresh model turns, 26 recorded tool results, eight tool names, one model (`home/GLM-5.3-Flash-EXL3`), macOS arm64. This is not a count of bugs or a pass-rate estimate for future model runs. After the final wording changes, both affected groups ran again: 9/9. A separate live standalone TUI turn was observed at 100×36 and 62×24.

Follow-up: [tool results across the TUI, model request, and session store](tool-interface-boundaries.md) documents the five interface regressions and their deterministic checks.

## What actually broke

| Area | Reproduction and observation | Fix |
|---|---|---|
| Search | `^red$` missed the middle two lines of a text file. The regex ran against the whole file with multiline anchors disabled. | Use line anchors, including CRLF. A terminating newline does not invent a last empty line for `^$`. Regression tested on CRLF as well as live LF input. |
| Read continuation | Reading a 65,537-character line reported success, `[lines 1-0 of 2]`, and `continue with offset=1`. Following it could never make progress. | Return an actionable error if the first requested line cannot fit. Do not emit half a line. An explicit request for the next, shorter line still succeeds. |
| Empty file | Offset 1 produced `[lines 1-0 of 0]`; offset 2 on the same empty file **succeeded** with `[lines 2-0 of 0]`. | An explicitly paged empty file has an empty-file notice and zero source lines. Offsets past its first position fail. An ordinary whole-file read is still byte-for-byte empty. |
| Read summary and gutter | A single source line at offset 2 acquired a recorded count of 3 (header + data + continuation). The TUI could claim it read lines 2–4. Offset without `limit` also lost its starting position in the summary. | Record source rows when reading, not by counting the formatted result. Show `line 2` or the actual range. Show an empty file without a fictitious line 1. Treat a literal line beginning with `…` as file data unless it matches the preview notice. |
| Find/search counts | `[no matches]` was counted as a match. A limited search could count a continuation or binary-skip notice as another hit. | Return the number of rows shown from the search/find implementation, alongside the text. An empty result now records 0, and a limited single hit records 1. |
| Edit feedback | The schema promised the occurrence count, but the ambiguous edit only said “more than once”. | Report the actual count, e.g. `2 occurrences`, without echoing the matched text. Retain both recovery options. The original file stays unchanged until a valid edit is issued. |
| Tool wording | Single-item outputs said `1 lines`, `1 more lines`, and `1 more matches`. | Correct the create/overwrite, paging, and find/search notices. |
| Floating jump controls | In the live 62-column chat, `↑ Top (Ctrl+K)` painted over the right end of the top transcript row. The durable transcript had all the text. | Use the full label only in blank cells; try the compact label, otherwise omit the button and its hit target. Keyboard shortcuts still work. A terminal-buffer regression covers a wide glyph at the edge. |
| Workspace status | Left truncation of a wide or combining path used character count, not terminal cells; clipping could lose the rightmost path or branch text. | Clip whole grapheme clusters by display width, retaining suffix styles. Covered by narrow CJK/combining tests and the existing styled-branch check. |
| Other expanded details | A successful filename containing `: ` could be split as an error reason. A truncated short command capture could say “0 more lines shown to the model”, implying knowledge of what the model saw. | Parse the reason separator only on failures. Distinguish captured rows from truncation and print `… output truncated` when there are no additional captured rows to expand. Covered by formatter tests; these were not live provider failures. |

## How the 30 checks were scored

The [runner](../../scripts/probe-live-tools.py) creates a disposable workspace, constrained external policy and separate session store. Only the selected provider/model configuration is copied into private temporary state. No shell, arbitrary command, external worker, or operator skill is granted. It asks the model in natural language and pairs each real assistant call with its tool result by call ID in ahi's durable recording. Hidden markers are generated per run; write assertions check actual file bytes. Expected tool refusals are successes **only when the tool really refused**.

| Group | Checks | Evidence |
|---|---:|---|
| Discover | 4 | Sorted listing and directory suffix; nested glob excludes ignored decoy; Unicode filename yields exact bytes; unprompted random marker reaches final answer. |
| Search | 5 | Case-insensitive literal with a glob; anchored regex with limit 1; no-match sentinel; zero recorded hits; limited result has one hit despite the continuation note. |
| Page / empty / oversized | 7 | Exact returned window; correct recorded line count; ordinary EOF refusal; empty first page; empty-file EOF refusal; oversized first line refuses instead of looping; next small line can be read. |
| Create / edit / refuse | 10 | Create; edit; verify bytes on disk and via read-back; effects recorded; ambiguous edit refused; actual count and recovery guidance; replace-all preserves surrounding bytes; no implicit overwrite; overwrite cannot create; failed writes have no file effects. |
| Command | 2 | Actual `pwd` from `docs/sub`; `printf` receives `two words` as a single argument. No shell. |
| Skill | 2 | Model called `load_skill` on `repo/probe`; independently generated marker returned in the answer. |

The exploratory baseline was **21/28**, not 21/30: the two search-presentation checks were added after inspection. One miss wasn't a harness defect: the model listed the workspace root rather than `docs`. Making the two requested directory scopes explicit fixed the prompt; we did not change `list_dir` or accept the wrong output. Negative cases require a matching tool result, not that the model make exactly the minimum number of calls.

## Terminal and reproduction

The installed CLI accepts `-p` under **`exec`**, not at the root. The runner executes the branch using `cargo run --quiet --locked -- --state-dir <scratch-state> exec -p <prompt> --model <model> --workdir <scratch-work> --tools <scratch-policy> --standalone …`. To rerun (**sends live model requests**):

```sh
python3 scripts/probe-live-tools.py --live --model home/GLM-5.3-Flash-EXL3
# Smaller repro, two independent turns:
python3 scripts/probe-live-tools.py --live --model home/GLM-5.3-Flash-EXL3 --group page --group oversize
```

For the live TUI check, we resumed the initial exec session in a private tmux server, asked GLM to read line 2 from `notes α.txt`, inspected the collapsed row, expanded with Ctrl+O, resized to 62×24, quit, and reopened the recording without submitting another prompt. The tool header showed `line 2`, the gutter showed `2 Row <marker>`, and the final answer matched. Separate offline PTYs exercised both standalone and service-backed resumed tool views and the full picker workflow. Local captures: `target/probe-tool-render/`; local run logs: `target/live-tools-{before,final,wording}.log`. These generated artifacts are not published.

**Limits:** the sweep did not exercise remote delegation, Full Access, interactive approval for arbitrary commands, another provider, native Windows, or Rex-specific rendering. The 30 checks verify tool behavior, not throughput or the model's reliability across repeated samples.
