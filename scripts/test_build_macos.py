"""Tests for scripts/build_macos.py: the CI inputs (identity, team, the caller notarizes) and the
receipt's notarization claims. No signing, notarization or build runs here; every external tool
is replaced, and every credential is a synthetic placeholder that is not a key."""
import base64
import contextlib
import io
import json
import os
import pathlib
import stat
import sys
import tempfile
import unittest
import zipfile
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import build_macos  # noqa: E402

IDENTITY = 'Developer ID Application: Example Person (ABCDE12345)'
FAKE_ENV = {
    'ASC_KEY_ID': 'synthetic-key-id',
    'ASC_ISSUER_ID': 'synthetic-issuer',
    'ASC_API_KEY_P8_B64': base64.b64encode(b'not a key, a test placeholder').decode(),
}


def parse(*argv):
    parser = build_macos.build_parser()
    args = parser.parse_args(list(argv))
    with contextlib.redirect_stderr(io.StringIO()):
        return build_macos.notarization_mode(parser, args)


class Modes(unittest.TestCase):
    def test_a_build_needs_a_notarization_decision(self):
        with self.assertRaises(SystemExit):
            parse('--identity', IDENTITY)

    def test_the_three_modes(self):
        self.assertEqual(parse('--identity', IDENTITY, '--notary-profile', 'p'), 'profile')
        self.assertEqual(parse('--identity', IDENTITY, '--allow-unnotarized'), 'unnotarized')
        self.assertEqual(parse('--identity', IDENTITY, '--external-notarization'), 'external')

    def test_the_caller_notarizing_excludes_the_other_modes(self):
        for other in (['--notary-profile', 'p'], ['--allow-unnotarized']):
            with self.assertRaises(SystemExit):
                parse('--identity', IDENTITY, '--external-notarization', *other)

    def test_finishing_needs_the_app_submission(self):
        with self.assertRaises(SystemExit):
            parse('--finish-external', 'artifacts/r.json')
        self.assertEqual(parse('--finish-external', 'artifacts/r.json', '--app-submission', 'x'), 'finish')

    def test_the_receipt_never_says_not_run_when_the_caller_notarizes(self):
        pending = build_macos.pending_notarization('external')
        self.assertEqual(pending['status'], 'PENDING_EXTERNAL')
        self.assertEqual(build_macos.pending_notarization('unnotarized')['status'], 'NOT_RUN')
        self.assertNotEqual(build_macos.pending_notarization('profile')['status'], 'NOT_RUN')


class Team(unittest.TestCase):
    def test_the_team_comes_from_the_identity(self):
        self.assertEqual(build_macos.signing_team(IDENTITY, {}), 'ABCDE12345')

    def test_a_team_given_by_the_environment_must_match_the_identity(self):
        self.assertEqual(build_macos.signing_team(IDENTITY, {'SWITCHBOARD_SIGNING_TEAM': 'ABCDE12345'}), 'ABCDE12345')
        with self.assertRaises(SystemExit):
            build_macos.signing_team(IDENTITY, {'SWITCHBOARD_SIGNING_TEAM': 'ZYXWV98765'})

    def test_an_identity_without_a_team_is_refused(self):
        with self.assertRaises(SystemExit):
            build_macos.signing_team('Developer ID Application: Nobody', {})

    def test_binaries_must_have_the_team_compiled_in(self):
        with tempfile.TemporaryDirectory() as folder:
            good = pathlib.Path(folder, 'good')
            good.write_bytes(b'\0prefix ABCDE12345 suffix\0')
            bad = pathlib.Path(folder, 'bad')
            bad.write_bytes(b'\0no team here\0')
            build_macos.require_team_compiled([good], 'ABCDE12345')
            with self.assertRaises(SystemExit):
                build_macos.require_team_compiled([good, bad], 'ABCDE12345')

    def test_no_team_id_is_written_in_the_product_code(self):
        # The team reaches the code only through SWITCHBOARD_SIGNING_TEAM at build time.
        source = (build_macos.ROOT / 'crates/switchboard-core/src/keychain_macos.rs').read_text()
        source = source.split('#[cfg(test)]\nmod tests')[0]  # the tests use a synthetic id
        self.assertIn('option_env!("SWITCHBOARD_SIGNING_TEAM")', source)
        self.assertNotRegex(source, r'"[A-Z0-9]{10}"')


