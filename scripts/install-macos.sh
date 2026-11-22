#!/usr/bin/env bash
# Bootstrap an attested Apple Silicon release. No sudo or shell profile edits.
set -euo pipefail
repo=ahi-sh/ahi
usage() { echo 'Usage: bash scripts/install-macos.sh [vMAJOR.MINOR.PATCH]'; echo 'Installs to ${AHI_INSTALL_DIR:-$HOME/.local/bin}/ahi. Requires gh auth login.'; }
case ${1:-} in -h|--help) usage; exit 0;; esac
[[ $# -le 1 ]] || { usage >&2; exit 2; }
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { echo 'Requires native Apple Silicon macOS. Otherwise use cargo install ahi --locked.' >&2; exit 1; }
command -v gh >/dev/null || { echo 'Install GitHub CLI (gh) and run gh auth login first.' >&2; exit 1; }
tag=${1:-$(gh release view --repo "$repo" --json tagName --jq .tagName)}
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Expected a stable vMAJOR.MINOR.PATCH tag.' >&2; exit 2; }
version=${tag#v}; archive="ahi-$version-macos-aarch64.tar.gz"; sums="ahi-$version-macos-sha256sums.txt"
dest=${AHI_INSTALL_DIR:-$HOME/.local/bin}
mkdir -p "$dest"; dest=$(cd "$dest" && pwd -P)
[[ ! -d $dest/ahi && ! -L $dest/ahi ]] || { echo "Refusing to replace directory or symlink: $dest/ahi" >&2; exit 1; }
work=$(mktemp -d "$dest/.ahi-install.XXXXXXXX"); trap 'rm -rf "$work"' EXIT
cd "$work"
gh release download "$tag" --repo "$repo" --pattern "$archive" --pattern "$sums"
commit=$(gh api "repos/$repo/commits/$tag" --jq .sha)
[[ $commit =~ ^[0-9a-f]{40}$ ]] || { echo 'Cannot resolve release commit.' >&2; exit 1; }
for file in "$sums" "$archive"; do
  gh attestation verify "$file" --repo "$repo" --source-ref "refs/tags/$tag" --source-digest "$commit" \
    --signer-workflow "$repo/.github/workflows/release_binary_assets.yml" --predicate-type 'https://slsa.dev/provenance/v1'
done
shasum -a 256 "$archive" > actual.sha256
cmp "$sums" actual.sha256 || { echo 'Archive checksum or manifest contents differ.' >&2; exit 1; }
[[ $(tar -tzf "$archive") == ahi ]] || { echo 'Expected one archive member: ahi.' >&2; exit 1; }
tar -xOzf "$archive" ahi > candidate
chmod 755 candidate
[[ $(./candidate --version) == "ahi $version" ]] || { echo 'Binary version mismatch.' >&2; exit 1; }
mv -f candidate "$dest/ahi"
printf 'Installed ahi %s at %s/ahi. Use ahi update for subsequent releases.\n' "$version" "$dest"
case ":$PATH:" in *":$dest:"*) ;; *) printf 'Add %s to PATH to use this installation.\n' "$dest";; esac
