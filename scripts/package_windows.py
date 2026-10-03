#!/usr/bin/env python3
"""Package the natively built Windows x64 release (release.yml, `windows` job): the NSIS installer
and the portable CLI in one ZIP, with checksums, the license, a README and a receipt that says
whether the executables carry a valid Authenticode signature.

  python3 scripts/package_windows.py --authenticode NOT_SIGNED --native-tests PASS
  python3 scripts/package_windows.py --authenticode SIGNED --signatures signatures.json --native-tests PASS

`signatures.json` is what the workflow's PowerShell step read with Get-AuthenticodeSignature:
a list of {file, status, signer, thumbprint, timestamper}. SIGNED is refused unless every
executable is listed with status Valid.
"""
import argparse
import json
import platform
import shutil
import subprocess
import zipfile
from pathlib import Path

from build_windows_cross import pe_header, sha

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'x86_64-pc-windows-msvc'
NOT_SIGNED_REASON = ('Windows signing (Azure Artifact Signing) is switched off: the release '
                     'environment variable AZURE_SIGNING_ENABLED is not true.')


def authenticode_record(mode, signatures, required):
    if mode == 'NOT_SIGNED':
        return {'status': 'NOT_SIGNED', 'reason': NOT_SIGNED_REASON}
    if not signatures:
        raise SystemExit('SIGNED needs the Get-AuthenticodeSignature report (--signatures).')
    by_file = {Path(item['file']).name: item for item in signatures}
    for name in required:
        status = by_file.get(name, {}).get('status')
        if status != 'Valid':
            raise SystemExit(f'{name}: Authenticode status {status or "missing"}, not Valid; refusing to call it signed.')
    return {'status': 'SIGNED', 'files': {name: by_file[name] for name in required}}


def readme(version, mode):
    signed = ('Windows executables are Authenticode signed; see the adjacent receipt.\n' if mode == 'SIGNED'
              else 'Windows binaries are NOT Authenticode signed; SmartScreen may warn. See the adjacent receipt.\n')
    return (f'Fabric Switchboard {version} Windows x64\n'
            'Run the NSIS setup for the desktop, or switchboard.exe --help for CLI.\n'
            'WebView2 is handled by the installer. Keep GUI or switchboard serve running for managed sessions.\n'
            'Built natively on a GitHub-hosted Windows runner by the release workflow.\n' + signed)


def package(root, version, commit, mode, signatures, toolchain, native_tests):
    artifacts = root / 'artifacts'
    folder = artifacts / f'Fabric-Switchboard-{version}-windows-x64'
    archive = Path(str(folder) + '.zip')
    receipt_path = artifacts / (folder.name + '-receipt.json')
    if any(path.exists() for path in (folder, archive, receipt_path)):
        raise SystemExit('Output already exists; preserve folder, ZIP and receipt before rebuilding.')
    app = root / 'target/release/fabric-switchboard.exe'
    cli = root / 'target' / TARGET / 'release/switchboard.exe'
    installer = root / 'target/release/bundle/nsis' / f'Fabric Switchboard_{version}_x64-setup.exe'
    headers = {p.name: pe_header(p, 0x14c if p == installer else 0x8664) for p in (app, cli, installer)}
    authenticode = authenticode_record(mode, signatures, [cli.name, installer.name, app.name])
    folder.mkdir(parents=True)
    for path in (cli, installer):
        shutil.copy2(path, folder / path.name)
    (folder / 'README.txt').write_text(readme(version, mode))
    # The license and the third-party notices travel with every binary archive.
    for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(root / name, folder / name)
    hashes = {p.name: sha(p) for p in sorted(folder.glob('*.exe'))}
    (folder / 'SHA256SUMS.txt').write_text(''.join(f'{value}  {name}\n' for name, value in hashes.items()))
    receipt = {
        'version': version, 'commit': commit, 'source_clean': True, 'target': TARGET,
        'method': 'native build on windows-latest (release workflow)', 'toolchain': toolchain,
        'pe_headers': headers, 'sha256': hashes, 'native_windows_tests': native_tests,
        'windows_authenticode': authenticode['status'], 'authenticode': authenticode,
        'provider_live_acceptance': 'NOT_RUN',
    }
    (folder / 'build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(folder.iterdir()):
            bundle.write(path, arcname=folder.name + '/' + path.name)
    receipt.update(archive=archive.name, archive_sha256=sha(archive))
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


def capture(args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--authenticode', choices=['SIGNED', 'NOT_SIGNED'], required=True)
    parser.add_argument('--signatures', type=Path, help='Get-AuthenticodeSignature report (JSON), required with SIGNED')
    parser.add_argument('--native-tests', choices=['PASS'], required=True, help='The native fixtures this job ran before packaging')
    args = parser.parse_args()
    if capture(['git', 'status', '--porcelain']):
        raise SystemExit('The source tree changed during the build; no receipt issued.')
    version = json.loads((ROOT / 'package.json').read_text())['version']
    signatures = json.loads(args.signatures.read_text(encoding='utf-8-sig')) if args.signatures else None
    if isinstance(signatures, dict):  # PowerShell writes a single object for a one-item list
        signatures = [signatures]
    subprocess.run([str(ROOT / 'target' / TARGET / 'release/switchboard.exe'), '--help'],
                   check=True, stdout=subprocess.DEVNULL)
    toolchain = {'rustc': capture(['rustc', '--version']), 'node': capture(['node', '--version']),
                 'os': platform.platform()}
    receipt = package(ROOT, version, capture(['git', 'rev-parse', 'HEAD']), args.authenticode,
                      signatures, toolchain, args.native_tests)
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
