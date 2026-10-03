"""Tests for scripts/package_windows.py: the Windows archive and what its receipt claims about
Authenticode. Synthetic PE files only; nothing is built or signed here."""
import json
import pathlib
import struct
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import package_windows  # noqa: E402

VERSION = '9.9.9'


def pe(machine):
    data = bytearray(512)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 0x3c, 0x80)
    data[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<H', data, 0x84, machine)
    return bytes(data)


def valid(name):
    return {'file': name, 'status': 'Valid', 'signer': 'CN=Example Org, O=Example Org, C=PL',
            'thumbprint': '00' * 20, 'timestamper': 'CN=Example TSA'}


class Authenticode(unittest.TestCase):
    FILES = ['switchboard.exe', f'Fabric Switchboard_{VERSION}_x64-setup.exe']

    def test_unsigned_is_said_plainly(self):
        record = package_windows.authenticode_record('NOT_SIGNED', None, self.FILES)
        self.assertEqual(record, {'status': 'NOT_SIGNED', 'reason': package_windows.NOT_SIGNED_REASON})

    def test_signed_needs_a_valid_signature_on_every_file(self):
        record = package_windows.authenticode_record('SIGNED', [valid(n) for n in self.FILES], self.FILES)
        self.assertEqual(record['status'], 'SIGNED')
        self.assertEqual(sorted(record['files']), sorted(self.FILES))
        with self.assertRaises(SystemExit):
            package_windows.authenticode_record('SIGNED', [valid(self.FILES[0])], self.FILES)
        bad = [valid(self.FILES[0]), dict(valid(self.FILES[1]), status='HashMismatch')]
        with self.assertRaises(SystemExit):
            package_windows.authenticode_record('SIGNED', bad, self.FILES)
        with self.assertRaises(SystemExit):
            package_windows.authenticode_record('SIGNED', None, self.FILES)

    def test_the_readme_tells_the_truth(self):
        self.assertIn('NOT Authenticode signed', package_windows.readme(VERSION, 'NOT_SIGNED'))
        self.assertNotIn('NOT Authenticode', package_windows.readme(VERSION, 'SIGNED'))


class Package(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        release = self.root / 'target/release'
        (release / 'bundle/nsis').mkdir(parents=True)
        (release / 'fabric-switchboard.exe').write_bytes(pe(0x8664))
        (release / f'bundle/nsis/Fabric Switchboard_{VERSION}_x64-setup.exe').write_bytes(pe(0x14c))
        cli = self.root / 'target/x86_64-pc-windows-msvc/release/switchboard.exe'
        cli.parent.mkdir(parents=True)
        cli.write_bytes(pe(0x8664))
        for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
            (self.root / name).write_text(name)

    def build(self, mode='NOT_SIGNED', signatures=None):
        return package_windows.package(self.root, VERSION, 'c0ffee', mode, signatures,
                                       toolchain={'rustc': 'rustc test'}, native_tests='PASS')

    def test_unsigned_archive_and_receipt(self):
        receipt = self.build()
        self.assertEqual(receipt['windows_authenticode'], 'NOT_SIGNED')
        self.assertEqual(receipt['native_windows_tests'], 'PASS')
        self.assertEqual(receipt['provider_live_acceptance'], 'NOT_RUN')
        archive = self.root / 'artifacts' / receipt['archive']
        self.assertEqual(archive.name, f'Fabric-Switchboard-{VERSION}-windows-x64.zip')
        self.assertEqual(receipt['archive_sha256'], package_windows.sha(archive))
        outer = json.loads((self.root / 'artifacts' / f'Fabric-Switchboard-{VERSION}-windows-x64-receipt.json').read_text())
        self.assertEqual(outer, receipt)
        with zipfile.ZipFile(archive) as bundle:
            names = {pathlib.PurePosixPath(n).name for n in bundle.namelist()}
        self.assertTrue({'switchboard.exe', f'Fabric Switchboard_{VERSION}_x64-setup.exe', 'README.txt',
                         'LICENSE', 'THIRD_PARTY_NOTICES.md', 'SHA256SUMS.txt', 'build-receipt.json'} <= names)

    def test_signed_archive_records_each_signature(self):
        files = ['switchboard.exe', f'Fabric Switchboard_{VERSION}_x64-setup.exe', 'fabric-switchboard.exe']
        receipt = self.build('SIGNED', [valid(n) for n in files])
        self.assertEqual(receipt['windows_authenticode'], 'SIGNED')
        self.assertEqual(receipt['authenticode']['files']['switchboard.exe']['status'], 'Valid')

    def test_existing_output_is_preserved(self):
        self.build()
        with self.assertRaises(SystemExit):
            self.build()


if __name__ == '__main__':
    unittest.main()
