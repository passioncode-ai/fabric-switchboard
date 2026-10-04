#!/usr/bin/env python3
"""Real-client MCP compatibility matrix for `switchboard mcp` (SB-43).

Runs the official clients, at pinned versions, against the CLI on an empty scratch data folder:
the MCP Inspector CLI, the TypeScript SDK v1 and v2 clients, and the Python SDK. A wrapper
records both directions of the stdio stream, so the matrix shows the protocol version each
client asked for and the one the server answered, beside the tools listed and one proving call.

The proving call is `switchboard_usage` without `refresh`: it reads only the scratch store.
`switchboard_status` and `switchboard_accounts` are listed but never called, because they read
the ordinary Claude Code and Codex sign-ins of this machine (AGENTS.md: no test reads global
credentials). Nothing here needs a provider account, a model call or a change to any agent's
configuration. Network is needed once to fetch the pinned clients (npm, PyPI).

    python3 scripts/mcp_compat.py target/debug/switchboard [--json out.json]
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

CLIENTS = {
    'inspector-cli': '@modelcontextprotocol/inspector@2.9.0',
    'typescript-sdk-v1': '@modelcontextprotocol/sdk@1.32.0',
    'typescript-client-v2': '@modelcontextprotocol/client@2.3.0',
    'python-sdk': 'mcp==2.3.0',
}
PROVING_CALL = 'switchboard_usage'
READ_ONLY_TOOLS = ['switchboard_status', 'switchboard_accounts', 'switchboard_usage', 'switchboard_project_context']

TS_V1 = """import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
"""
TS_V2 = """import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
"""
TS_BODY = """const [command, ...args] = process.argv.slice(2);
const client = new Client({ name: 'switchboard-compat', version: '1.0.0' });
await client.connect(new StdioClientTransport({ command, args }));
const tools = await client.listTools();
const call = await client.callTool({ name: '%s', arguments: {} });
await client.close();
console.log(JSON.stringify({ tools: tools.tools.map(t => t.name), isError: Boolean(call.isError) }));
""" % PROVING_CALL
PYTHON = """import asyncio, json, sys
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
async def main(command, *args):
    async with stdio_client(StdioServerParameters(command=command, args=list(args))) as (r, w):
        async with ClientSession(r, w) as session:
            await session.initialize()
            tools = await session.list_tools()
            call = await session.call_tool('%s', {})
            print(json.dumps({'tools': [t.name for t in tools.tools], 'isError': bool(call.is_error)}))
asyncio.run(main(*sys.argv[1:]))
""" % PROVING_CALL


def wrapper(folder, binary, data):
    """A server command that records stdin and stdout per client name (its first argument)."""
    path = folder / 'wrap.sh'
    path.write_text(
        '#!/bin/sh\n'
        f'tee -a "{folder}/$1.in" | "{binary}" mcp --data-dir "{data}" --read-only | tee -a "{folder}/$1.out"\n'
    )
    path.chmod(0o700)
    return path


def negotiated(folder, name):
    asked, answered = [], []
    for line in (folder / f'{name}.in').read_text().splitlines():
        message = json.loads(line)
        if message.get('method') == 'initialize':
            asked.append(message['params']['protocolVersion'])
    for line in (folder / f'{name}.out').read_text().splitlines():
        message = json.loads(line)
        result = message.get('result')
        if isinstance(result, dict) and 'protocolVersion' in result:
            answered.append(result['protocolVersion'])
        if 'error' in message:
            answered.append('error')
    return asked, answered


def run(command, cwd, timeout=300):
    done = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if done.returncode != 0:
        raise RuntimeError(f'{command[0]} exited {done.returncode}: {done.stderr.strip()[-400:]}')
    return done.stdout


def inspector(folder, wrap):
    package = CLIENTS['inspector-cli']
    base = ['npx', '-y', package, '--cli', str(wrap), 'inspector-cli']
    listed = json.loads(run(base + ['--method', 'tools/list'], folder))
    called = json.loads(run(base + ['--method', 'tools/call', '--tool-name', PROVING_CALL], folder))
    return {'tools': [t['name'] for t in listed['tools']], 'isError': bool(called.get('isError'))}


def typescript(folder, wrap, name, header):
    project = folder / name
    project.mkdir()
    run(['npm', 'init', '-y'], project)
    run(['npm', 'install', '--silent', '--no-audit', '--no-fund', CLIENTS[name]], project)
    (project / 'run.mjs').write_text(header + TS_BODY)
    return json.loads(run(['node', 'run.mjs', str(wrap), name], project).strip().splitlines()[-1])


def python(folder, wrap):
    (folder / 'run_py.py').write_text(PYTHON)
    out = run(['uv', 'run', '-q', '--no-project', '--with', CLIENTS['python-sdk'], 'python', 'run_py.py', str(wrap), 'python-sdk'], folder)
    return json.loads(out.strip().splitlines()[-1])


def matrix(binary):
    binary = Path(binary).resolve()
    version = run([str(binary), '--version'], None).strip()
    rows = []
    with tempfile.TemporaryDirectory(prefix='switchboard-mcp-compat-') as temporary:
        folder = Path(temporary)
        data = folder / 'data'
        data.mkdir(mode=0o700)
        wrap = wrapper(folder, binary, data)
        steps = {
            'inspector-cli': lambda: inspector(folder, wrap),
            'typescript-sdk-v1': lambda: typescript(folder, wrap, 'typescript-sdk-v1', TS_V1),
            'typescript-client-v2': lambda: typescript(folder, wrap, 'typescript-client-v2', TS_V2),
            'python-sdk': lambda: python(folder, wrap),
        }
        for name, step in steps.items():
            row = {'client': name, 'package': CLIENTS[name]}
            try:
                result = step()
                asked, answered = negotiated(folder, name)
                row.update({
                    'asked': sorted(set(asked)),
                    'answered': sorted(set(answered)),
                    'tools': result['tools'],
                    'proving_call': PROVING_CALL,
                    'proving_call_error': result['isError'],
                })
                row['status'] = 'PASS' if (
                    result['tools'] == READ_ONLY_TOOLS and not result['isError']
                    and answered and 'error' not in answered
                ) else 'FAIL'
            except (RuntimeError, subprocess.TimeoutExpired, KeyError, ValueError) as error:
                row.update({'status': 'FAIL', 'reason': str(error)[-400:]})
            rows.append(row)
        # Every client closed its stdin: no server may outlive it.
        time.sleep(0.5)
        left = subprocess.run(['pgrep', '-f', f'{binary} mcp --data-dir {data}'], capture_output=True, text=True).stdout.split()
    return {
        'server': version,
        'server_versions': ['2025-06-18', '2025-03-26', '2024-11-05'],
        'mode': 'read-only, empty scratch data folder, no provider account, no model call',
        'clients': rows,
        'servers_left_running_after_eof': len(left),
        'not_run': {
            'claude-code': 'needs its user MCP configuration changed or a model call; operator acceptance (SB-15)',
            'codex-cli': 'needs its user MCP configuration changed or a model call; operator acceptance (SB-15)',
        },
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('binary')
    parser.add_argument('--json', type=Path)
    args = parser.parse_args()
    for tool in ('npx', 'npm', 'node', 'uv'):
        if shutil.which(tool) is None:
            print(f'{tool} is required', file=sys.stderr)
            sys.exit(2)
    result = matrix(args.binary)
    text = json.dumps(result, indent=2)
    if args.json:
        args.json.write_text(text + '\n')
    print(text)
    sys.exit(0 if all(r['status'] == 'PASS' for r in result['clients']) and not result['servers_left_running_after_eof'] else 1)
