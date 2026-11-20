"""Compare two cargo package archives for the published-member gate."""
import hashlib
import sys
import tarfile
import tomllib


def inspect(path, prefix):
    with tarfile.open(path, 'r:gz') as archive:
        manifest = tomllib.loads(archive.extractfile(prefix + 'Cargo.toml').read().decode())
        # Cargo's normalized manifest spells out even auto-discovered binaries.
        # If auto-discovery isn't explicitly disabled, don't assume library-only.
        library_only = ('lib' in manifest and not manifest.get('bin')
                        and manifest['package'].get('autobins') is False)
        ignored = {prefix + 'Cargo.toml', prefix + '.cargo_vcs_info.json'}
        files = {m.name: hashlib.sha256(archive.extractfile(m).read()).digest()
                 for m in archive if m.isfile() and m.name not in ignored}
    return files, library_only


def main(name, version, published_path, local_path):
    prefix = f'{name}-{version}/'
    published, published_lib = inspect(published_path, prefix)
    local, local_lib = inspect(local_path, prefix)
    if published_lib and local_lib:
        for files in (published, local):
            files.pop(prefix + 'Cargo.lock', None)
    if published == local:
        print(f'{name} {version}: published crate matches the tree')
        return 0
    print(f'{name} {version} is on crates.io but differs from the tree:', file=sys.stderr)
    for file in sorted(published.keys() | local.keys()):
        if published.get(file) != local.get(file):
            action = 'added' if file not in published else 'removed' if file not in local else 'changed'
            print(f'  {action:<8} {file}', file=sys.stderr)
    print(f'  bump `version` in crates/{name}/Cargo.toml and the exact `{name}` pin in ahi,\n'
          f'  then `cargo update -p {name} --offline`; the publish script uploads it before ahi.',
          file=sys.stderr)
    return 1


if __name__ == '__main__':
    sys.exit(main(*sys.argv[1:]))
