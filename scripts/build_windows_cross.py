#!/usr/bin/env python3
"""Cross-build Windows x64 CLI + NSIS installer; never claim native acceptance."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zipfile

from prune_artifacts import prune

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'x86_64-pc-windows-msvc'


def run(args, env=None):
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def capture(args, env=None):
    return subprocess.check_output(args, cwd=ROOT, env=env, text=True).strip()


def source_commit():
    if capture(['git', 'status', '--porcelain']):
        raise SystemExit('Commit task sources first: artifact receipts require a clean Git tree.')
    return capture(['git', 'rev-parse', 'HEAD'])


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_header(path, expected_machine):
    data = path.read_bytes()
    if data[:2] != b'MZ' or len(data) < 64:
        raise SystemExit('Invalid Windows executable header.')
    offset = struct.unpack_from('<I', data, 0x3c)[0]
    if data[offset:offset + 4] != b'PE\0\0':
        raise SystemExit('Invalid PE signature.')
    machine = struct.unpack_from('<H', data, offset + 4)[0]
    if machine != expected_machine:
        raise SystemExit('Windows executable architecture mismatch.')
    return {'machine': hex(machine), 'subsystem': struct.unpack_from('<H', data, offset + 24 + 68)[0]}


def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    commit = source_commit()
    env = os.environ.copy()
    llvm = Path('/opt/homebrew/opt/llvm/bin')
    if llvm.is_dir():
        env['PATH'] = str(llvm) + os.pathsep + env.get('PATH', '')
    # cargo-xwin resolves its own linker (including Rust's bundled rust-lld).
    for name in ('clang-cl', 'llvm-rc', 'llvm-readobj', 'makensis', 'cargo-xwin'):
        if not shutil.which(name, path=env.get('PATH')):
            raise SystemExit(f'Missing build tool: {name}. See docs/DISTRIBUTION.md.')
    env['XWIN_CACHE_DIR'] = str(ROOT / 'artifacts/xwin')
    env['XWIN_ARCH'] = 'x86_64'
    version = json.loads((ROOT / 'package.json').read_text(encoding='utf-8'))['version']
    folder = ROOT / 'artifacts' / f'Fabric-Switchboard-{version}-windows-x64'
    outputs = (folder, Path(str(folder) + '.zip'), folder.parent / (folder.name + '-receipt.json'))
    if any(path.exists() for path in outputs):
        raise SystemExit('Output already exists; preserve folder, ZIP and receipt before rebuilding.')
    # The CLI first: the installer embeds it (src-tauri/tauri.windows.conf.json resources, SB-05).
    cli_env = env.copy()
    cli_env['RUSTFLAGS'] = '-C target-feature=+crt-static'
    run(['cargo', 'xwin', 'build', '--release', '--locked', '--target', TARGET, '-p', 'switchboard-cli'], cli_env)
    run(['npm', 'exec', 'tauri', 'build', '--', '--runner', 'cargo-xwin', '--target', TARGET,
         '--bundles', 'nsis', '--no-sign', '--ci', '--', '--locked'], env)
    if source_commit() != commit:
        raise SystemExit('Source changed during build; no artifact receipt issued.')
    base = ROOT / 'target' / TARGET / 'release'
    app = base / 'fabric-switchboard.exe'
    cli = base / 'switchboard.exe'
    installer = base / 'bundle/nsis' / f'Fabric Switchboard_{version}_x64-setup.exe'
    headers = {p.name: pe_header(p, 0x14c if p == installer else 0x8664) for p in (app, cli, installer)}
    imports = capture(['llvm-readobj', '--coff-imports', str(cli)], env)
    if any(name.upper().startswith(('VCRUNTIME', 'MSVCP')) for name in re.findall(r'Name:\s+(\S+)', imports)):
        raise SystemExit('Portable CLI unexpectedly requires a separate Visual C++ runtime.')
    folder.mkdir(parents=True)
    for path in (cli, installer):
        shutil.copy2(path, folder / path.name)
    (folder / 'README.txt').write_text(
        f'Fabric Switchboard {version} Windows x64 engineering beta\n'
        'Run the NSIS setup for the desktop, or switchboard.exe --help for CLI.\n'
        'WebView2 is handled by the installer. Keep GUI or switchboard serve running for managed sessions.\n'
        'Cross-compiled with cargo-xwin and LLVM; native Windows tests and provider login have NOT run.\n'
        'Windows binaries are NOT Authenticode signed. See docs/evidence/release-0.3.md.\n')
    # The license and the third-party notices travel with every binary archive.
    for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, folder / name)
    hashes = {p.name: sha(p) for p in sorted(folder.glob('*.exe'))}
    (folder / 'SHA256SUMS.txt').write_text(''.join(f'{value}  {name}\n' for name, value in hashes.items()))
    receipt = {
        'version': version, 'commit': commit, 'source_clean': True, 'target': TARGET,
        'build_host': capture(['uname', '-sm']), 'method': 'cross-compiled; native acceptance NOT_RUN',
        'toolchain': {'rustc': capture(['rustc', '--version']), 'cargo_xwin': capture(['cargo', 'xwin', '--version'], env),
                      'llvm': capture(['clang-cl', '--version'], env).splitlines()[0], 'nsis': capture(['makensis', '-VERSION'], env)},
        'cli_rustflags': cli_env['RUSTFLAGS'], 'cli_external_vcruntime_import': False,
        'pe_headers': headers, 'sha256': hashes, 'native_windows_tests': 'NOT_RUN',
        'windows_authenticode': 'NOT_SIGNED', 'provider_live_acceptance': 'NOT_RUN',
    }
    (folder / 'build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    archive = Path(str(folder) + '.zip')
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(folder.iterdir()):
            bundle.write(path, arcname=folder.name + '/' + path.name)
    if source_commit() != commit:
        raise SystemExit('Source changed during packaging; no artifact receipt issued.')
    receipt.update(archive=archive.name, archive_sha256=sha(archive))
    receipt_path = folder.parent / (folder.name + '-receipt.json')
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    # Lifecycle LC-15: this release and the one before stay; older binaries go (receipts stay).
    for path in prune(ROOT / 'artifacts'):
        print(f'pruned {path.name}', file=sys.stderr)
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
