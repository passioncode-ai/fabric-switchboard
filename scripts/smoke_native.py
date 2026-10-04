#!/usr/bin/env python3
"""Require a built desktop to render and complete real IPC round trips in a memory-vault smoke session."""
import argparse
import json
import os
import tempfile
import subprocess
import sys
from pathlib import Path


def verify(binary, version, timeout=45, background=False):
    """With `background`, the app is started as the lifecycle broker starts it (`--background`,
    plus an argument it does not know): the UI must still load, and the window must stay hidden."""
    with tempfile.TemporaryDirectory(prefix='switchboard-native-check-') as temporary:
        env = dict(os.environ, TMPDIR=temporary, TEMP=temporary, TMP=temporary)
        return _verify(binary, version, timeout, env, background)

def _verify(binary, version, timeout, env, background=False):
    arguments = [str(Path(binary).resolve()), '--smoke-test']
    if background:
        arguments += ['--background', '--an-argument-the-app-does-not-know']
    process = subprocess.Popen(arguments, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()
        raise RuntimeError('Native frontend did not become ready before the deadline.') from None
    expected = f'SWITCHBOARD_FRONTEND_READY {version}'.encode()
    lines = stdout.splitlines()
    if process.returncode != 0 or expected not in lines:
        raise RuntimeError('Native frontend readiness failed; no credential or raw process output is printed.')
    window = b'SWITCHBOARD_WINDOW hidden' if background else b'SWITCHBOARD_WINDOW visible'
    if window not in lines:
        raise RuntimeError(f'Native window state is not {window.decode().split()[-1]} for this launch.')
    return {'status': 'PASS', 'version': version, 'mode': 'temporary store, memory vault, native IPC and bundled frontend', 'launch': 'background' if background else 'ordinary', 'window': window.decode().split()[-1], 'real_provider_auth': 'NOT_READ'}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--background', action='store_true', help='start as the lifecycle broker does; require a hidden window')
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.binary, args.version, background=args.background)))
    except RuntimeError as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
