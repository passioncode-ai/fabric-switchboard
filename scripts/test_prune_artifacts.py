"""Lifecycle LC-15: a build leaves at most the current and the previous release of each kind."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prune_artifacts import prune, release_key  # noqa: E402


def make_release(root: Path, version: str, kind: str, app: bool = False) -> None:
    folder = root / f'Fabric-Switchboard-{version}-{kind}'
    folder.mkdir()
    (folder / 'switchboard').write_text('binary')
    if app:
        (folder / 'Fabric Switchboard.app' / 'Contents').mkdir(parents=True)
    (root / f'{folder.name}.zip').write_text('zip')
    (root / f'{folder.name}-receipt.json').write_text('{}')


class PruneTest(unittest.TestCase):
    def test_two_builds_leave_two_releases_per_kind_and_keep_receipts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for version in ('0.4.0', '0.5.0', '0.5.9'):
                make_release(root, version, 'macos-universal', app=True)
                make_release(root, version, 'windows-x64')
            (root / 'release-notes.md').write_text('notes')
            (root / 'Fabric-Switchboard-0.1.0-macos-arm64.zip').write_text('old zip only')
            unregistered = []
            # Two builds, each followed by the prune the build script runs.
            for version in ('0.5.10', '0.5.11'):
                make_release(root, version, 'macos-universal', app=True)
                make_release(root, version, 'windows-x64')
                prune(root, keep=2, unregister=unregistered.append)
            names = sorted(p.name for p in root.iterdir())
            for kind in ('macos-universal', 'windows-x64'):
                kept = sorted({release_key(Path(n))[1] for n in names
                               if release_key(Path(n)) and release_key(Path(n))[0] == kind
                               and not n.endswith('-receipt.json')})
                self.assertEqual(kept, [(0, 5, 10), (0, 5, 11)], kind)
            # The newer of an unrelated kind stays; receipts and notes are never removed.
            self.assertIn('Fabric-Switchboard-0.1.0-macos-arm64.zip', names)
            self.assertIn('Fabric-Switchboard-0.4.0-windows-x64-receipt.json', names)
            self.assertIn('release-notes.md', names)
            # Every removed app bundle was unregistered from LaunchServices first.
            self.assertEqual(
                sorted(Path(p).parent.name for p in unregistered),
                ['Fabric-Switchboard-0.4.0-macos-universal',
                 'Fabric-Switchboard-0.5.0-macos-universal',
                 'Fabric-Switchboard-0.5.9-macos-universal'])

    def test_versions_sort_numerically_not_as_text(self):
        self.assertGreater(release_key(Path('Fabric-Switchboard-0.5.10-windows-x64'))[1],
                           release_key(Path('Fabric-Switchboard-0.5.9-windows-x64'))[1])
        self.assertIsNone(release_key(Path('build-macos-0.5.0.log')))
        self.assertIsNone(release_key(Path('Fabric-Switchboard-0.5.0-macos-universal-receipt.json')))


if __name__ == '__main__':
    unittest.main()