class Credentials(unittest.TestCase):
    def test_a_missing_variable_is_named_and_no_value_is_printed(self):
        env = dict(FAKE_ENV, ASC_ISSUER_ID='')
        with self.assertRaises(SystemExit) as caught:
            with build_macos.asc_key(env):
                pass
        message = str(caught.exception)
        self.assertIn('ASC_ISSUER_ID', message)
        for value in FAKE_ENV.values():
            self.assertNotIn(value, message)

    def test_the_key_lives_in_a_private_file_only_for_the_call(self):
        with build_macos.asc_key(FAKE_ENV) as creds:
            key = pathlib.Path(creds[creds.index('--key') + 1])
            self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o600)
            self.assertEqual(key.read_bytes(), b'not a key, a test placeholder')
            self.assertNotIn(FAKE_ENV['ASC_API_KEY_P8_B64'], creds)
        self.assertFalse(key.exists())


class Parsing(unittest.TestCase):
    def test_cdhash_from_codesign(self):
        text = 'Executable=/x/switchboard\nIdentifier=switchboard\nCDHash=0A1B2C3D4E5F60718293A4B5C6D7E8F901234567\n'
        self.assertEqual(build_macos.parse_cdhash(text), '0a1b2c3d4e5f60718293a4b5c6d7e8f901234567')
        with self.assertRaises(SystemExit):
            build_macos.parse_cdhash('Identifier=switchboard\n')

    def test_ticket_cdhashes_from_the_notary_log(self):
        log = {'status': 'Accepted', 'ticketContents': [
            {'path': 'switchboard', 'arch': 'arm64', 'cdhash': 'AA11'},
            {'path': 'switchboard', 'arch': 'x86_64', 'cdhash': 'bb22'}]}
        self.assertEqual(build_macos.ticket_cdhashes(log), {'aa11', 'bb22'})
        self.assertEqual(build_macos.ticket_cdhashes({'status': 'Accepted'}), set())


