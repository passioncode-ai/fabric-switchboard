#!/usr/bin/env python3
"""Prove, after a release is published, that the update feed serves it (SB-563, M4-08).

  python3 scripts/feed_check.py --tag vX.Y.Z [--attempts 20] [--wait 30]

Every installed copy reads `releases/latest/download/latest.json`. This reads the same URL until it
announces the tag's version (GitHub may serve the previous release for a short while after a
publish), then requires every platform entry to name a file of this release, downloads one package
(`windows-x86_64`, the smallest) from the URL the feed gives, and verifies it the way the app's
updater does: the minisign signature from the feed against the public key in
`src-tauri/tauri.conf.json`, then the version that signature names (`requireSignedVersion`). A
release whose feed points elsewhere, or whose package does not verify, fails the run that
published it instead of failing every installed copy in silence.

The signature is checked here, in the standard library only: Ed25519 as RFC 8032 section 5.1.7
defines it (verification only needs public data) and minisign's two formats, `Ed` (the file signed
as it is) and `ED` (its BLAKE2b-512 digest signed). `scripts/test_feed_check.py` holds the RFC's
test vectors.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from updater_artifacts import REPOSITORY, REQUIRED_TARGETS, TAG, download_url  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
FEED = f'https://github.com/{REPOSITORY}/releases/latest/download/latest.json'
VERIFIED_TARGET = 'windows-x86_64'
USER_AGENT = 'fabric-switchboard-release-feed-check'

# Ed25519 (RFC 8032 section 5.1): the field, the curve constant, the group order and the base point.
P = 2 ** 255 - 19
L = 2 ** 252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def _recover_x(y, sign):
    if y >= P:
        return None
    x2 = (y * y - 1) * pow(D * y * y + 1, P - 2, P)
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P:
        x = x * SQRT_M1 % P
    if (x * x - x2) % P:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


_BY = 4 * pow(5, P - 2, P) % P
_BX = _recover_x(_BY, 0)
BASE = (_BX, _BY, 1, _BX * _BY % P)


def _add(a, b):
    a_minus = (a[1] - a[0]) * (b[1] - b[0]) % P
    a_plus = (a[1] + a[0]) * (b[1] + b[0]) % P
    c = 2 * a[3] * b[3] * D % P
    d = 2 * a[2] * b[2] % P
    e, f, g, h = a_plus - a_minus, d - c, d + c, a_plus + a_minus
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def _mul(scalar, point):
    result = (0, 1, 1, 0)
    while scalar:
        if scalar & 1:
            result = _add(result, point)
        point = _add(point, point)
        scalar >>= 1
    return result


def _equal(a, b):
    return (a[0] * b[2] - b[0] * a[2]) % P == 0 and (a[1] * b[2] - b[1] * a[2]) % P == 0


def _compress(point):
    zinv = pow(point[2], P - 2, P)
    x, y = point[0] * zinv % P, point[1] * zinv % P
    return int.to_bytes(y | ((x & 1) << 255), 32, 'little')


def _decompress(data):
    if len(data) != 32:
        return None
    y = int.from_bytes(data, 'little')
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % P)


def _hash_int(data):
    return int.from_bytes(hashlib.sha512(data).digest(), 'little')


def ed25519_verify(public, message, signature):
    """RFC 8032 section 5.1.7: True only when `signature` is `public`'s signature of `message`."""
    if len(public) != 32 or len(signature) != 64:
        return False
    a, r = _decompress(public), _decompress(signature[:32])
    s = int.from_bytes(signature[32:], 'little')
    if a is None or r is None or s >= L:
        return False
    h = _hash_int(signature[:32] + public + message) % L
    return _equal(_mul(s, BASE), _add(r, _mul(h, a)))


def ed25519_public(secret):
    """The public key of a 32-byte seed (RFC 8032 section 5.1.5). Used by the tests to sign."""
    digest = hashlib.sha512(secret).digest()
    a = int.from_bytes(digest[:32], 'little') & ((1 << 254) - 8) | (1 << 254)
    return _compress(_mul(a, BASE))


def ed25519_sign(secret, message):
    """RFC 8032 section 5.1.6. Used only by the tests: the release key never comes here."""
    digest = hashlib.sha512(secret).digest()
    a = int.from_bytes(digest[:32], 'little') & ((1 << 254) - 8) | (1 << 254)
    public = _compress(_mul(a, BASE))
    r = _hash_int(digest[32:] + message) % L
    big_r = _compress(_mul(r, BASE))
    s = (r + _hash_int(big_r + public + message) * a) % L
    return big_r + int.to_bytes(s, 32, 'little')


def _lines(text):
    return [line for line in text.replace('\r\n', '\n').split('\n') if line.strip()]


def minisign_public_key(text):
    """(key id, 32-byte key) of a minisign public key file."""
    lines = _lines(text)
    raw = base64.b64decode(lines[-1])
    if len(raw) != 42 or raw[:2] != b'Ed':
        raise ValueError('not a minisign Ed25519 public key')
    return raw[2:10], raw[10:]


