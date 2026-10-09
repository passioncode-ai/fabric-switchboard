"""Tests for scripts/updater_artifacts.py: latest.json never sends an installed app to a file the
release does not carry, and the macOS package unpacks the way the updater unpacks it."""
import datetime
import json
import pathlib
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import updater_artifacts as ua  # noqa: E402

CHANGELOG = """# Changelog

## Unreleased

- next

## 0.7.0 — 2026-10-06

- Automatic updates.

## 0.6.0

- older
"""
TAG = 'v0.7.0'
NOW = datetime.datetime(2026, 10, 6, 9, 30, tzinfo=datetime.timezone.utc)


def release_folder(root):
    folder = pathlib.Path(root)
    for targets, name in ua.packages('0.7.0'):
        (folder / name).write_bytes(b'package')
        (folder / (name + '.sig')).write_text(f'sig-of-{targets[0]}\n')
    return folder


class Manifest(unittest.TestCase):
    def test_every_platform_points_at_its_own_file_and_signature(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            document = ua.manifest(TAG, folder, CHANGELOG, now=NOW)
            self.assertEqual(document['version'], '0.7.0')
            self.assertEqual(document['notes'], '- Automatic updates.')
            self.assertEqual(document['pub_date'], '2026-10-06T09:30:00Z')
            self.assertEqual(set(document['platforms']), set(ua.REQUIRED_TARGETS))
            self.assertEqual(len(document['platforms']), 7)
            mac = document['platforms']['darwin-universal']
            self.assertEqual(mac['signature'], 'sig-of-darwin-universal')
            self.assertEqual(mac['url'], f'https://github.com/{ua.REPOSITORY}/releases/download/v0.7.0/Fabric-Switchboard-0.7.0-macos-universal.app.tar.gz')
            self.assertEqual(document['platforms']['darwin-aarch64'], mac)
            self.assertEqual(document['platforms']['windows-x86_64']['signature'], 'sig-of-windows-x86_64')
            # SB-88: Windows on ARM and both Linux architectures, each its own file.
            self.assertTrue(document['platforms']['windows-aarch64']['url'].endswith('/Fabric-Switchboard-0.7.0-windows-arm64-setup.exe'))
            self.assertTrue(document['platforms']['linux-x86_64']['url'].endswith('/Fabric-Switchboard-0.7.0-linux-x64.AppImage'))
            self.assertEqual(document['platforms']['linux-aarch64']['signature'], 'sig-of-linux-aarch64')
            self.assertEqual(json.loads((folder / ua.MANIFEST).read_text()), document)
            self.assertEqual(ua.check(TAG, folder), [])

    def test_a_missing_package_writes_no_manifest(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            (folder / ua.windows_name('0.7.0')).unlink()
            with self.assertRaises(SystemExit):
                ua.manifest(TAG, folder, CHANGELOG, now=NOW)
            self.assertFalse((folder / ua.MANIFEST).exists())

    def test_a_release_without_notes_says_its_version(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(ua.manifest(TAG, release_folder(root), '# Changelog\n', now=NOW)['notes'], 'Fabric Switchboard 0.7.0.')


class Platforms(unittest.TestCase):
    def test_a_missing_linux_package_writes_no_manifest(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            (folder / ua.linux_name('0.7.0', 'arm64')).unlink()
            with self.assertRaises(SystemExit):
                ua.manifest(TAG, folder, CHANGELOG, now=NOW)

    def test_a_manifest_without_linux_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            document = ua.manifest(TAG, folder, CHANGELOG, now=NOW)
            document['platforms'].pop('linux-x86_64')
            (folder / ua.MANIFEST).write_text(json.dumps(document))
            self.assertEqual(ua.check(TAG, folder), ['latest.json has no linux-x86_64 entry.'])

    def test_unknown_architectures_and_ambiguous_folders_are_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = pathlib.Path(root)
            with self.assertRaises(SystemExit):
                ua.linux(folder, folder / 'out', '0.7.0', 'riscv')
            with self.assertRaises(SystemExit):
                ua.linux(folder, folder / 'out', '0.7.0', 'x64')  # no AppImage at all
            (folder / 'a.AppImage').write_bytes(b'a')
            (folder / 'b.AppImage').write_bytes(b'b')
            with self.assertRaises(SystemExit):
                ua.linux(folder, folder / 'out', '0.7.0', 'x64')
            with self.assertRaises(SystemExit):
                ua.windows(folder, folder / 'out', '0.7.0', 'mips')


class Check(unittest.TestCase):
    def manifest(self, folder, edit):
        document = ua.manifest(TAG, folder, CHANGELOG, now=NOW)
        edit(document)
        (folder / ua.MANIFEST).write_text(json.dumps(document))

    def test_an_entry_naming_a_file_the_release_lacks_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            self.manifest(folder, lambda d: d['platforms']['windows-x86_64'].update(url=ua.download_url(TAG, 'absent-setup.exe')))
            self.assertEqual(ua.check(TAG, folder), ['windows-x86_64: absent-setup.exe is not among the release files.'])

    def test_an_entry_under_another_tag_or_repository_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            self.manifest(folder, lambda d: d['platforms']['darwin-x86_64'].update(url=ua.download_url('v0.6.0', ua.macos_name('0.7.0'))))
            self.assertEqual(len(ua.check(TAG, folder)), 1)
            self.assertIn('is not a file of release v0.7.0', ua.check(TAG, folder)[0])

    def test_a_signature_that_is_not_the_files_own_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            self.manifest(folder, lambda d: d['platforms']['darwin-universal'].update(signature='sig-of-windows-x86_64'))
            self.assertEqual(ua.check(TAG, folder), ['darwin-universal: the signature in latest.json is not Fabric-Switchboard-0.7.0-macos-universal.app.tar.gz.sig.'])

    def test_a_missing_platform_or_a_wrong_version_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            folder = release_folder(root)
            self.manifest(folder, lambda d: (d['platforms'].pop('darwin-universal'), d.update(version='0.6.9')))
            errors = ua.check(TAG, folder)
            self.assertIn("latest.json announces '0.6.9', the tag is v0.7.0.", errors)
            self.assertIn('latest.json has no darwin-universal entry.', errors)

    def test_no_manifest_is_refused(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(ua.check(TAG, pathlib.Path(root)), ['latest.json is missing or not JSON.'])


class Archive(unittest.TestCase):
    def test_the_app_is_the_one_top_level_entry_without_appledouble_members(self):
        with tempfile.TemporaryDirectory() as root:
            app = pathlib.Path(root) / ua.APP_NAME
            (app / 'Contents' / 'MacOS').mkdir(parents=True)
            (app / 'Contents' / 'MacOS' / 'fabric-switchboard').write_bytes(b'binary')
            (app / 'Contents' / '._Info.plist').write_bytes(b'appledouble')
            (app / 'Contents' / 'Current').symlink_to('MacOS')
            archive = pathlib.Path(root) / 'out.tar.gz'
            ua.archive_app(app, archive)
            with tarfile.open(archive) as bundle:
                names = bundle.getnames()
                link = bundle.getmember(f'{ua.APP_NAME}/Contents/Current')
            self.assertEqual({name.split('/')[0] for name in names}, {ua.APP_NAME})
            self.assertNotIn(f'{ua.APP_NAME}/Contents/._Info.plist', names)
            self.assertTrue(link.issym())


if __name__ == '__main__':
    unittest.main()
