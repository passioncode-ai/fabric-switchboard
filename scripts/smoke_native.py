#!/usr/bin/env python3
"""Require a built desktop to render and complete real IPC round trips in a memory-vault smoke session."""
import argparse
import json
import os
import tempfile
import subprocess
import sys
from pathlib import Path


def verify(binary, version, timeout=45):
    with tempfile.TemporaryDirectory(prefix='switchboard-native-check-') as temporary:
        env = dict(os.environ, TMPDIR=temporary, TEMP=temporary, TMP=temporary)
        return _verify(binary, version, timeout, env)

def _verify(binary, version, timeout, env):
    process = subprocess.Popen([str(Path(binary).resolve()), '--smoke-test'], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()
        raise RuntimeError('Native frontend did not become ready before the deadline.') from None
    expected = f'SWITCHBOARD_FRONTEND_READY {version}'.encode()
    if process.returncode != 0 or expected not in stdout.splitlines():
        raise RuntimeError('Native frontend readiness failed; no credential or raw process output is printed.')
    return {'status': 'PASS', 'version': version, 'mode': 'temporary store, memory vault, native IPC and bundled frontend', 'real_provider_auth': 'NOT_READ'}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--version', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.binary, args.version)))
    except RuntimeError as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