def minisign_verify(public_text, signature_text, data):
    """The trusted comment of a valid minisign signature of `data`; ValueError otherwise."""
    key_id, key = minisign_public_key(public_text)
    lines = _lines(signature_text)
    if len(lines) != 4 or not lines[2].startswith('trusted comment: '):
        raise ValueError('not a minisign signature file')
    raw = base64.b64decode(lines[1])
    if len(raw) != 74:
        raise ValueError('the signature line has the wrong length')
    algorithm, signed_by, signature = raw[:2], raw[2:10], raw[10:]
    if signed_by != key_id:
        raise ValueError('signed by another key')
    if algorithm == b'ED':
        message = hashlib.blake2b(data, digest_size=64).digest()
    elif algorithm == b'Ed':
        message = data
    else:
        raise ValueError(f'unknown signature algorithm {algorithm!r}')
    if not ed25519_verify(key, message, signature):
        raise ValueError('the signature does not match the file')
    comment = lines[2][len('trusted comment: '):]
    if not ed25519_verify(key, signature + comment.encode('utf-8'), base64.b64decode(lines[3])):
        raise ValueError('the trusted comment is not covered by the signature')
    return comment


def signed_version(comment):
    """The `version:` field of a Tauri trusted comment (tab-separated key:value pairs), or None."""
    for field in comment.split('\t'):
        if field.startswith('version:'):
            return field[len('version:'):]
    return None


def feed_errors(document, tag):
    """Every reason the feed does not announce this release; empty when it does."""
    found = TAG.match(tag)
    if not found:
        return [f'{tag!r} is not a release tag.']
    if document.get('version') != found.group(1):
        return [f'latest.json announces {document.get("version")!r}, not {found.group(1)}.']
    errors = []
    platforms = document.get('platforms') or {}
    prefix = download_url(tag, '')
    for target in REQUIRED_TARGETS:
        entry = platforms.get(target)
        if not entry:
            errors.append(f'latest.json has no {target} entry.')
        elif not str(entry.get('url', '')).startswith(prefix):
            errors.append(f'{target}: {entry.get("url")} is not a file of release {tag}.')
        elif not entry.get('signature'):
            errors.append(f'{target}: no signature.')
    return errors


def verify_package(entry, data, public_key_b64, version):
    """Verify one feed entry against the package bytes; ValueError with the reason otherwise."""
    public_text = base64.b64decode(public_key_b64).decode('utf-8')
    signature_text = base64.b64decode(entry['signature']).decode('utf-8')
    comment = minisign_verify(public_text, signature_text, data)
    signed = signed_version(comment)
    if signed != version:
        raise ValueError(f'the package is signed for version {signed!r}, the feed announces {version}')
    return comment


def fetch(url, timeout=120):
    request = urllib.request.Request(url, headers={'User-Agent': USER_AGENT, 'Cache-Control': 'no-cache'})
    with urllib.request.urlopen(request, timeout=timeout) as answer:
        return answer.read()


def run(tag, public_key_b64, *, get=fetch, attempts=20, wait=30.0, sleep=time.sleep, say=print):
    """Every reason the published release is not what installed copies are offered; empty: it is."""
    found = TAG.match(tag)
    if not found:
        return [f'{tag!r} is not a release tag.']
    version = found.group(1)
    errors = ['latest.json was never read.']
    document = None
    for attempt in range(1, attempts + 1):
        try:
            document = json.loads(get(FEED + f'?attempt={attempt}'))
            errors = feed_errors(document, tag)
        except (OSError, ValueError, urllib.error.URLError) as exc:
            errors = [f'latest.json could not be read: {exc}']
        if not errors:
            break
        say(f'attempt {attempt}/{attempts}: {errors[0]}')
        if document is not None and document.get('version') == version:
            break  # this release's own feed, malformed: waiting does not change it
        if attempt < attempts:
            sleep(wait)
    if errors:
        return errors
    entry = document['platforms'][VERIFIED_TARGET]
    try:
        package = get(entry['url'])
    except (OSError, urllib.error.URLError) as exc:
        return [f'{VERIFIED_TARGET}: {entry["url"]} could not be downloaded: {exc}']
    try:
        comment = verify_package(entry, package, public_key_b64, version)
    except (ValueError, UnicodeDecodeError) as exc:
        return [f'{VERIFIED_TARGET}: {exc}']
    say(f'latest.json announces {version}; {VERIFIED_TARGET} ({len(package)} bytes) verifies: {comment}')
    return []


def public_key(root=ROOT):
    config = json.loads((root / 'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    return config['plugins']['updater']['pubkey']


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--attempts', type=int, default=20)
    parser.add_argument('--wait', type=float, default=30.0)
    args = parser.parse_args(argv)
    errors = run(args.tag, public_key(), attempts=args.attempts, wait=args.wait)
    for error in errors:
        print(f'::error title=feed check::{error}')
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main())
