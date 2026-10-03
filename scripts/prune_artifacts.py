#!/usr/bin/env python3
"""Lifecycle LC-15: keep the current and the previous release of each kind in artifacts/.

Run by build_macos.py and build_windows_cross.py after a successful build, and by hand with
`python3 scripts/prune_artifacts.py`. A release is `Fabric-Switchboard-<version>-<platform>-<arch>`
as a folder and/or a `.zip`; per platform-arch the two highest versions stay. Receipts (small
JSON), notes and logs are never removed. App bundles inside a removed folder are unregistered
from LaunchServices first, so no stale copy answers an `open`.
"""
from pathlib import Path
import re
import shutil
import subprocess
import sys

LSREGISTER = Path('/System/Library/Frameworks/CoreServices.framework/Frameworks/'
                  'LaunchServices.framework/Support/lsregister')
NAME = re.compile(r'^Fabric-Switchboard-(\d+)\.(\d+)\.(\d+)-([a-z0-9]+-[a-z0-9_]+)(\.zip)?$')


def release_key(path: Path):
    """(platform-arch, version tuple) of a release folder or archive; None for anything else."""
    match = NAME.match(path.name)
    if not match:
        return None
    major, minor, patch, kind, _ = match.groups()
    return kind, (int(major), int(minor), int(patch))


def unregister_app(app: Path) -> None:
    if LSREGISTER.exists():
        subprocess.run([str(LSREGISTER), '-u', str(app)], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def prune(artifacts: Path, keep: int = 2, unregister=unregister_app) -> list:
    """Removes all but the `keep` newest versions per kind; returns what was removed."""
    if not artifacts.is_dir():
        return []
    releases = {}
    for path in artifacts.iterdir():
        key = release_key(path)
        if key and not path.is_symlink():
            releases.setdefault(key[0], {}).setdefault(key[1], []).append(path)
    removed = []
    for versions in releases.values():
        for version in sorted(versions, reverse=True)[keep:]:
            for path in versions[version]:
                if path.is_dir():
                    for app in sorted(path.rglob('*.app')):
                        unregister(app)
                    shutil.rmtree(path)
                else:
                    path.unlink()
                removed.append(path)
    return removed


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    for path in prune(root / 'artifacts'):
        print(f'pruned {path.name}')


if __name__ == '__main__':
    sys.exit(main())
