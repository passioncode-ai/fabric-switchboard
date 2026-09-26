#!/usr/bin/env python3
"""Read-only relative Markdown target and scenario-coverage checker."""
from pathlib import Path
import re
import sys
from urllib.parse import unquote

root = Path(__file__).resolve().parents[1]
files = [root / 'README.md', *sorted((root / 'docs').rglob('*.md'))]
errors = []
links = 0
for file in files:
    for match in re.finditer(r'\[[^\]\n]*\]\(([^)\n]+)\)', file.read_text()):
        target = match[1].split(' "', 1)[0].strip('<>')
        if re.match(r'^[a-zA-Z][a-zA-Z0-9+.-]*:', target) or target.startswith('#'):
            continue
        target = unquote(target.split('#')[0])
        if not target:
            continue
        links += 1
        resolved = (file.parent / target).resolve()
        if not resolved.is_relative_to(root) or not resolved.exists():
            errors.append(f'{file.relative_to(root)}: unresolved/local-only target {target}')
scenarios = (root / 'docs/ux/scenarios.md').read_text()
for section in re.split(r'^## SCN-', scenarios, flags=re.M)[1:]:
    if '**Coverage:** none yet' in section:
        errors.append(f'SCN-{section[:3]}: coverage must name evidence or explicit NOT_RUN')
for error in errors:
    print(error, file=sys.stderr)
print(f'{len(files)} Markdown files, {links} relative links, {len(errors)} errors')
sys.exit(bool(errors))
