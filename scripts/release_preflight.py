#!/usr/bin/env python3
"""Refuse a release run before any build or approval: the tag must name the version being built,
every version declaration must agree, and a published release needs its CHANGELOG section, which
must say `windows_authenticode: NOT_SIGNED` while Windows signing is switched off. With
`--latest` (the tag of the latest published release, empty when there is none), a published vX.Y.Z
must be newer than it (SB-563, M4-09): the publish workflow refuses such a tag too, but only at
the end of the run, after every platform was built and signed.

  python3 scripts/release_preflight.py --tag v0.5.4 --publish true [--windows-signing false] [--latest v0.5.3]
"""
import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TAG = re.compile(r'^v(\d+\.\d+\.\d+)(-(?:rc|beta)\.\d+)?$')
NOT_SIGNED = 'windows_authenticode: NOT_SIGNED'


def declared_versions(root=ROOT):
    cargo = re.search(r'^version = "([^"]+)"', (root / 'Cargo.toml').read_text(encoding='utf-8'), re.MULTILINE)
    return {
        'package.json': json.loads((root / 'package.json').read_text(encoding='utf-8'))['version'],
        'src-tauri/tauri.conf.json': json.loads((root / 'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))['version'],
        'Cargo.toml': cargo.group(1) if cargo else None,
    }


def section(changelog, version):
    """The `## <version>` section, matched the way release-publish.yml's awk matches it."""
    lines, inside = [], False
    head = re.compile(r'^## ' + re.escape(version) + r'([ —-]|$)')
    for line in changelog.splitlines():
        if inside and line.startswith('## '):
            break
        if inside:
            lines.append(line)
        elif head.match(line):
            inside = True
    return '\n'.join(lines).strip()


def core(tag):
    """The X.Y.Z of a tag as a tuple of numbers, or None when it carries none."""
    found = re.search(r'(\d+)\.(\d+)\.(\d+)', tag or '')
    return tuple(int(part) for part in found.groups()) if found else None


def not_newer(tag, latest):
    """Why a published vX.Y.Z may not follow `latest`, or None. Matches release-publish.yml: a
    prerelease (-beta, -alpha, -preview) never becomes latest, so it is not compared, and a tag equal
    to latest (a run started again on a published tag) is left to the publish workflow, which
    refuses to rewrite it."""
    if not latest or '-' in tag or tag == latest:
        return None
    new, current = core(tag), core(latest)
    if new is None or current is None or new > current:
        return None
    return (f'{tag} is not newer than the latest release {latest}; installed copies read their updates '
            f'from the latest release, so cut a newer tag.')


def check(tag, versions, changelog, publish, windows_signing=None, latest=None):
    """Every reason this run must not start; an empty list means go."""
    found = TAG.match(tag)
    if not found:
        return [f'{tag!r} is not a release tag (vX.Y.Z, vX.Y.Z-rc.N or vX.Y.Z-beta.N); run the workflow on a tag.']
    errors = [f'{name} declares {value}, but the tag is {tag}.'
              for name, value in versions.items() if value != found.group(1)]
    if publish:
        version = tag[1:]
        notes = section(changelog, version)
        if not notes:
            errors.append(f'CHANGELOG.md has no "## {version}" section; the release notes come from it.')
        elif windows_signing is False and NOT_SIGNED not in notes:
            errors.append(f'Windows signing is off, so the "## {version}" notes must say "{NOT_SIGNED}".')
        older = not_newer(tag, latest)
        if older:
            errors.append(older)
    return errors


def flag(value):
    return value.strip().lower() == 'true'


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--publish', required=True, type=flag)
    parser.add_argument('--windows-signing', type=flag, help='vars.AZURE_SIGNING_ENABLED; omitted where the variable is not visible')
    parser.add_argument('--latest', help='the latest published release tag (empty: none); omitted: not compared')
    args = parser.parse_args()
    changelog = ROOT / 'CHANGELOG.md'
    errors = check(args.tag, declared_versions(), changelog.read_text(encoding='utf-8') if changelog.exists() else '',
                   args.publish, args.windows_signing, args.latest)
    for error in errors:
        print(f'::error::{error}')
    if errors:
        sys.exit(1)
    print(f'preflight: {args.tag} publish={args.publish} windows_signing={args.windows_signing} '
          f'latest={args.latest if args.latest is not None else "not compared"}: ok')


if __name__ == '__main__':
    main()
