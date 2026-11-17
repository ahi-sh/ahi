# Shared skill runtime

Skills are bounded instructions and supporting text, not permission grants.
The TUI, `ahi exec` (including `--headless`), and the session service use the
same discovery and loading implementation.

## Discovery and metadata

ahi discovers `<workspace>/.agents/skills/<name>/SKILL.md` and
`<user home>/.agents/skills/<name>/SKILL.md`. The OS resolves the home directory;
ahi configuration/state overrides do not relocate it. Repository definitions
win name collisions; `ahi doctor` reports shadowed operator definitions.
Discovery still uses these two roots, not ancestor or recursive skill discovery.

Discovery reads each bounded file locally to validate and extract frontmatter.
It retains metadata, not the instruction body. The model does not receive the
full file at startup.

```yaml
---
name: review
description: >-
  Review changes for correctness and maintainability. Use before submitting a PR.
license: MIT
compatibility: Requires Rust and Cargo
metadata:
  author: example
  version: "1"
disable-model-invocation: false
---

Read `references/checklist.md` before reviewing the changes.
```

- `name` and a nonempty `description` are required. The name must match the
  directory and satisfy ahi's existing 1–64-character name rules (ASCII letters,
  digits, hyphens, underscores, and dots).
- Descriptions may contain up to 1024 Unicode characters, including multiline
  YAML strings. They are normalized to one line in the model-facing catalog.
- `disable-model-invocation: true` marks a skill manual-only.
- `license`, `compatibility`, `metadata`, `allowed-tools`, and unknown extension
  keys are accepted and retained as informational YAML values. `ahi doctor`
  lists their keys, not their values. **`allowed-tools` never grants permission.**
- Legacy ahi fields `when`, `references`, and `version` remain accepted.
  `when` and optional declared references augment the catalog. `version` is
  informational. References are optional; supporting paths need not be listed
  in frontmatter. Listed references are validated at discovery.
- Malformed YAML, duplicate keys, unsafe YAML constructs, missing required
  fields, invalid names, symlinks, and oversized files are still rejected.

The `load_skill` tool description advertises approved automatic skills with
name, description, source, and `SKILL.md` location (plus legacy routing hints
when present). Other metadata values and instruction bodies are not advertised.
Manual-only skills are visible to the operator but omitted from this automatic
catalog. A guessed automatic call cannot load a manual-only skill.

## Explicit invocation

`$name` works anywhere in the new operator prompt, including multiline input:

```sh
ahi exec --model local/model -p 'Please use $review for these changes'
```

Quote shell arguments so the shell does not expand `$review`. Headless callers
can put the same text in `--prompt-file`. Escape a literal named dollar reference
as `\$HOME`; bare dollars and amounts such as `$20.50` remain literal.
Skill names consume letters, digits, `-`, `_`, and `.`; separate following prose
with whitespace. The TUI picker inserts references but does not send the draft.

Before the first model request, the shared backend:

1. Extracts and deduplicates explicit references from the new operator message.
   It does not reinterpret history, assistant text, or tool output as new intent.
2. Runs actual `load_skill` calls through the existing policy, approval channel,
   cancellation checks, and output budget.
3. Records the calls and results as a normal, replayable tool exchange. The
   original user prompt remains unchanged, including its digest in headless runs.
4. Sends the resulting instructions to the model as tool results. Each result
   identifies the skill source and base directory for supporting paths.

This is deterministic loading, not an instruction asking the model to decide
whether to load the skill. Duplicate references load the body only once per
turn. Explicitly selected manual-only skills become available for subsequent
supporting-file calls during that turn. They are not automatically re-enabled
by their presence in history on later turns.

A failed, denied, or cancelled explicit load prevents inference. Instructions
that exceed the turn output budget are not sent partially. Headless execution
denies an `ask: true` load when no operator is attending; explicit syntax does
not bypass approvals. Automatic loads retain normal tool-loop error behavior.

## Supporting files and permissions

After loading a skill, the model can read a supporting file with:

```json
{"name": "review", "file": "references/checklist.md"}
```

Paths are relative to that skill's directory, not the workspace. `references/`,
`scripts/`, and `assets/` text files can be read without frontmatter declarations.
Reading a script never executes it; executing it still requires a separate
command approval. Omit `file` to read the instruction body without frontmatter.

Reads require regular UTF-8 files and enforce a 32 KiB limit, including a bounded
read if the file grows. Absolute paths, parent traversal, backslashes, drive/ADS
syntax, and symbolic links are refused. macOS/Linux also enforce no-follow
component opens through directory descriptors; other platforms use checked
pathname opens. These checks are not a sandbox.

An `action: skill` approval authorizes reading the named skill's instructions
and bounded supporting files inside its directory. Other tool permissions remain
independent. The audit log records source, path, size, and content hash, not the
instruction text. Session tool results retain the content for replay.

This does not implement `AGENTS.md` instruction loading, automatic project trust,
operator symlink adoption, or a new skill allow/deny list. Existing tool-policy
approval remains required.
