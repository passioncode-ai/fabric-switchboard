#!/usr/bin/env python3
"""Build and Developer-ID sign app + CLI; preserve resumable notarization receipts.

Three ways to notarize, chosen explicitly:
  --notary-profile P       a saved notarytool profile on this Mac (local; never in CI);
  --external-notarization  the caller notarizes: the release workflow notarizes and staples the
                           app, then runs --finish-external, which notarizes the CLI with the App
                           Store Connect API key from the environment and repackages;
  --allow-unnotarized      a local engineering build; the receipt says NOT_RUN.
A build signed anywhere but the release workflow is a debug build and is never published.
"""
import argparse
import base64
import contextlib
import os
from smoke_native import verify as verify_native_startup
from prune_artifacts import prune, unregister_app
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

APP_NAME = 'Fabric Switchboard.app'
TEAM_VARIABLE = 'SWITCHBOARD_SIGNING_TEAM'
ASC_VARIABLES = ('ASC_KEY_ID', 'ASC_ISSUER_ID', 'ASC_API_KEY_P8_B64')
CLI_STAPLE_REASON = ('A bare Mach-O cannot carry a stapled ticket; Gatekeeper looks the ticket up '
                     'online by the code hash the first time a quarantined copy runs.')


def pending_notarization(mode):
    """What the receipt says before notarization has happened, by mode."""
    if mode == 'external':
        return {'status': 'PENDING_EXTERNAL', 'by': 'the release workflow (App Store Connect API key)'}
    if mode == 'profile':
        return {'status': 'PENDING', 'by': 'saved notarytool profile'}
    return {'status': 'NOT_RUN', 'reason': 'Local engineering build (--allow-unnotarized)'}


def signing_team(identity_name, environ):
    """The team the build compiles in: the identity's own, which a team the environment
    names (`SWITCHBOARD_SIGNING_TEAM`, from the release environment's APPLE_TEAM_ID) must equal."""
    found = re.search(r'\(([A-Z0-9]{10})\)$', identity_name)
    if not found:
        raise SystemExit('The Developer ID identity names no team; refusing to guess one.')
    team = found.group(1)
    given = environ.get(TEAM_VARIABLE)
    if given and given != team:
        raise SystemExit(f'{TEAM_VARIABLE} names a different team than the signing identity; refusing to build.')
    return team


def require_team_compiled(binaries, team):
    """An executable built without the team trusts no team and would use the development
    Keychain namespace (docs/KEYCHAIN.md); checked before signing adds the certificate."""
    for binary in binaries:
        if team.encode() not in Path(binary).read_bytes():
            raise SystemExit(f'{Path(binary).name} was compiled without {TEAM_VARIABLE}; it would be a development build.')


@contextlib.contextmanager
def asc_key(environ):
    """notarytool arguments for the App Store Connect API key named by the environment. The key
    is written to a 0600 file in a private temporary directory and removed after the call."""
    missing = [name for name in ASC_VARIABLES if not environ.get(name)]
    if missing:
        raise SystemExit(f'{", ".join(missing)} is empty; is the release environment\'s secret set?')
    with tempfile.TemporaryDirectory(prefix='switchboard-asc-') as private:
        os.chmod(private, 0o700)
        key = Path(private) / 'key.p8'
        descriptor = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'wb') as handle:
            handle.write(base64.b64decode(environ['ASC_API_KEY_P8_B64']))
        yield ['--key', str(key), '--key-id', environ['ASC_KEY_ID'], '--issuer', environ['ASC_ISSUER_ID']]


def notarytool(verb, credentials):
    """Run `xcrun notarytool <verb…> <credentials> --output-format json` and return its JSON
    (empty when it printed none). Its own output carries no credential."""
    result = subprocess.run(['xcrun', 'notarytool', *verb, *credentials, '--output-format', 'json'],
                            cwd=ROOT, capture_output=True, text=True)
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {}


def parse_cdhash(codesign_output):
    found = re.search(r'^CDHash=([0-9A-Fa-f]+)$', codesign_output, re.MULTILINE)
    if not found:
        raise SystemExit('codesign printed no CDHash for the CLI.')
    return found.group(1).lower()


