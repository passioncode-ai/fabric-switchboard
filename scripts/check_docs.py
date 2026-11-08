#!/usr/bin/env python3
"""Read-only relative Markdown target and scenario-coverage checker."""
from pathlib import Path
import re
import sys
from urllib.parse import unquote

# A tree to check other than this repository (SB-307: the planted-defect test).
root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
files = [root / 'README.md', *sorted((root / 'docs').rglob('*.md'))]
errors = []
links = 0
for file in files:
    for match in re.finditer(r'\[[^\]\n]*\]\(([^)\n]+)\)', file.read_text(encoding='utf-8')):
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
scenarios = (root / 'docs/ux/scenarios.md').read_text(encoding='utf-8')
for section in re.split(r'^### SCN-', scenarios, flags=re.M)[1:]:
    if '**Coverage:** none yet' in section:
        errors.append(f'SCN-{section[:3]}: coverage must name evidence or explicit NOT_RUN')
# SB-306: the board keeps its table shape and a status that starts with its state, because
# readers of the shared backlog take a status's first word (docs/DOCMAP.md, Registers).
BOARD_WORDS = ('open', 'in', 'done', 'closed', 'released', 'fixed', 'superseded', 'decided', 'filed', 'transferred', 'handed')
board = root / 'docs/evidence/backlog.md'
if board.exists():
    for number, line in enumerate(board.read_text(encoding='utf-8').splitlines(), 1):
        if not line.startswith('| SB-'):
            continue
        cells = re.split(r'(?<!\\)\|', line)
        where = f'docs/evidence/backlog.md:{number}'
        if len(cells) != 7:
            errors.append(f'{where}: {len(cells) - 2} cells, not 5 (write a | inside a cell as \\|)')
            continue
        status = cells[5].strip()
        word = re.match(r'[a-z]*', re.sub(r'^[*_\s]+', '', status).lower())[0]
        if word not in BOARD_WORDS:
            errors.append(f'{where}: status starts with "{word}", not one of {", ".join(BOARD_WORDS)}')
        last = status.split(' · ')[-1].strip()
        if re.match(r'^\*{0,2}(closed|done)\*{0,2}\s*20\d\d-\d\d-\d\d', last) and word not in ('closed', 'done'):
            errors.append(f'{where}: its last note closes it, so its status starts with closed or done')
for error in errors:
    print(error, file=sys.stderr)
print(f'{len(files)} Markdown files, {links} relative links, {len(errors)} errors')
sys.exit(bool(errors))
