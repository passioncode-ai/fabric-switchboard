#!/usr/bin/env python3
"""Render docs/AGENT-SUPPORT.md from catalog/agents.json (the one source for the CLI, the app,
this page and the website). `--check` fails when the page is stale.

    python3 scripts/agents_doc.py [--check]
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / 'catalog/agents.json'
PAGE = ROOT / 'docs/AGENT-SUPPORT.md'
LEVEL = {
    'launch': 'Launch from Switchboard',
    'proxy': 'Through Switchboard (set up once in the agent)',
    'mcp': 'Tools only (`switchboard mcp`)',
}


def yes(value):
    return {True: 'yes', False: 'no'}.get(value, 'unverified')


def render(catalog):
    agents = catalog['agents']
    lines = [
        '# Agents Switchboard works with',
        '',
        f'Generated from [`catalog/agents.json`](../catalog/agents.json) by `scripts/agents_doc.py`; do not edit by hand. '
        f'Facts and sources: [research, {catalog["as_of"]}](research/agents-2026-10-05.md). Ranking: {catalog.get("ranking_source", "—")}.',
        '',
        'Every agent here can use Switchboard at one of three levels:',
        '',
        '- **Tools only** — it registers `switchboard mcp`: it reads which account handles requests and how much quota is left, switches the account of a managed session, and knows the project of its folder.',
        '- **Through Switchboard** — it can also send its model requests to Switchboard\'s local proxy (`http://127.0.0.1:<port>/claude/<pool>` for the Anthropic Messages API, `…/codex/<pool>/v1` for the OpenAI Responses and Chat Completions APIs), which serves the pool\'s selected account and switches accounts for it. It is set up once in the agent\'s own config; `switchboard agents connect <id>` prints the exact lines.',
        '- **Launch from Switchboard** — configured by environment alone, so `switchboard agents launch <id> --pool <pool> --dir <folder>` (or *Agents → Other agents → Set up → Launch*) starts it in Terminal. Agents set up once in their config launch the same way.',
        '',
        '**Accounts.** A subscription sign-in (Claude.ai, a Claude setup token, ChatGPT) is for the provider\'s own client — Claude Code or Codex — by the providers\' terms. Other agents get their own proxy key (`switchboard agents key`, derived from the session capability, never stored in their config), and the proxy serves that key only from **API-key accounts**; a pool whose selected account is a subscription sign-in answers 403. Projects still apply: a project\'s account starts only inside its folders.',
        '',
        '| # | Agent | Kind | Level | MCP | Anthropic endpoint | OpenAI endpoint | Headless |',
        '|---|---|---|---|---|---|---|---|',
    ]
    for i, a in enumerate(agents, 1):
        an, oa = a.get('anthropic') or {}, a.get('openai') or {}
        if a.get('proxy_ok') is False:
            anth, oai = 'no (own service)', 'no'
        elif a.get('proxy_ok') is not True:
            anth, oai = 'unverified with a local endpoint', 'unverified'
        else:
            anth = yes(an.get('supported')) + (f" (`{an['base_env']}`)" if an.get('base_env') else (' (config)' if an.get('supported') else ''))
            kinds = [k for k, v in (('Responses', oa.get('responses')), ('Chat Completions', oa.get('chat_completions'))) if v]
            oai = ', '.join(kinds) or 'no'
        rank = f" · #{a['openrouter_rank']} on OpenRouter" if a.get('openrouter_rank') else ''
        lines.append(f"| {i} | [{a['name']}]({a['url']}){rank} | {a['kind']} | {LEVEL[a['level']]} | {yes(a['mcp']['supported'])} | {anth} | {oai} | {('`' + a['headless'] + '`') if a.get('headless') else '—'} |")
    lines += ['', '## Per agent', '']
    for a in agents:
        lines.append(f"### {a['name']}")
        lines.append('')
        lines.append(f"`{a['id']}` · {LEVEL[a['level']]} · {a.get('maker') or ''} · license {a.get('license') or 'unverified'}")
        lines.append('')
        mcp = a['mcp']
        if mcp.get('add_command'):
            lines.append(f"- Tools: `{mcp['add_command']}`")
        elif mcp.get('config_path'):
            lines.append(f"- Tools: add `switchboard` (`switchboard mcp`) to `{mcp['config_path']}`")
        elif not mcp.get('supported'):
            lines.append('- Tools: no MCP support.')
        lines.append(f"- Setup: `switchboard agents connect {a['id']} --pool <pool>`" + (f"; launch: `switchboard agents launch {a['id']} --pool <pool> --dir <folder>`" if a['level'] != 'mcp' and a.get('binary') else ''))
        if a.get('subscription_warning'):
            lines.append(f"- **Accounts:** {a['subscription_warning']}")
        if a.get('notes'):
            lines.append(f"- Notes: {a['notes']}")
        lines.append('- Sources: ' + ', '.join(f'[{n}]({u})' for n, u in enumerate(a['sources'], 1)))
        lines.append('')
    return '\n'.join(lines).rstrip() + '\n'


if __name__ == '__main__':
    text = render(json.loads(CATALOG.read_text(encoding='utf-8')))
    if '--check' in sys.argv:
        if not PAGE.exists() or PAGE.read_text(encoding='utf-8') != text:
            print('docs/AGENT-SUPPORT.md is stale; run `python3 scripts/agents_doc.py`', file=sys.stderr)
            sys.exit(1)
        print('docs/AGENT-SUPPORT.md is current')
    else:
        PAGE.write_text(text, encoding='utf-8')
        print(f'wrote {PAGE.relative_to(ROOT)} ({len(text)} bytes)')
