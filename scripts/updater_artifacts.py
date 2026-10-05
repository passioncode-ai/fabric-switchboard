#!/usr/bin/env python3
"""Updater packages and their manifest (SB-55): what an installed Switchboard downloads to update
itself. Run by the release workflow only; the signing key never leaves its `release` environment.

  macos     --app <stapled .app> --out <dir>     the notarized, stapled app as .app.tar.gz + .sig
  windows   --nsis-dir <dir> --out <dir>          the (Authenticode-signed) NSIS setup + .sig
  manifest  --tag vX.Y.Z --dir <dir>              latest.json from the packages and signatures there
  check     --tag vX.Y.Z --dir <dir>              every entry of latest.json names a file in <dir>,
                                                  under this tag, with that file's own signature

Each package is signed with `tauri signer sign --app-version <version>`: the signature names the
version, and the app refuses one whose version differs from what latest.json announces
(`requireSignedVersion`). Signing reads TAURI_SIGNING_PRIVATE_KEY and
TAURI_SIGNING_PRIVATE_KEY_PASSWORD from the environment; neither is printed or written to disk.
"""
import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from release_preflight import section

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'passioncode-ai/fabric-switchboard'
APP_NAME = 'Fabric Switchboard.app'
MANIFEST = 'latest.json'
TAG = re.compile(r'^v(\d+\.\d+\.\d+)(-(?:rc|beta)\.\d+)?$')
# The universal app answers both Mac architectures; the app asks for `darwin-universal`
# (src-tauri/src/updates.rs MACOS_TARGET), the two others serve an app that asks by architecture.
MACOS_TARGETS = ('darwin-universal', 'darwin-aarch64', 'darwin-x86_64')
WINDOWS_TARGET = 'windows-x86_64'
NOTES_LIMIT = 4000


def version(root=ROOT):
    return json.loads((root / 'package.json').read_text(encoding='utf-8'))['version']


def macos_name(v):
    return f'Fabric-Switchboard-{v}-macos-universal.app.tar.gz'


def windows_name(v):
    return f'Fabric-Switchboard-{v}-windows-x64-setup.exe'


def download_url(tag, name):
    return f'https://github.com/{REPOSITORY}/releases/download/{tag}/{name.replace(" ", "%20")}'


def sign(path, v):
    """`tauri signer sign` beside the file; the key comes from the environment only."""
    for name in ('TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD'):
        if not os.environ.get(name):
            raise SystemExit(f'{name} is empty; is the release environment\'s secret set?')
    npx = 'npx.cmd' if os.name == 'nt' else 'npx'
    result = subprocess.run([npx, 'tauri', 'signer', 'sign', '--app-version', v, str(path)],
                            cwd=ROOT, capture_output=True, text=True)
    signature = Path(str(path) + '.sig')
    if result.returncode or not signature.is_file():
        # The CLI's own output names no secret; its exit code and the missing file are enough.
        raise SystemExit(f'Signing {path.name} failed (exit {result.returncode}).')
    return signature


def archive_app(app, archive):
    """The app as the updater unpacks it: one top-level `<name>.app/` entry, no AppleDouble or
    extended-attribute members, symlinks kept as symlinks (the framework layout depends on it)."""
    def clean(info):
        if Path(info.name).name.startswith('._'):
            return None
        info.uid = info.gid = 0
        info.uname = info.gname = ''
        return info
    with tarfile.open(archive, 'w:gz', format=tarfile.PAX_FORMAT) as bundle:
        bundle.add(app, arcname=app.name, filter=clean)


def verify_archive(archive):
    """Unpack the archive as the updater would and require the app inside to still pass
    codesign, the staple and Gatekeeper: a tarball that broke the seal would install a dead app."""
    with tempfile.TemporaryDirectory(prefix='switchboard-updater-') as folder:
        with tarfile.open(archive) as bundle:
            bundle.extractall(folder, filter='tar')
        app = Path(folder) / APP_NAME
        subprocess.run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(app)], check=True)
        subprocess.run(['xcrun', 'stapler', 'validate', str(app)], check=True, capture_output=True)
        subprocess.run(['/usr/sbin/spctl', '--assess', '--type', 'execute', str(app)], check=True)


def macos(app, out, v):
    app = Path(app)
    if app.name != APP_NAME or not app.is_dir():
        raise SystemExit(f'Expected the stapled {APP_NAME}, got {app}.')
    out.mkdir(parents=True, exist_ok=True)
    archive = out / macos_name(v)
    archive_app(app, archive)
    verify_archive(archive)
    return archive, sign(archive, v)


