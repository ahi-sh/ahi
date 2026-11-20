#!/usr/bin/env bash
# Refuse a member crate whose version is already on crates.io with different
# contents. ahi pins its in-repo crates (ahi-win32, ahi-widgets, ahi-unix, ahi-durable)
# by exact version, and the publish script skips a member whose version the
# registry already lists. That skip is only safe when the published crate is
# the one in the tree. A dependency bump inside a member (Dependabot edits
# crates/*/Cargo.toml without bumping the crate's version) breaks that: the
# tree builds from the path, the registry copy pins the old dependency, and
# `cargo publish` of ahi fails at release time (0.13.0, 2026-11-08).
#
# For each member: read its version; if the sparse index lists it, download
# the published .crate and compare it file by file with `cargo package`'s
# output, ignoring Cargo's normalized manifest and VCS info, and the root
# lockfile only for library-only packages. Any other difference is an error. An
# unpublished version passes: it will be published. No token is needed.
#
# Runs in CI on every push, in cut-release.sh before anything is tagged, and
# in publish-workspace.sh before a member is skipped.
set -euo pipefail

# shellcheck source=scripts/python_runner.sh
source "$(dirname "${BASH_SOURCE[0]}")/python_runner.sh"

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

members=(crates/ahi-win32 crates/ahi-widgets crates/ahi-unix crates/ahi-durable)
status=0

manifest_version() {
  sed -n 's/^version = "\(.*\)"$/\1/p' "$1" | head -n 1
}

manifest_name() {
  sed -n 's/^name = "\(.*\)"$/\1/p' "$1" | head -n 1
}

index_path() {
  local name="$1"
  case "${#name}" in
    1) echo "1/${name}" ;;
    2) echo "2/${name}" ;;
    3) echo "3/${name:0:1}/${name}" ;;
    *) echo "${name:0:2}/${name:2:2}/${name}" ;;
  esac
}

for member in "${members[@]}"; do
  manifest="$member/Cargo.toml"
  name="$(manifest_name "$manifest")"
  version="$(manifest_version "$manifest")"
  if [[ -z "$name" || -z "$version" ]]; then
    echo "check-published-members: cannot read name and version from $manifest" >&2
    exit 2
  fi
  index="https://index.crates.io/$(index_path "$name")"
  if ! curl --fail --silent --show-error "$index" 2>/dev/null | grep --quiet "\"vers\":\"${version}\""; then
    echo "${name} ${version}: not on crates.io yet; it will be published"
    continue
  fi
  work="$(mktemp -d "${TMPDIR:-/tmp}/check-published.XXXXXX")"
  trap 'rm -rf "$work"' EXIT
  published="$work/${name}-${version}.published.crate"
  curl --fail --silent --show-error --location \
    --output "$published" \
    "https://static.crates.io/crates/${name}/${name}-${version}.crate"
  # --allow-dirty: the release script runs this with the version bump
  # uncommitted; the member crates themselves are compared, not ahi.
  cargo package --quiet --allow-dirty --no-verify --package "$name" \
    --target-dir "$work/target" >/dev/null
  local_crate="$work/target/package/${name}-${version}.crate"
  if ! run_python "$name" "$version" "$published" "$local_crate" < scripts/compare_published_member.py; then
    status=1
  fi
  rm -rf "$work"
  trap - EXIT
done

exit "$status"
