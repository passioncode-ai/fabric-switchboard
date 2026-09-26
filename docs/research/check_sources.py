#!/usr/bin/env python3
"""Check report citations against pinned Git objects without executing reviewed code.

Usage: python3 docs/research/check_sources.py /path/to/account-switch-research
Only reads sources.json, README.md and Git objects in the four named clones.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('clones', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    pins = json.loads((root / 'sources.json').read_text())
    report = (root / 'README.md').read_text()
    by_remote = {p['remote'].removesuffix('.git'): p for p in pins}
    for pin in pins:
        clone = args.clones / pin['name']
        head = subprocess.check_output(
            ['git', '-C', str(clone), 'rev-parse', 'HEAD'], text=True
        ).strip()
        if head != pin['commit']:
            raise SystemExit(f"HEAD mismatch: {pin['name']}")
    definitions = dict(re.findall(r'^\[([^\]]+)\]: (\S+)$', report, re.MULTILINE))
    uses = set(re.findall(r'\[[^\]\n]+\]\[([^\]]+)\]', report))
    if missing := uses - definitions.keys():
        raise SystemExit(f'Undefined references: {sorted(missing)}')
    checked = 0
    for label, url in definitions.items():
        match = re.fullmatch(
            r'(https://github.com/[^/]+/[^/]+)/blob/([0-9a-f]{40})/(.+)#L(\d+)-L(\d+)',
            url,
        )
        if not match:
            raise SystemExit(f'Unpinned source reference: {label}')
        remote, commit, path, first, last = match.groups()
        pin = by_remote.get(remote)
        if pin is None or commit != pin['commit']:
            raise SystemExit(f'Source mismatch: {label}')
        content = subprocess.check_output([
            'git', '-C', str(args.clones / pin['name']),
            'show', f'{commit}:{path}',
        ], text=True)
        if not 1 <= int(first) <= int(last) <= len(content.splitlines()):
            raise SystemExit(f'Invalid line range: {label}')
        checked += 1
    print(f'PASS: {len(pins)} pinned HEADs, {checked} source ranges, {len(uses)} referenced labels.')
    print('Bounds/object check only; no remote HTTP check, provider calls or third-party code execution.')


if __name__ == '__main__':
    main()