def windows(nsis_dir, out, v):
    setups = sorted(Path(nsis_dir).glob('*-setup.exe'))
    if len(setups) != 1:
        raise SystemExit(f'Expected exactly one NSIS setup in {nsis_dir}, found {len(setups)}.')
    out.mkdir(parents=True, exist_ok=True)
    setup = out / windows_name(v)
    shutil.copy2(setups[0], setup)
    return setup, sign(setup, v)


def manifest(tag, folder, changelog, now=None):
    """latest.json in Tauri's static format: one entry per platform, each with the download URL
    under this tag and the text of that file's own .sig."""
    found = TAG.match(tag)
    if not found:
        raise SystemExit(f'{tag!r} is not a release tag.')
    v = found.group(1)
    folder = Path(folder)
    platforms = {}
    for targets, name in ((MACOS_TARGETS, macos_name(v)), ((WINDOWS_TARGET,), windows_name(v))):
        package, signature = folder / name, folder / (name + '.sig')
        if not package.is_file() or not signature.is_file():
            raise SystemExit(f'{name} or its signature is missing from {folder}; latest.json not written.')
        entry = {'signature': signature.read_text(encoding='utf-8').strip(), 'url': download_url(tag, name)}
        for target in targets:
            platforms[target] = dict(entry)
    notes = section(changelog, v)[:NOTES_LIMIT] or f'Fabric Switchboard {v}.'
    moment = now or datetime.datetime.now(datetime.timezone.utc)
    document = {'version': v, 'notes': notes, 'pub_date': moment.strftime('%Y-%m-%dT%H:%M:%SZ'),
                'platforms': platforms}
    (folder / MANIFEST).write_text(json.dumps(document, indent=2) + '\n', encoding='utf-8')
    return document


def check(tag, folder):
    """Every reason latest.json would send an installed app to a file the release does not have,
    or to a file whose signature is not the one beside it; empty means it is sound."""
    folder = Path(folder)
    try:
        document = json.loads((folder / MANIFEST).read_text(encoding='utf-8'))
    except (OSError, ValueError):
        return [f'{MANIFEST} is missing or not JSON.']
    found = TAG.match(tag)
    errors = []
    if not found or document.get('version') != found.group(1):
        errors.append(f'{MANIFEST} announces {document.get("version")!r}, the tag is {tag}.')
    platforms = document.get('platforms') or {}
    for required in (*MACOS_TARGETS, WINDOWS_TARGET):
        if required not in platforms:
            errors.append(f'{MANIFEST} has no {required} entry.')
    prefix = f'https://github.com/{REPOSITORY}/releases/download/{tag}/'
    for target, entry in sorted(platforms.items()):
        url = str(entry.get('url', ''))
        if not url.startswith(prefix):
            errors.append(f'{target}: {url} is not a file of release {tag}.')
            continue
        name = url[len(prefix):].replace('%20', ' ')
        package, signature = folder / name, folder / (name + '.sig')
        if '/' in name or not package.is_file():
            errors.append(f'{target}: {name} is not among the release files.')
        elif not signature.is_file():
            errors.append(f'{target}: {name}.sig is not among the release files.')
        elif signature.read_text(encoding='utf-8').strip() != entry.get('signature'):
            errors.append(f'{target}: the signature in {MANIFEST} is not {name}.sig.')
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    mac = sub.add_parser('macos')
    mac.add_argument('--app', required=True)
    mac.add_argument('--out', required=True, type=Path)
    win = sub.add_parser('windows')
    win.add_argument('--nsis-dir', required=True)
    win.add_argument('--out', required=True, type=Path)
    for name in ('manifest', 'check'):
        command = sub.add_parser(name)
        command.add_argument('--tag', required=True)
        command.add_argument('--dir', required=True, type=Path)
    args = parser.parse_args(argv)
    if args.command == 'macos':
        for path in macos(args.app, args.out, version()):
            print(f'wrote {path.name}')
    elif args.command == 'windows':
        for path in windows(args.nsis_dir, args.out, version()):
            print(f'wrote {path.name}')
    elif args.command == 'manifest':
        document = manifest(args.tag, args.dir, (ROOT / 'CHANGELOG.md').read_text(encoding='utf-8'))
        print(f'wrote {MANIFEST}: {document["version"]}, {", ".join(sorted(document["platforms"]))}')
    else:
        errors = check(args.tag, args.dir)
        for error in errors:
            print(f'::error::{error}', file=sys.stderr)
        if errors:
            return 1
        print(f'{MANIFEST} names only files of this release, each with its own signature.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