def cli_cdhashes(cli):
    """The code hash of each architecture slice: what a notarization ticket is issued for."""
    hashes = []
    for arch in capture(['/usr/bin/lipo', '-archs', str(cli)]).split():
        result = run(['/usr/bin/codesign', '-d', '-vvv', '--arch', arch, str(cli)], capture_output=True, text=True)
        hashes.append(parse_cdhash(result.stderr))
    return hashes


def ticket_cdhashes(log):
    return {str(item.get('cdhash', '')).lower() for item in log.get('ticketContents') or [] if item.get('cdhash')}


def checked_receipt(receipt_path):
    """Load a receipt from this repository's artifacts and check its archive before trusting it."""
    receipt_path = receipt_path.resolve()
    if not receipt_path.is_relative_to(ARTIFACTS.resolve()):
        raise SystemExit('Resume receipt must be inside this repository’s artifacts directory.')
    receipt = json.loads(receipt_path.read_text(encoding='utf-8'))
    for key in ('archive', 'folder'):
        if Path(receipt[key]).name != receipt[key] or receipt[key] in ('.', '..'):
            raise SystemExit('Invalid artifact receipt paths.')
    archive = (ARTIFACTS / receipt['archive']).resolve()
    if archive.parent != ARTIFACTS.resolve() or sha(archive) != receipt['archive_sha256']:
        raise SystemExit('Artifact receipt paths/hash do not match; notarization refused.')
    return receipt_path, receipt, archive


def finish_external(receipt_path, app_submission, environ):
    """After the workflow notarized and stapled the app inside the build folder: check the
    staple and Gatekeeper, notarize the standalone CLI (zipped, the only form Apple takes for a
    bare executable), require the ticket to cover every slice of it, then repackage the folder
    and write what actually happened into the receipt."""
    receipt_path, receipt, archive = checked_receipt(receipt_path)
    if receipt.get('notarization', {}).get('status') != 'PENDING_EXTERNAL':
        raise SystemExit('This receipt is not waiting for the release workflow’s notarization.')
    folder = (ARTIFACTS / receipt['folder']).resolve()
    app, cli = folder / APP_NAME, folder / 'switchboard'
    if folder.parent != ARTIFACTS.resolve() or sha(cli) != receipt['cli_sha256']:
        raise SystemExit('The build folder does not match the signed receipt; nothing submitted.')
    for item in (app, cli):
        run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(item)])
    run(['xcrun', 'stapler', 'validate', str(app)])
    run(['/usr/sbin/spctl', '--assess', '--type', 'execute', '--verbose=2', str(app)])
    notarization = {'status': 'CLI_PENDING', 'by': 'the release workflow (App Store Connect API key)',
                    'app': {'submission': app_submission, 'status': 'Accepted', 'stapled': True,
                            'staple_validated': True, 'gatekeeper_accepted': True}}
    hashes = cli_cdhashes(cli)
    with asc_key(environ) as credentials, tempfile.TemporaryDirectory(prefix='.cli-notary-', dir=ARTIFACTS) as staging:
        upload = Path(staging) / 'switchboard.zip'
        run(['/usr/bin/ditto', '-c', '-k', '--keepParent', str(cli), str(upload)])
        submitted = notarytool(['submit', str(upload), '--wait', '--timeout', '45m'], credentials)
        cli_status = {'submission': submitted.get('id'), 'status': submitted.get('status') or 'SUBMIT_FAILED',
                      'stapled': False, 'reason_not_stapled': CLI_STAPLE_REASON}
        notarization['cli'] = cli_status
        receipt['notarization'] = notarization
        save(receipt_path, receipt)
        if cli_status['status'] != 'Accepted' or not cli_status['submission']:
            if cli_status['submission']:
                print(json.dumps(notarytool(['log', cli_status['submission']], credentials), indent=2), file=sys.stderr)
            raise SystemExit('Apple did not accept the CLI; submission and status retained in the receipt.')
        covered = ticket_cdhashes(notarytool(['log', cli_status['submission']], credentials))
    if not set(hashes) <= covered:
        cli_status['status'] = 'TICKET_INCOMPLETE'
        save(receipt_path, receipt)
        raise SystemExit('The notarization ticket does not cover every slice of the CLI; not called notarized.')
    cli_status['ticket_cdhashes'] = hashes
    notarization['status'] = 'Accepted'
    with tempfile.TemporaryDirectory(prefix='.notary-', dir=ARTIFACTS) as temporary:
        replacement = Path(temporary) / 'stapled.zip'
        archive_folder(folder, replacement)
        # Atomic publication keeps the original ZIP intact on failure.
        replacement.replace(archive)
    receipt['archive_sha256'] = sha(archive)
    save(receipt_path, receipt)
    return receipt

