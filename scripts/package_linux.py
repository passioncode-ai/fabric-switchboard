#!/usr/bin/env python3
"""Package the natively built Linux release, x64 or arm64 (release.yml, `linux` job; SB-88): the
.deb under a stable name, the standalone CLI as a tarball for machines without a desktop, and a
receipt with checksums, the ELF architecture of each executable and the checks the job ran. The
AppImage ships as the updater package (scripts/updater_artifacts.py linux), signed for updates.

  python3 scripts/package_linux.py --arch x64 --native-tests PASS --smoke PASS
"""
import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Per architecture: the .deb's architecture word and the ELF e_machine of the executables.
ARCHES = {'x64': ('amd64', 62), 'arm64': ('arm64', 183)}


def sha(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def elf_machine(path, expected):
    """The ELF header's machine, refused unless it is a 64-bit little-endian `expected`."""
    data = Path(path).read_bytes()[:64]
    if data[:4] != b'\x7fELF' or data[4] != 2 or data[5] != 1:
        raise SystemExit(f'{Path(path).name} is not a 64-bit little-endian ELF executable.')
    machine = int.from_bytes(data[18:20], 'little')
    if machine != expected:
        raise SystemExit(f'{Path(path).name}: ELF machine {machine}, expected {expected}.')
    return machine


def package(root, version, commit, arch, toolchain, native_tests, smoke):
    if arch not in ARCHES:
        raise SystemExit(f'Unknown architecture {arch!r}; expected one of {", ".join(ARCHES)}.')
    deb_arch, machine = ARCHES[arch]
    artifacts = root / 'artifacts'
    deb_out = artifacts / f'Fabric-Switchboard-{version}-linux-{arch}.deb'
    cli_out = artifacts / f'Fabric-Switchboard-{version}-linux-{arch}-cli.tar.gz'
    receipt_path = artifacts / f'Fabric-Switchboard-{version}-linux-{arch}-receipt.json'
    if any(p.exists() for p in (deb_out, cli_out, receipt_path)):
        raise SystemExit('Output already exists; preserve it before rebuilding.')
    app = root / 'target/release/fabric-switchboard'
    cli = root / 'target/release/switchboard'
    debs = sorted((root / 'target/release/bundle/deb').glob(f'*_{version}_{deb_arch}.deb'))
    if len(debs) != 1:
        raise SystemExit(f'Expected exactly one {deb_arch} .deb for {version}, found {len(debs)}.')
    machines = {p.name: elf_machine(p, machine) for p in (app, cli)}
    artifacts.mkdir(parents=True, exist_ok=True)
    shutil.copy2(debs[0], deb_out)
    with tarfile.open(cli_out, 'w:gz') as bundle:
        folder = f'Fabric-Switchboard-{version}-linux-{arch}-cli'
        bundle.add(cli, arcname=f'{folder}/switchboard')
        for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
            bundle.add(root / name, arcname=f'{folder}/{name}')
    receipt = {
        'version': version, 'commit': commit, 'source_clean': True, 'arch': arch,
        'method': 'native build on a GitHub-hosted Ubuntu runner (release workflow)',
        'toolchain': toolchain, 'elf_machine': machines,
        'sha256': {deb_out.name: sha(deb_out), cli_out.name: sha(cli_out)},
        'native_linux_tests': native_tests, 'smoke': smoke,
        'provider_live_acceptance': 'NOT_RUN',
    }
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


def capture(args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--arch', choices=sorted(ARCHES), required=True)
    parser.add_argument('--native-tests', choices=['PASS'], required=True, help='The tests this job ran before packaging')
    parser.add_argument('--smoke', choices=['PASS'], required=True, help='scripts/smoke_native.py on the built app, both launches')
    args = parser.parse_args()
    if capture(['git', 'status', '--porcelain', '--untracked-files=all']):
        raise SystemExit('The source tree changed during the build; no receipt issued.')
    version = json.loads((ROOT / 'package.json').read_text(encoding='utf-8'))['version']
    subprocess.run([str(ROOT / 'target/release/switchboard'), '--help'], check=True, stdout=subprocess.DEVNULL)
    toolchain = {'rustc': capture(['rustc', '--version']), 'node': capture(['node', '--version']),
                 'os': platform.platform()}
    receipt = package(ROOT, version, capture(['git', 'rev-parse', 'HEAD']), args.arch, toolchain,
                      args.native_tests, args.smoke)
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
