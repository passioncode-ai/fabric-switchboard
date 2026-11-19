"""Tests for scripts/feed_check.py: after a publish, the feed must serve the tag with a package
that verifies as the app's updater verifies it (SB-563, M4-08)."""
import base64
import hashlib
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import feed_check  # noqa: E402
from updater_artifacts import REQUIRED_TARGETS, download_url  # noqa: E402

# RFC 8032 section 7.1, tests 1 and 2.
RFC = [
    ('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60',
     'd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a', '',
     'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b'),
    ('4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb',
     '3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c', '72',
     '92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00'),
]
SEED = bytes(range(32))
KEY_ID = bytes.fromhex('0102030405060708')
VERSION = '1.2.3'
TAG = f'v{VERSION}'
PACKAGE = b'the installer bytes' * 100


def public_file(seed=SEED, key_id=KEY_ID):
    raw = b'Ed' + key_id + feed_check.ed25519_public(seed)
    return f'untrusted comment: minisign public key: {key_id.hex().upper()}\n{base64.b64encode(raw).decode()}\n'


def signature_file(data, comment, seed=SEED, key_id=KEY_ID, prehashed=True):
    message = hashlib.blake2b(data, digest_size=64).digest() if prehashed else data
    signature = feed_check.ed25519_sign(seed, message)
    global_signature = feed_check.ed25519_sign(seed, signature + comment.encode())
    line = base64.b64encode((b'ED' if prehashed else b'Ed') + key_id + signature).decode()
    return (f'untrusted comment: signature from tauri secret key\n{line}\n'
            f'trusted comment: {comment}\n{base64.b64encode(global_signature).decode()}\n')


def b64(text):
    return base64.b64encode(text.encode()).decode()


def comment(version=VERSION):
    return f'timestamp:1700000000\tfile:setup.exe\tversion:{version}'


def feed(version=VERSION, tag=TAG, signature=None):
    sig = b64(signature or signature_file(PACKAGE, comment(version)))
    return {'version': version, 'notes': 'n', 'pub_date': '2026-11-19T00:00:00Z',
            'platforms': {t: {'url': download_url(tag, f'{t}.bin'), 'signature': sig} for t in REQUIRED_TARGETS}}


class Ed25519(unittest.TestCase):
    def test_the_rfc_8032_vectors(self):
        for secret, public, message, signature in RFC:
            secret, public = bytes.fromhex(secret), bytes.fromhex(public)
            message, signature = bytes.fromhex(message), bytes.fromhex(signature)
            self.assertEqual(feed_check.ed25519_public(secret), public)
            self.assertEqual(feed_check.ed25519_sign(secret, message), signature)
            self.assertTrue(feed_check.ed25519_verify(public, message, signature))
            self.assertFalse(feed_check.ed25519_verify(public, message + b'x', signature))
            bent = bytearray(signature)
            bent[5] ^= 1
            self.assertFalse(feed_check.ed25519_verify(public, message, bytes(bent)))

    def test_a_scalar_out_of_range_is_refused(self):
        secret, public, message, signature = (bytes.fromhex(v) for v in RFC[0])
        s = int.from_bytes(signature[32:], 'little') + feed_check.L
        self.assertFalse(feed_check.ed25519_verify(public, message, signature[:32] + s.to_bytes(32, 'little')))


class Minisign(unittest.TestCase):
    def test_both_formats_verify_and_return_the_trusted_comment(self):
        for prehashed in (True, False):
            text = signature_file(PACKAGE, comment(), prehashed=prehashed)
            self.assertEqual(feed_check.minisign_verify(public_file(), text, PACKAGE), comment())

    def test_a_changed_file_comment_or_key_is_refused(self):
        text = signature_file(PACKAGE, comment())
        with self.assertRaisesRegex(ValueError, 'does not match'):
            feed_check.minisign_verify(public_file(), text, PACKAGE + b'!')
        forged = text.replace(comment(), comment('9.9.9'))
        with self.assertRaisesRegex(ValueError, 'trusted comment'):
            feed_check.minisign_verify(public_file(), forged, PACKAGE)
        with self.assertRaisesRegex(ValueError, 'another key'):
            feed_check.minisign_verify(public_file(key_id=b'\0' * 8), text, PACKAGE)
        other = signature_file(PACKAGE, comment(), seed=bytes(32))
        with self.assertRaisesRegex(ValueError, 'does not match'):
            feed_check.minisign_verify(public_file(), other, PACKAGE)

    def test_the_signed_version_is_read_from_the_comment(self):
        self.assertEqual(feed_check.signed_version(comment('2.0.0')), '2.0.0')
        self.assertIsNone(feed_check.signed_version('timestamp:1\tfile:x'))

    def test_the_release_public_key_parses(self):
        text = base64.b64decode(feed_check.public_key()).decode()
        key_id, key = feed_check.minisign_public_key(text)
        self.assertEqual((len(key_id), len(key)), (8, 32))


class Feed(unittest.TestCase):
    def run_check(self, documents, package=PACKAGE):
        """feed_check.run against a feed that answers `documents` in turn (an exception is raised)."""
        answers, said, slept = list(documents), [], []

        def get(url):
            if url.startswith(feed_check.FEED):
                answer = answers.pop(0) if len(answers) > 1 else answers[0]
                if isinstance(answer, Exception):
                    raise answer
                return json.dumps(answer).encode()
            return package

        errors = feed_check.run(TAG, b64(public_file()), get=get, attempts=3, wait=7, sleep=slept.append, say=said.append)
        return errors, said, slept

    def test_a_feed_that_serves_the_tag_with_a_verified_package_passes(self):
        errors, said, slept = self.run_check([feed()])
        self.assertEqual(errors, [])
        self.assertEqual(slept, [])
        self.assertIn('verifies', said[-1])

    def test_the_previous_release_is_waited_out_then_refused(self):
        errors, said, slept = self.run_check([feed('1.2.2', 'v1.2.2'), feed()])
        self.assertEqual(errors, [])
        self.assertEqual(slept, [7])
        errors, _, slept = self.run_check([feed('1.2.2', 'v1.2.2')])
        self.assertTrue(errors and '1.2.2' in errors[0])
        self.assertEqual(slept, [7, 7])

    def test_an_unreadable_feed_is_retried(self):
        errors, _, slept = self.run_check([OSError('503'), feed()])
        self.assertEqual((errors, slept), ([], [7]))

    def test_an_entry_outside_the_release_or_missing_is_refused(self):
        document = feed()
        document['platforms']['linux-aarch64']['url'] = download_url('v1.2.2', 'old.AppImage')
        del document['platforms']['darwin-universal']
        errors = feed_check.feed_errors(document, TAG)
        self.assertTrue(any('linux-aarch64' in e for e in errors))
        self.assertTrue(any('darwin-universal' in e for e in errors))

    def test_a_malformed_feed_of_this_release_is_not_waited_out(self):
        document = feed()
        del document['platforms']['windows-aarch64']
        errors, _, slept = self.run_check([document])
        self.assertTrue(errors and 'windows-aarch64' in errors[0])
        self.assertEqual(slept, [])

    def test_a_package_that_does_not_verify_fails_the_check(self):
        errors, _, _ = self.run_check([feed()], package=PACKAGE + b'tampered')
        self.assertTrue(errors and 'does not match' in errors[0])

    def test_a_package_signed_for_another_version_fails_the_check(self):
        errors, _, _ = self.run_check([feed(signature=signature_file(PACKAGE, comment('1.2.2')))])
        self.assertTrue(errors and "'1.2.2'" in errors[0])


if __name__ == '__main__':
    unittest.main()