def notarize(receipt_path, profile):
    receipt_path, receipt, archive = checked_receipt(receipt_path)
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
        run(['/usr/sbin/spctl', '--assess', '--type', 'execute', '--verbose=2', str(app)])
        receipt['notarization']['gatekeeper_accepted'] = True
        replacement = staging / 'stapled.zip'
        archive_folder(folder, replacement)
        # Atomic publication keeps the original ZIP intact on failure.
        replacement.replace(archive)
    receipt['notarization']['archive_app_staple_validated'] = True
    receipt['archive_sha256'] = sha(archive)
    save(receipt_path, receipt)
    return receipt

def build_parser():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--identity', help='Exact installed Developer ID Application name or SHA-1')
    parser.add_argument('--notary-profile', help='Existing notarytool profile name, never a password')
    parser.add_argument('--notarize-existing', type=Path, help='Resume from an existing adjacent receipt JSON')
    parser.add_argument('--allow-unnotarized', action='store_true', help='Explicit local engineering build; does not pass public Gatekeeper acceptance')
    parser.add_argument('--external-notarization', action='store_true', help='The caller notarizes (the release workflow); the receipt says PENDING_EXTERNAL until --finish-external')
    parser.add_argument('--finish-external', type=Path, metavar='RECEIPT', help='After the caller notarized and stapled the app: notarize the CLI with ASC_* from the environment and repackage')
    parser.add_argument('--app-submission', help='With --finish-external: the app’s accepted notarization submission id')
    parser.add_argument('--arch', choices=['universal', 'arm64'], default='universal')
    return parser


def notarization_mode(parser, args):
    """One explicit decision per run: profile, external, unnotarized, resume or finish."""
    if args.finish_external:
        if not args.app_submission:
            parser.error('--finish-external requires --app-submission')
        return 'finish'
    if args.external_notarization and (args.notary_profile or args.allow_unnotarized or args.notarize_existing):
        parser.error('--external-notarization means the caller notarizes; do not combine it with a profile or --allow-unnotarized')
    if args.notarize_existing:
        if not args.notary_profile:
            parser.error('--notarize-existing requires --notary-profile')
        return 'resume'
    if args.external_notarization:
        return 'external'
    if args.notary_profile:
        return 'profile'
    if args.allow_unnotarized:
        return 'unnotarized'
    parser.error('Public macOS builds require --notary-profile or --external-notarization. For local engineering only, explicitly use --allow-unnotarized.')


