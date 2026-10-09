"""Tests for scripts/package_linux.py: the .deb and the CLI tarball under stable names, executables
checked by ELF architecture, existing output preserved (SB-88)."""
import json
import pathlib
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import package_linux  # noqa: E402

VERSION = '0.7.0'


def elf(machine):
    header = bytearray(64)
    header[:4] = b'\x7fELF'
    header[4], header[5] = 2, 1
    header[18:20] = machine.to_bytes(2, 'little')
    return bytes(header) + b'body'


class Package(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        release = self.root / 'target/release'
        (release / 'bundle/deb').mkdir(parents=True)
        for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
            (self.root / name).write_text(name)

    def files(self, machine, deb_arch):
        release = self.root / 'target/release'
        (release / 'fabric-switchboard').write_bytes(elf(machine))
        (release / 'switchboard').write_bytes(elf(machine))
        (release / f'bundle/deb/Fabric Switchboard_{VERSION}_{deb_arch}.deb').write_bytes(b'deb')

    def build(self, arch):
        return package_linux.package(self.root, VERSION, 'c0ffee', arch, {'rustc': 'test'}, 'PASS', 'PASS')

    def test_x64_and_arm64_are_packaged_under_their_own_names(self):
        for arch, machine, deb in (('x64', 62, 'amd64'), ('arm64', 183, 'arm64')):
            self.files(machine, deb)
            receipt = self.build(arch)
            self.assertEqual(set(receipt['sha256']), {f'Fabric-Switchboard-{VERSION}-linux-{arch}.deb',
                                                     f'Fabric-Switchboard-{VERSION}-linux-{arch}-cli.tar.gz'})
            self.assertEqual(receipt['elf_machine'], {'fabric-switchboard': machine, 'switchboard': machine})
            self.assertEqual(receipt['provider_live_acceptance'], 'NOT_RUN')
            with tarfile.open(self.root / 'artifacts' / f'Fabric-Switchboard-{VERSION}-linux-{arch}-cli.tar.gz') as bundle:
                names = {pathlib.PurePosixPath(n).name for n in bundle.getnames()}
            self.assertEqual(names, {'switchboard', 'LICENSE', 'THIRD_PARTY_NOTICES.md'})
            saved = json.loads((self.root / 'artifacts' / f'Fabric-Switchboard-{VERSION}-linux-{arch}-receipt.json').read_text())
            self.assertEqual(saved, receipt)

    def test_a_binary_of_the_other_architecture_is_refused(self):
        self.files(62, 'arm64')
        with self.assertRaises(SystemExit):
            self.build('arm64')

    def test_a_missing_deb_or_existing_output_is_refused(self):
        (self.root / 'target/release/fabric-switchboard').write_bytes(elf(62))
        (self.root / 'target/release/switchboard').write_bytes(elf(62))
        with self.assertRaises(SystemExit):
            self.build('x64')
        self.files(62, 'amd64')
        self.build('x64')
        with self.assertRaises(SystemExit):
            self.build('x64')

    def test_a_non_elf_file_is_refused(self):
        self.files(62, 'amd64')
        (self.root / 'target/release/switchboard').write_bytes(b'MZ' + bytes(62))
        with self.assertRaises(SystemExit):
            self.build('x64')


if __name__ == '__main__':
    unittest.main()
