"""Tests for scripts/release_bump.py: one step moves every version and opens the CHANGELOG section."""
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from release_bump import bumped, LOCK_PACKAGES  # noqa: E402

FILES = {
    '.claude-plugin/marketplace.json': '{"plugins": [{"name": "switchboard", "version": "1.2.3"}]}\n',
    'plugins/switchboard/.claude-plugin/plugin.json': '{"name": "switchboard", "version": "1.2.3"}\n',
    'plugins/switchboard/skills/switching-accounts/SKILL.md': '---\nmetadata:\n  version: "1.2.3"\n---\n',
    'Cargo.toml': '[workspace.package]\nversion = "1.2.3"\n\n[dependencies]\nserde = { version = "1.2.3" }\n',
    'package.json': '{\n  "name": "x",\n  "version": "1.2.3",\n  "dependencies": {"y": "1.2.3"}\n}\n',
    'package-lock.json': '{\n  "version": "1.2.3",\n  "packages": {\n    "": {\n      "version": "1.2.3"\n    },\n    "node_modules/y": {\n      "version": "1.2.3"\n    }\n  }\n}\n',
    'src-tauri/tauri.conf.json': '{\n  "version": "1.2.3"\n}\n',
    'Cargo.lock': ''.join(f'[[package]]\nname = "{n}"\nversion = "1.2.3"\n\n' for n in LOCK_PACKAGES)
    + '[[package]]\nname = "serde"\nversion = "1.2.3"\n',
    'CHANGELOG.md': '# Changelog\n\n## Unreleased\n\n- a change\n\n## 1.2.3\n\n- older\n',
}


def tree(files):
    root = pathlib.Path(tempfile.mkdtemp())
    for path, text in files.items():
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(text)
    return root


class Bump(unittest.TestCase):
    def test_every_place_moves_and_nothing_else_does(self):
        old, out = bumped(tree(FILES), '1.2.4')
        self.assertEqual(old, '1.2.3')
        self.assertIn('version = "1.2.4"', out['Cargo.toml'])
        self.assertIn('serde = { version = "1.2.3" }', out['Cargo.toml'], 'a dependency keeps its version')
        self.assertEqual(out['package-lock.json'].count('"version": "1.2.4"'), 2)
        self.assertIn('"node_modules/y": {\n      "version": "1.2.3"', out['package-lock.json'])
        self.assertEqual(out['Cargo.lock'].count('version = "1.2.4"'), len(LOCK_PACKAGES))
        self.assertIn('name = "serde"\nversion = "1.2.3"', out['Cargo.lock'])
        self.assertIn('version: "1.2.4"', out['plugins/switchboard/skills/switching-accounts/SKILL.md'])
        self.assertTrue(out['CHANGELOG.md'].startswith('# Changelog\n\n## Unreleased\n\n## 1.2.4\n\n- a change'))

    def test_it_refuses_an_older_version_a_missing_place_and_an_empty_release(self):
        with self.assertRaises(SystemExit):
            bumped(tree(FILES), '1.2.3')
        with self.assertRaises(SystemExit):
            bumped(tree(FILES), 'v1.2.4')
        broken = dict(FILES, **{'src-tauri/tauri.conf.json': '{"version": "1.2.2"}\n'})
        with self.assertRaises(SystemExit):
            bumped(tree(broken), '1.2.4')
        empty = dict(FILES, **{'CHANGELOG.md': '# Changelog\n\n## Unreleased\n\n## 1.2.3\n\n- older\n'})
        with self.assertRaises(SystemExit):
            bumped(tree(empty), '1.2.4')


if __name__ == '__main__':
    unittest.main()
