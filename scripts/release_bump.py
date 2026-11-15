#!/usr/bin/env python3
"""Moves the release version in every file that carries it and opens the CHANGELOG section.

A release changed nine files by hand (the plugin's marketplace entry and manifest, the skill,
Cargo.toml and the workspace packages in Cargo.lock, package.json and the two lines of
package-lock.json, the app's tauri.conf.json) and put `## <version>` under `## Unreleased`; a
missed one fails `release_preflight.py` late. This does it in one step and refuses when the new
version is not newer, when a file no longer carries the old version where it is expected, or when
Unreleased is empty.

Usage: python3 scripts/release_bump.py 0.7.97 [--root .] [--dry-run]
"""
import argparse
import pathlib
import re
import sys

# (file, pattern with OLD in it, how many replacements it must make)
PLACES = [
    ('.claude-plugin/marketplace.json', '"version": "{old}"', 1),
    ('plugins/switchboard/.claude-plugin/plugin.json', '"version": "{old}"', 1),
    ('plugins/switchboard/skills/switching-accounts/SKILL.md', 'version: "{old}"', 1),
    ('Cargo.toml', 'version = "{old}"', 1),
    ('package.json', '"version": "{old}"', 1),
    ('package-lock.json', '"version": "{old}"', 2),
    ('src-tauri/tauri.conf.json', '"version": "{old}"', 1),
]
# The workspace packages that carry the release version (switchboard-core keeps its own 0.1.0).
LOCK_PACKAGES = ['fabric-switchboard', 'switchboard-cli', 'switchboard-proxy', 'switchboard-runtime']
VERSION = re.compile(r'^\d+\.\d+\.\d+$')


def parse(version):
    return tuple(int(p) for p in version.split('.'))


def current(root):
    """The version Cargo.toml carries now."""
    found = re.search(r'^version = "(\d+\.\d+\.\d+)"', (root / 'Cargo.toml').read_text(), re.M)
    if not found:
        raise SystemExit('Cargo.toml carries no version.')
    return found.group(1)


def bumped(root, new):
    """Every changed file's new text, by path; raises SystemExit with the reason it refused."""
    if not VERSION.match(new):
        raise SystemExit(f'Give the new version as X.Y.Z, not {new!r}.')
    old = current(root)
    if parse(new) <= parse(old):
        raise SystemExit(f'{new} is not newer than {old}.')
    out = {}
    for path, pattern, count in PLACES:
        text = (root / path).read_text()
        needle = pattern.format(old=old)
        if text.count(needle) < count:
            raise SystemExit(f'{path} does not carry {needle!r} {count} time(s).')
        out[path] = text.replace(needle, pattern.format(old=new), count)
    lock = (root / 'Cargo.lock').read_text()
    for name in LOCK_PACKAGES:
        needle = f'name = "{name}"\nversion = "{old}"'
        if needle not in lock:
            raise SystemExit(f'Cargo.lock does not carry {name} {old}.')
        lock = lock.replace(needle, f'name = "{name}"\nversion = "{new}"', 1)
    out['Cargo.lock'] = lock
    changelog = (root / 'CHANGELOG.md').read_text()
    head = '## Unreleased\n'
    start = changelog.find(head)
    if start < 0:
        raise SystemExit('CHANGELOG.md has no "## Unreleased" section.')
    rest = changelog[start + len(head):]
    following = rest.find('\n## ')
    body = rest if following < 0 else rest[:following]
    if not body.strip():
        raise SystemExit('CHANGELOG.md says nothing under Unreleased: there is nothing to release.')
    out['CHANGELOG.md'] = changelog[:start] + head + f'\n## {new}\n' + rest
    return old, out


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('version')
    parser.add_argument('--root', default='.')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args(argv)
    root = pathlib.Path(args.root)
    old, out = bumped(root, args.version)
    for path, text in out.items():
        if not args.dry_run:
            (root / path).write_text(text)
        print(f'{"would change" if args.dry_run else "changed"} {path}')
    print(f'{old} -> {args.version}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
