#!/usr/bin/env python3
"""Build and Developer-ID sign app + CLI; preserve resumable notarization receipts."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / 'artifacts'

def run(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)

def capture(args):
    return run(args, capture_output=True, text=True).stdout.strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def source_commit():
    if capture(['git', 'status', '--porcelain']):
        raise SystemExit('Commit task sources first: a signed build requires a clean Git working tree.')
    return capture(['git', 'rev-parse', 'HEAD'])

def verify_source(expected):
    if source_commit() != expected:
        raise SystemExit('Source changed during build; output retained but no release attestation issued.')

def save(path, receipt):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(receipt, indent=2)+'\n')
    temporary.replace(path)

def archive_folder(folder, archive):
    run(['/usr/bin/ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(folder), str(archive)])

def notarize(receipt_path, profile):
    receipt_path = receipt_path.resolve()
    if not receipt_path.is_relative_to(ARTIFACTS.resolve()):
        raise SystemExit('Resume receipt must be inside this repository’s artifacts directory.')
    receipt = json.loads(receipt_path.read_text())
    for key in ('archive', 'folder'):
        if Path(receipt[key]).name != receipt[key] or receipt[key] in ('.', '..'):
            raise SystemExit('Invalid artifact receipt paths.')
    archive = (ARTIFACTS / receipt['archive']).resolve()
    if archive.parent != ARTIFACTS.resolve() or sha(archive) != receipt['archive_sha256']:
        raise SystemExit('Artifact receipt paths/hash do not match; notarization refused.')
    # Resume never trusts the separately mutable unpacked build folder.
    with zipfile.ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            path = Path(entry.filename)
            if path.is_absolute() or '..' in path.parts:
                raise SystemExit('Unsafe artifact archive path.')
    status = receipt.get('notarization', {})
    submission = status.get('id')
    if not submission:
        result = subprocess.run(['xcrun', 'notarytool', 'submit', str(archive), '--keychain-profile', profile, '--output-format', 'json'], cwd=ROOT, capture_output=True, text=True)
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError:
            parsed = {}
        submission = parsed.get('id')
        receipt['notarization'] = {'id': submission, 'status': parsed.get('status', 'SUBMIT_FAILED'), 'exit_code': result.returncode}
        save(receipt_path, receipt)
        if result.returncode or not submission:
            raise SystemExit('Notary submission failed; signed output and sanitized receipt retained. Check profile authorization.')
    result = subprocess.run(['xcrun', 'notarytool', 'wait', submission, '--keychain-profile', profile, '--output-format', 'json'], cwd=ROOT, capture_output=True, text=True)
    try:
        parsed = json.loads(result.stdout)
    except json.JSONDecodeError:
        parsed = {}
    receipt['notarization'] = {'id': submission, 'status': parsed.get('status', 'WAIT_FAILED'), 'exit_code': result.returncode}
    save(receipt_path, receipt)
    if result.returncode or parsed.get('status') != 'Accepted':
        raise SystemExit('Notarization not Accepted; submission ID/status retained in receipt. No notarized claim issued.')
    with tempfile.TemporaryDirectory(prefix='.notary-', dir=ARTIFACTS) as temporary:
        staging = Path(temporary)
        run(['/usr/bin/ditto', '-x', '-k', str(archive), str(staging)])
        folder = staging / receipt['folder']
        app = folder / 'Fabric Switchboard.app'
        cli = folder / 'switchboard'
        if not folder.resolve().is_relative_to(staging.resolve()) or sha(cli) != receipt['cli_sha256']:
            raise SystemExit('Extracted artifact does not match signed receipt.')
        for item in (app, cli):
            run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(item)])
        run(['xcrun', 'stapler', 'staple', str(app)])
        run(['xcrun', 'stapler', 'validate', str(app)])
        replacement = staging / 'stapled.zip'
        archive_folder(folder, replacement)
        # Atomic publication keeps the original ZIP intact on failure.
        replacement.replace(archive)
    receipt['notarization']['archive_app_staple_validated'] = True
    receipt['archive_sha256'] = sha(archive)
    save(receipt_path, receipt)
    return receipt

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--identity', help='Exact installed Developer ID Application name or SHA-1')
    parser.add_argument('--notary-profile', help='Existing notarytool profile name, never a password')
    parser.add_argument('--notarize-existing', type=Path, help='Resume from an existing adjacent receipt JSON')
    parser.add_argument('--arch', choices=['universal', 'arm64'], default='universal')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This command requires macOS.')
    if args.notarize_existing:
        if not args.notary_profile:
            parser.error('--notarize-existing requires --notary-profile')
        print(json.dumps(notarize(args.notarize_existing, args.notary_profile), indent=2))
        return
    if not args.identity:
        parser.error('--identity is required for a new build')
    commit = source_commit()
    identities = capture(['security', 'find-identity', '-v', '-p', 'codesigning'])
    choices = re.findall(r'([0-9A-F]{40}) "(Developer ID Application: [^"]+)"', identities)
    match = next(((digest, name) for digest, name in choices if args.identity in (digest, name)), None)
    if match is None:
        raise SystemExit('Exact requested Developer ID identity is unavailable; no fallback signer used.')
    identity_hash, identity_name = match
    version = json.loads((ROOT/'package.json').read_text())['version']
    folder = ARTIFACTS / f'Fabric-Switchboard-{version}-macos-{args.arch}'
    if folder.exists():
        raise SystemExit('Output folder exists; preserve/rename it before rebuilding or resume its notarization receipt.')
    target = 'universal-apple-darwin' if args.arch == 'universal' else 'aarch64-apple-darwin'
    run(['npm', 'exec', 'tauri', 'build', '--', '--target', target, '--bundles', 'app', '--no-sign', '--ci', '--', '--locked'])
    folder.mkdir(parents=True)
    app = folder / 'Fabric Switchboard.app'
    shutil.copytree(ROOT/'target'/target/'release/bundle/macos/Fabric Switchboard.app', app)
    cli = folder/'switchboard'
    targets = ('aarch64-apple-darwin', 'x86_64-apple-darwin') if args.arch == 'universal' else (target,)
    for item in targets:
        run(['cargo', 'build', '--release', '--locked', '-p', 'switchboard-cli', '--target', item])
    if args.arch == 'universal':
        run(['/usr/bin/lipo', '-create', *[str(ROOT/'target'/item/'release/switchboard') for item in targets], '-output', str(cli)])
    else:
        shutil.copy2(ROOT/'target'/target/'release/switchboard', cli)
    verify_source(commit)
    architectures = {name: capture(['/usr/bin/lipo', '-archs', str(binary)]).split() for name,binary in [('cli', cli), ('app', app/'Contents/MacOS/fabric-switchboard')]}
    expected_arch = {'arm64','x86_64'} if args.arch == 'universal' else {'arm64'}
    if any(set(value) != expected_arch for value in architectures.values()):
        raise SystemExit('Built architecture does not match requested target.')
    for binary in (cli, app):
        run(['/usr/bin/codesign', '--force', '--options', 'runtime', '--timestamp', '--sign', identity_hash, str(binary)])
        run(['/usr/bin/codesign', '--verify', '--deep', '--strict', '--verbose=2', str(binary)])
    (folder/'README.txt').write_text(f'Fabric Switchboard {version}\nMove the app to Applications. CLI: ./switchboard --help\nKeep GUI or switchboard serve open for managed sessions. See repository docs/CLI.md.\nSignature and notarization differ; see the adjacent release receipt JSON.\n')
    archive = Path(str(folder)+'.zip')
    archive_folder(folder, archive)
    verify_source(commit)
    toolchain = {'rustc': capture(['rustc', '--version']), 'node': capture(['node', '--version']), 'macos': capture(['sw_vers', '-productVersion']), 'sdk': capture(['xcrun', '--show-sdk-version'])}
    receipt = {'version':version, 'commit':commit, 'source_clean':True, 'toolchain':toolchain, 'architectures':architectures, 'developer_id_identity':identity_name, 'signing_certificate_sha1':identity_hash, 'signature_verified':True, 'notarization':{'status':'NOT_RUN','reason':'No notarytool profile supplied'}, 'cli_sha256':sha(cli), 'archive_sha256':sha(archive), 'archive':archive.name, 'folder':folder.name}
    receipt_path = ARTIFACTS/(folder.name+'-receipt.json')
    save(receipt_path, receipt)
    if args.notary_profile:
        receipt = notarize(receipt_path, args.notary_profile)
    print(json.dumps(receipt, indent=2))

if __name__ == '__main__':
    main()
