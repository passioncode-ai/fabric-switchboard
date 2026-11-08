"""Unit tests for scripts/check_docs.py (SB-307).

Each test writes a small tree (a README, a scenario, a board row), plants one defect and
asserts the checker fails and names it; the clean tree must pass, so a checker that cannot
fail is caught here. Standard library only.

Run: python3 -m unittest scripts/test_check_docs.py
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

CHECKER = Path(__file__).resolve().parent / "check_docs.py"
ROW = "| SB-1 | P3 | An item | a source | {status} |"


class CheckDocs(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.write("README.md", "# Readme\n\nSee [the board](docs/evidence/backlog.md).\n")
        self.write("docs/ux/scenarios.md", "# Scenarios\n\n### SCN-001: First\n**Coverage:** a test.\n")
        self.board("open")

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, rel, text):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def board(self, status, row=None):
        line = row if row is not None else ROW.format(status=status)
        self.write("docs/evidence/backlog.md", f"# Board\n\n| ID | P | Item | Source | Status |\n|---|---|---|---|---|\n{line}\n")

    def run_checker(self):
        return subprocess.run([sys.executable, str(CHECKER), str(self.root)], capture_output=True, text=True)

    def assertFails(self, fragment):
        result = self.run_checker()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(fragment, result.stderr)

    def test_the_clean_tree_passes(self):
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("0 errors", result.stdout)

    def test_a_broken_link_fails(self):
        self.write("docs/note.md", "[gone](missing.md)\n")
        self.assertFails("unresolved/local-only target missing.md")

    def test_a_scenario_without_coverage_fails(self):
        self.write("docs/ux/scenarios.md", "# Scenarios\n\n### SCN-002: Second\n**Coverage:** none yet\n")
        self.assertFails("SCN-002: coverage must name evidence")

    def test_a_pipe_inside_a_cell_fails_unless_escaped(self):
        self.board("", row="| SB-1 | P3 | `a|b` | a source | open |")
        self.assertFails("6 cells, not 5")
        self.board("", row="| SB-1 | P3 | `a\\|b` | a source | open |")
        self.assertEqual(self.run_checker().returncode, 0)

    def test_a_status_must_start_with_a_declared_word(self):
        self.board("maybe later")
        self.assertFails('status starts with "maybe"')
        self.board("**done** 2026-11-08: shipped")
        self.assertEqual(self.run_checker().returncode, 0, "emphasis is not part of the word")

    def test_a_row_closed_in_its_last_note_starts_closed(self):
        self.board("open · built · **closed 2026-11-08**: released")
        self.assertFails("its last note closes it")
        self.board("closed 2026-11-08 (receipt at the end) · open · built · **closed 2026-11-08**: released")
        self.assertEqual(self.run_checker().returncode, 0)


if __name__ == "__main__":
    unittest.main()