class FinishExternal(unittest.TestCase):
    """The workflow notarized and stapled the app; the script notarizes the CLI, checks the
    ticket covers it, repackages and writes what actually happened."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.artifacts = pathlib.Path(self.temp.name, 'artifacts')
        folder = self.artifacts / 'Fabric-Switchboard-9.9.9-macos-universal'
        (folder / 'Fabric Switchboard.app/Contents/MacOS').mkdir(parents=True)
        (folder / 'switchboard').write_bytes(b'cli bytes')
        self.archive = self.artifacts / (folder.name + '.zip')
        self.zip(folder, self.archive)
        self.receipt_path = self.artifacts / (folder.name + '-receipt.json')
        self.receipt_path.write_text(json.dumps({
            'version': '9.9.9', 'folder': folder.name, 'archive': self.archive.name,
            'archive_sha256': build_macos.sha(self.archive), 'cli_sha256': build_macos.sha(folder / 'switchboard'),
            'notarization': build_macos.pending_notarization('external')}))
        self.calls = []
        patches = [
            mock.patch.object(build_macos, 'ARTIFACTS', self.artifacts),
            mock.patch.object(build_macos, 'run', side_effect=lambda args, **_: self.calls.append(args)),
            mock.patch.object(build_macos, 'archive_folder', side_effect=self.zip),
            mock.patch.object(build_macos, 'cli_cdhashes', return_value=['aa11', 'bb22']),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.addCleanup(self.temp.cleanup)

    @staticmethod
    def zip(folder, archive):
        with zipfile.ZipFile(archive, 'w') as bundle:
            for path in sorted(folder.rglob('*')):
                bundle.write(path, arcname=str(path.relative_to(folder.parent)))
        return archive

    def notary(self, ticket):
        def answer(verb, *_args, **_kwargs):
            if verb[0] == 'submit':
                return {'id': 'sub-cli', 'status': 'Accepted'}
            if verb[0] == 'log':
                return {'status': 'Accepted', 'ticketContents': [{'cdhash': c} for c in ticket]}
            raise AssertionError(verb)
        return answer

    def test_accepted_and_covered(self):
        with mock.patch.object(build_macos, 'notarytool', side_effect=self.notary(['AA11', 'BB22'])):
            receipt = build_macos.finish_external(self.receipt_path, 'sub-app', FAKE_ENV)
        n = receipt['notarization']
        self.assertEqual(n['status'], 'Accepted')
        self.assertEqual(n['app'], {'submission': 'sub-app', 'status': 'Accepted', 'stapled': True,
                                    'staple_validated': True, 'gatekeeper_accepted': True})
        self.assertEqual(n['cli']['submission'], 'sub-cli')
        self.assertEqual(n['cli']['status'], 'Accepted')
        self.assertFalse(n['cli']['stapled'])
        self.assertEqual(n['cli']['ticket_cdhashes'], ['aa11', 'bb22'])
        self.assertEqual(receipt['archive_sha256'], build_macos.sha(self.archive))
        self.assertEqual(json.loads(self.receipt_path.read_text()), receipt)
        commands = [' '.join(map(str, c)) for c in self.calls]
        self.assertTrue(any('stapler validate' in c for c in commands))
        self.assertTrue(any('spctl --assess' in c for c in commands))
        self.assertFalse(any(FAKE_ENV['ASC_API_KEY_P8_B64'] in c for c in commands))

    def test_a_ticket_that_misses_the_cli_is_not_called_notarized(self):
        before = self.receipt_path.read_text()
        with mock.patch.object(build_macos, 'notarytool', side_effect=self.notary(['aa11'])):
            with self.assertRaises(SystemExit):
                build_macos.finish_external(self.receipt_path, 'sub-app', FAKE_ENV)
        receipt = json.loads(self.receipt_path.read_text())
        self.assertNotEqual(receipt['notarization'].get('status'), 'Accepted')
        self.assertNotEqual(before, '')

    def test_apple_refusing_the_cli_fails_and_keeps_the_submission(self):
        def refused(verb, *_a, **_k):
            if verb[0] == 'submit':
                return {'id': 'sub-cli', 'status': 'Invalid'}
            return {'status': 'Invalid', 'issues': []}
        with mock.patch.object(build_macos, 'notarytool', side_effect=refused):
            with self.assertRaises(SystemExit):
                build_macos.finish_external(self.receipt_path, 'sub-app', FAKE_ENV)
        receipt = json.loads(self.receipt_path.read_text())
        self.assertEqual(receipt['notarization']['cli']['submission'], 'sub-cli')
        self.assertEqual(receipt['notarization']['cli']['status'], 'Invalid')
        self.assertNotEqual(receipt['notarization']['status'], 'Accepted')

    def test_a_changed_cli_is_refused_before_any_submission(self):
        folder = self.artifacts / 'Fabric-Switchboard-9.9.9-macos-universal'
        (folder / 'switchboard').write_bytes(b'other bytes')
        with mock.patch.object(build_macos, 'notarytool') as notary:
            with self.assertRaises(SystemExit):
                build_macos.finish_external(self.receipt_path, 'sub-app', FAKE_ENV)
            notary.assert_not_called()

    def test_only_a_receipt_still_waiting_for_the_caller_is_finished(self):
        receipt = json.loads(self.receipt_path.read_text())
        receipt['notarization'] = build_macos.pending_notarization('unnotarized')
        self.receipt_path.write_text(json.dumps(receipt))
        with self.assertRaises(SystemExit):
            build_macos.finish_external(self.receipt_path, 'sub-app', FAKE_ENV)


if __name__ == '__main__':
    unittest.main()