def main():
    parser = build_parser()
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This command requires macOS.')
    mode = notarization_mode(parser, args)
    # Use a working installed toolchain without accepting a license or changing xcode-select.
    if 'DEVELOPER_DIR' not in os.environ:
        probe = subprocess.run(['xcrun', '--show-sdk-version'], capture_output=True)
        clt = Path('/Library/Developer/CommandLineTools')
        if probe.returncode and clt.is_dir():
            env = dict(os.environ, DEVELOPER_DIR=str(clt))
            if subprocess.run(['xcrun', '--show-sdk-version'], env=env, capture_output=True).returncode == 0:
                os.environ['DEVELOPER_DIR'] = str(clt)
                print('Using installed Command Line Tools for this build only.')
    if mode == 'finish':
        print(json.dumps(finish_external(args.finish_external, args.app_submission, os.environ), indent=2))
        return
    if mode == 'resume':
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
    # The Keychain code trusts the team compiled into it (docs/KEYCHAIN.md); every cargo
    # process below inherits it.
    team = signing_team(identity_name, os.environ)
    os.environ[TEAM_VARIABLE] = team
    version = json.loads((ROOT/'package.json').read_text(encoding='utf-8'))['version']
    folder = ARTIFACTS / f'Fabric-Switchboard-{version}-macos-{args.arch}'
    outputs = (folder, Path(str(folder)+'.zip'), folder.parent/(folder.name+'-receipt.json'))
    if any(path.exists() for path in outputs):
        raise SystemExit('Output already exists; preserve folder, ZIP and receipt before rebuilding or resume notarization.')
    target = 'universal-apple-darwin' if args.arch == 'universal' else 'aarch64-apple-darwin'
    run(['npm', 'exec', 'tauri', 'build', '--', '--target', target, '--bundles', 'app', '--no-sign', '--ci', '--', '--locked'])
    folder.mkdir(parents=True)
    app = folder / 'Fabric Switchboard.app'
    built = ROOT/'target'/target/'release/bundle/macos/Fabric Switchboard.app'
    shutil.copytree(built, app)
    # The bundle under target/ is a build intermediate; it must not answer `open -b` (LC-15).
    unregister_app(built)
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
    require_team_compiled([cli, app/'Contents/MacOS/fabric-switchboard'], team)
    # The CLI also travels inside the app, so the desktop can hand agents `switchboard mcp`
    # and link it into ~/.local/bin. Nested code is signed before the bundle that seals it.
    run(['/usr/bin/codesign', '--force', '--options', 'runtime', '--timestamp', '--sign', identity_hash, str(cli)])
    embedded = app/'Contents/MacOS/switchboard'
    shutil.copy2(cli, embedded)
    for binary in (cli, app):
        if binary == app:
            run(['/usr/bin/codesign', '--force', '--options', 'runtime', '--timestamp', '--sign', identity_hash, str(binary)])
        run(['/usr/bin/codesign', '--verify', '--deep', '--strict', '--verbose=2', str(binary)])
    if sha(embedded) != sha(cli):
        raise SystemExit('Embedded CLI differs from the archived CLI.')
    startup = verify_native_startup(app/'Contents/MacOS/fabric-switchboard', version)
    # The lifecycle broker's always-on start: hidden window, unknown argument ignored (SB-30).
    startup_background = verify_native_startup(app/'Contents/MacOS/fabric-switchboard', version, background=True)
    (folder/'README.txt').write_text(f'Fabric Switchboard {version}\nMove the app to Applications. CLI: ./switchboard --help\nKeep GUI or switchboard serve open for managed sessions. See repository docs/CLI.md.\nSignature and notarization differ; see the adjacent release receipt JSON.\n')
    # The license and the third-party notices travel with every binary archive.
    for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT/name, folder/name)
    archive = Path(str(folder)+'.zip')
    archive_folder(folder, archive)
    verify_source(commit)
    toolchain = {'rustc': capture(['rustc', '--version']), 'node': capture(['node', '--version']), 'macos': capture(['sw_vers', '-productVersion']), 'sdk': capture(['xcrun', '--show-sdk-version'])}
    receipt = {'version':version, 'commit':commit, 'source_clean':True, 'toolchain':toolchain, 'architectures':architectures, 'developer_id_identity':identity_name, 'signing_certificate_sha1':identity_hash, 'signing_team':team, 'signature_verified':True, 'native_startup':startup, 'native_startup_background':startup_background, 'notarization':pending_notarization(mode), 'cli_sha256':sha(cli), 'archive_sha256':sha(archive), 'archive':archive.name, 'folder':folder.name}
    receipt_path = ARTIFACTS/(folder.name+'-receipt.json')
    save(receipt_path, receipt)
    if mode == 'profile':
        receipt = notarize(receipt_path, args.notary_profile)
    # Lifecycle LC-15: this release and the one before stay; older binaries go (receipts stay).
    for path in prune(ARTIFACTS):
        print(f'pruned {path.name}', file=sys.stderr)
    print(json.dumps(receipt, indent=2))

if __name__ == '__main__':
    main()
