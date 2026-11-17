# Issue #39: bounded search-open hardening draft

## Implemented scope

On macOS and Linux, `Workspace` opens a directory descriptor at initialization.
`search_files` opens each candidate through that descriptor, one component at a
time, with `openat` and `O_NOFOLLOW`; ancestors also require `O_DIRECTORY`.
The final open uses `O_NONBLOCK` so a substituted FIFO does not block before
regular-file validation. `O_CLOEXEC` prevents descriptor inheritance on exec.
Owned `std::fs::File` values close descriptors and provide metadata/read APIs.
There are no new registry dependencies, handwritten stat layouts, custom
readdir implementation, or unsafe blocks in the main crate.

The root is a trusted bootstrap boundary: canonicalization and opening that root
are not an atomic operation. After it is opened, replacement of the root pathname
does not redirect reads. Excluded ahi directories are checked lexically before
candidate opening, as well as being pruned during enumeration.

## Exact guarantee and remaining gaps

Candidate file content is not read through a substituted symlink, whether the
symlink replaces the final file or an ancestor. This is not complete filesystem
containment and is not a sandbox.

- The tree walker and `.gitignore` loading still use pathname APIs. This change
  does not secure enumeration or prevent disclosure of names from raced walks.
- `read_file`, skill loading, editing, writing, and listing are unchanged.
- Windows and other platforms retain the existing pathname-based search opens;
  no equivalent hardening is claimed there.
- Root bootstrap races, hard links, mounts, and renaming already-open directories
  are not addressed. In particular, path-based exclusions are not inode-based
  access controls and do not survive adversarial moves of protected content.
- Arbitrary shell execution remains outside builtin file-tool protections.
- Deep paths require more opens than before. No performance improvement is
  claimed; directory handle reuse/custom iteration is deferred.

## Verification and next review gates

Deterministic tests enumerate a candidate, replace the file or an ancestor with
an outside symlink, and invoke the real scanner. They verify refusal, normal
matching output, exclusion checks, recovery after failed opens, root-handle
anchoring, and rejection of traversal/absolute/NUL paths.

Local execution is on macOS. Linux flag values and behavior still require Linux
execution; Windows fallback requires its normal CI build/test coverage. Before
merge, review platform ABI constants, descriptor lifetime/close-on-exec, and add
bounded FIFO/descriptor-leak checks. Benchmark only to assess regression, not to
justify a speedup claim.

This is an unpublished draft. The changed `ahi-unix` crate must receive a new
version and the root dependency pin/lockfile must follow before release; the
published-member immutability gate is intentionally not satisfied by reusing
0.3.1. Do not publish this tree as-is.
