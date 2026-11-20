"""Offline fixtures for the archive comparison used by CI and releases."""
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest


class PublishedMemberTest(unittest.TestCase):
    def test_package_changes(self):
        lib = '[package]\nname="fixture"\nversion="1.0.0"\nautobins=false\n[lib]\npath="src/lib.rs"\n'
        binary = lib + '[[bin]]\nname="cli"\npath="src/main.rs"\n'
        original = lib + '[dependencies]\nregex="=1.13.1"\n'
        files = {'Cargo.toml': lib, 'Cargo.toml.orig': original, 'Cargo.lock': 'old',
                 'src/lib.rs': '', 'fixtures/Cargo.lock': 'old'}
        for label, manifest, edits, changed in [
            ('library resolution only', lib, {}, None),
            ('dependency requirement', lib, {'Cargo.toml.orig': original.replace('1.13.1', '1.13.2')}, 'Cargo.toml.orig'),
            ('library source', lib, {'src/lib.rs': 'pub fn added() {}'}, 'src/lib.rs'),
            ('nested lockfile', lib, {'fixtures/Cargo.lock': 'new'}, 'fixtures/Cargo.lock'),
            ('library and binary', binary, {}, 'Cargo.lock'),
        ]:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as tmp:
                paths = [Path(tmp) / f'{side}.crate' for side in ('published', 'local')]
                base = {**files, 'Cargo.toml': manifest}
                for path, contents in zip(paths, (base, {**base, 'Cargo.lock': 'new', **edits})):
                    with tarfile.open(path, 'w:gz') as archive:
                        for name, text in contents.items():
                            data = text.encode()
                            entry = tarfile.TarInfo(f'fixture-1.0.0/{name}')
                            entry.size = len(data)
                            archive.addfile(entry, io.BytesIO(data))
                result = subprocess.run([sys.executable, Path(__file__).with_name('compare_published_member.py'),
                                         'fixture', '1.0.0', *paths], capture_output=True, text=True)
                self.assertEqual(result.returncode, int(changed is not None), result.stderr)
                differences = [line.split() for line in result.stderr.splitlines() if line.startswith('  changed')]
                self.assertEqual(differences, [['changed', f'fixture-1.0.0/{changed}']] if changed else [])


if __name__ == '__main__':
    unittest.main()
