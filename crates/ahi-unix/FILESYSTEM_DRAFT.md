# Issue #39: shared workspace read-opening API draft

## Goal and structure

Give builtin file readers one consistent opening operation, rather than separate
path validation, opening, and metadata logic per tool. This is a code/API
improvement, not a security boundary. Sandboxes remain the containment mechanism.

`Workspace::open_read` owns relative-path validation, existing exact-spelling and
symlink rules, ahi-directory exclusions, platform opening, and regular-file
validation. It returns `OpenedRead`: an owned file and its size observed from the
same handle. A file can still change after this snapshot.

The named tool modules remain intact:

- `src/tools/read_file.rs` calls `open_read`, then owns UTF-8 handling, pagination,
  and its file/output limits.
- `src/tools/search_files.rs` calls `open_read`, then owns matching, binary-file
  handling, budgets, and skip-on-open-error behavior.
- `src/tools/workspace.rs` owns the common opening contract.
- `crates/ahi-unix/src/directory.rs` owns the platform-specific descriptor API.

No search-only opening API remains. Search now applies the same path-spelling and
exclusion checks as direct reads, rather than relying solely on enumeration.

## Platform implementation

On macOS and Linux, Workspace opens a directory descriptor at initialization.
Descendant opens use `openat`, one component at a time, with `O_NOFOLLOW`;
ancestors also require `O_DIRECTORY`. The final open uses `O_NONBLOCK` so a FIFO
replacement does not block before regular-file validation. `O_CLOEXEC` prevents
descriptor inheritance on exec. Owned standard File values close descriptors;
Workspace obtains metadata from that handle before either tool reads it.

There are no new registry dependencies, handwritten stat layouts, custom readdir
implementation, or unsafe blocks in the main crate. Other platforms use pathname
opening behind the same shared checks. Platform guarantees are not identical.

## Deliberate limits

- Directory traversal and `.gitignore` loading retain their pathname APIs.
- Editing, writing, listing, and skill loading are not migrated: their operations
  and allowed roots differ from these two file readers.
- Root bootstrap races, hard links, mounts, and renames of already-open
  directories are not addressed. Path exclusions are not inode access controls.
- Shell execution remains outside builtin file-tool rules.
- Shared canonical path checks and component-wise opens may cost more than the
  previous search path. No performance improvement is claimed.

## Verification and remaining gates

Tests exercise ordinary reads and search output, non-regular-file refusal,
excluded paths, file/ancestor replacement with symlinks, recovery after failed
opens, descriptor root anchoring, and invalid paths. Existing tool tests cover
pagination, UTF-8, search matching, and budgets. Direct descriptor tests ensure
symlink refusal is not tested solely through the prior canonical-path check.

Local execution is on macOS. Linux ABI constants/behavior need Linux execution;
Windows fallback needs its normal CI coverage. Descriptor-leak/FIFO checks and
representative overhead measurements remain useful before merge.

This is an unpublished draft. Before release, bump the changed ahi-unix crate and
update the root dependency pin/lockfile. The published-member immutability gate
must not be bypassed by publishing altered contents as 0.3.1.
