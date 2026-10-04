"""Tests for scripts/release_preflight.py: what the release workflow refuses before any build."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from release_preflight import check, section  # noqa: E402

VERSIONS = {'package.json': '1.2.3', 'src-tauri/tauri.conf.json': '1.2.3', 'Cargo.toml': '1.2.3'}
CHANGELOG = """# Changelog

## Unreleased

- something

## 1.2.3 — 2026-10-03

- Windows: windows_authenticode: NOT_SIGNED (Azure Artifact Signing is not enabled yet).

## 1.2.2

- older
"""


class Preflight(unittest.TestCase):
    def test_a_rehearsal_tag_needs_only_matching_versions(self):
        self.assertEqual(check('v1.2.3-rc.1', VERSIONS, '', publish=False), [])

    def test_a_tag_must_name_the_version_being_built(self):
        errors = check('v1.2.4-rc.1', VERSIONS, CHANGELOG, publish=False)
        self.assertTrue(any('1.2.4' in e for e in errors))

    def test_versions_must_agree_with_each_other(self):
        errors = check('v1.2.3-rc.1', dict(VERSIONS, **{'Cargo.toml': '1.2.2'}), '', publish=False)
        self.assertTrue(any('Cargo.toml' in e for e in errors))

    def test_only_a_version_tag_releases(self):
        for ref in ('main', 'v1.2', 'v1.2.3-preview', '1.2.3'):
            self.assertTrue(check(ref, VERSIONS, CHANGELOG, publish=False), ref)

    def test_publishing_needs_the_changelog_section(self):
        self.assertEqual(check('v1.2.3', VERSIONS, CHANGELOG, publish=True), [])
        errors = check('v1.2.3', VERSIONS, '# Changelog\n\n## Unreleased\n', publish=True)
        self.assertTrue(any('## 1.2.3' in e for e in errors))

    def test_unsigned_windows_must_be_said_in_the_notes(self):
        self.assertEqual(check('v1.2.3', VERSIONS, CHANGELOG, publish=True, windows_signing=False), [])
        quiet = CHANGELOG.replace('windows_authenticode: NOT_SIGNED', 'Windows build')
        errors = check('v1.2.3', VERSIONS, quiet, publish=True, windows_signing=False)
        self.assertTrue(any('NOT_SIGNED' in e for e in errors))
        # Signed Windows builds need no such line; a rehearsal publishes no notes at all.
        self.assertEqual(check('v1.2.3', VERSIONS, quiet, publish=True, windows_signing=True), [])
        self.assertEqual(check('v1.2.3-rc.1', VERSIONS, quiet, publish=False, windows_signing=False), [])

    def test_section_matches_the_publish_workflow(self):
        self.assertIn('NOT_SIGNED', section(CHANGELOG, '1.2.3'))
        self.assertNotIn('older', section(CHANGELOG, '1.2.3'))
        self.assertEqual(section(CHANGELOG, '1.2'), '')


if __name__ == '__main__':
    unittest.main()


class Encoding(unittest.TestCase):
    def test_the_notes_are_read_as_utf8_whatever_the_locale(self):
        """The Windows runner's default encoding is cp1252: CHANGELOG's typographic quotes made the
        v0.5.4-beta.1 run fail before any build. Run the script under an ASCII locale."""
        import json
        import os
        import subprocess
        root = pathlib.Path(__file__).resolve().parent.parent
        version = json.loads((root / 'package.json').read_text(encoding='utf-8'))['version']
        self.assertTrue(any(ord(c) > 127 for c in (root / 'CHANGELOG.md').read_text(encoding='utf-8')))
        env = dict(os.environ, LC_ALL='C', LANG='C', PYTHONUTF8='0', PYTHONCOERCECLOCALE='0', PYTHONIOENCODING='ascii')
        done = subprocess.run([sys.executable, str(root / 'scripts/release_preflight.py'), '--tag', f'v{version}', '--publish', 'false'],
                              env=env, capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stderr[-500:])
