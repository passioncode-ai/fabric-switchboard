"""Planted-defect tests for docs/ux/lint.py (SB-309).

The linter reads this project's own UX layers; each test copies them into a temporary
project, plants one defect and asserts the linter fails on it. The clean copy must pass,
so a linter that sees no entries (as it did before the headings became `### ID: name`)
is caught here. Standard library only.

Run: python3 -m unittest scripts/test_ux_lint.py
"""
from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class UxLint(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        # Coverage lines cite source files, so the copy holds the tree without build output.
        shutil.copytree(ROOT, self.root, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git", "node_modules", "target", "dist", "artifacts", "gen"))

    def tearDown(self):
        self._tmp.cleanup()

    def lint(self):
        return subprocess.run([sys.executable, str(ROOT / "docs/ux/lint.py"), str(self.root / "docs/ux")], capture_output=True, text=True)

    def plant(self, text):
        path = self.root / "docs/ux/scenarios.md"
        path.write_text(path.read_text(encoding="utf-8") + text, encoding="utf-8")

    def test_the_project_copy_passes(self):
        result = self.lint()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_duplicate_scenario_fails(self):
        self.plant("\n### SCN-001: First run again\n**Expected result:** then it shows.\n")
        result = self.lint()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[U001]", result.stdout + result.stderr)

    def test_coverage_naming_a_missing_file_fails(self):
        self.plant("\n### SCN-999: Planted\n**Expected result:** then it shows.\n**Coverage:** [gone](../evidence/no-such-file.md)\n")
        result = self.lint()
        self.assertEqual(result.returncode, 1)
        self.assertIn("docs/evidence/no-such-file.md", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
