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
def table_cells(line):
    """A table row's cells as the shared backlog reader splits them (fabric-workspace
    lib/backlog.mjs `cells`): `\\|` is a literal bar, a bar inside a code span is not a
    separator, and an empty first and last cell (the row's edge bars) are dropped."""
    result, cell, ticks, i = [], '', 0, 0
    while i < len(line):
        c = line[i]
        if c == '\\' and line[i + 1:i + 2] == '|':
            cell += '|'
            i += 2
            continue
        if c == '`':
            n = 1
            while line[i + n:i + n + 1] == '`':
                n += 1
            if not ticks:
                ticks = n
            elif ticks == n:
                ticks = 0
            cell += '`' * n
            i += n
            continue
        if c == '|' and not ticks:
            result.append(cell.strip())
            cell = ''
        else:
            cell += c
        i += 1
    result.append(cell.strip())
    if result and result[0] == '':
        result.pop(0)
    if result and result[-1] == '':
        result.pop()
    return result
BOARD_WORDS = ('open', 'in', 'done', 'closed', 'released', 'fixed', 'superseded', 'decided', 'filed', 'transferred', 'handed')
board = root / 'docs/evidence/backlog.md'
if board.exists():
    for number, line in enumerate(board.read_text(encoding='utf-8').splitlines(), 1):
        if not line.startswith('| SB-'):
            continue
        cells = table_cells(line)
        # GitHub splits on every unescaped bar, inside code spans too; the shared reader keeps a
        # code span's bars. A row has five cells for both, and its closing bar.
        github = [c.strip() for c in re.split(r'(?<!\\)\|', line.strip())]
        github = github[1:] if github and github[0] == '' else github
        github = github[:-1] if github and github[-1] == '' else github
        where = f'docs/evidence/backlog.md:{number}'
        if len(github) != 5 or len(cells) != 5:
            count = len(github) if len(github) != 5 else len(cells)
            errors.append(f'{where}: {count} cells, not 5 (write a | inside a cell as \\|)')
            continue
        if not line.rstrip().endswith('|'):
            errors.append(f'{where}: no closing | (the shared reader reads text after the last bar as a cell)')
            continue
        status = cells[4]
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
