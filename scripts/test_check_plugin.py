"""Unit tests for scripts/check_plugin.py.

Each test copies the real plugin, evals and the two Rust sources the checker reads into a
temporary repository, plants one defect, and asserts the checker names it. The clean copy
must pass, so a checker that cannot fail — or one that fails on the shipped tree — is
caught here. Standard library only.

Run: python3 -m unittest scripts/test_check_plugin.py
"""
from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location("check_plugin", ROOT / "scripts/check_plugin.py")
check_plugin = importlib.util.module_from_spec(_spec)
sys.modules["check_plugin"] = check_plugin
_spec.loader.exec_module(check_plugin)

COPIED = (
    ".claude-plugin",
    "plugins",
    "test/evals",
    "package.json",
    check_plugin.MCP_SOURCE,
    check_plugin.APPLY_SOURCE,
)
SKILL = "plugins/switchboard/skills/switching-accounts/SKILL.md"


class Fixture(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        for rel in COPIED:
            source, target = ROOT / rel, self.root / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, target, ignore=shutil.ignore_patterns("__pycache__"))
            else:
                shutil.copy2(source, target)

    def tearDown(self):
        self._tmp.cleanup()

    def edit(self, rel, old, new):
        path = self.root / rel
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text, "fixture drifted: %r not in %s" % (old, rel))
        path.write_text(text.replace(old, new, 1), encoding="utf-8")

    def edit_json(self, rel, change):
        path = self.root / rel
        data = json.loads(path.read_text(encoding="utf-8"))
        change(data)
        path.write_text(json.dumps(data), encoding="utf-8")

    def assertFlags(self, fragment):
        errors = check_plugin.check(self.root)
        self.assertTrue(any(fragment in e for e in errors), "expected %r in %r" % (fragment, errors))


class CleanTree(Fixture):
    def test_shipped_tree_passes(self):
        self.assertEqual(check_plugin.check(self.root), [])

    def test_real_repository_passes(self):
        self.assertEqual(check_plugin.check(ROOT), [])


class PlantedDefects(Fixture):
    def test_marketplace_version_drift(self):
        self.edit_json(".claude-plugin/marketplace.json", lambda d: d["plugins"][0].update(version="0.4.1"))
        self.assertFlags("differs from plugin.json")

    def test_skill_metadata_version_drift(self):
        self.edit(SKILL, 'version: "0.4.0"', 'version: "0.3.9"')
        self.assertFlags("metadata.version")

    def test_package_version_checked_once_it_reaches_0_4(self):
        self.edit_json("package.json", lambda d: d.update(version="0.4.1"))
        self.assertFlags("does not match plugin")

    def test_package_prerelease_core_matches(self):
        self.edit_json("package.json", lambda d: d.update(version="0.4.0-beta.1"))
        self.assertEqual(check_plugin.check(self.root), [])

    def test_package_below_0_4_is_not_compared(self):
        self.edit_json("package.json", lambda d: d.update(version="0.3.2"))
        self.assertEqual(check_plugin.check(self.root), [])

    def test_prerelease_plugin_version_rejected(self):
        for rel in (".claude-plugin/marketplace.json",):
            self.edit_json(rel, lambda d: d["plugins"][0].update(version="0.4.0-beta.1"))
        self.edit_json("plugins/switchboard/.claude-plugin/plugin.json", lambda d: d.update(version="0.4.0-beta.1"))
        self.assertFlags("not a plain semver")

    def test_license_expression_must_be_exact(self):
        self.edit(SKILL, "license: PolyForm-Noncommercial-1.0.0 OR", "license: PolyForm-Noncommercial-1.0.0 AND")
        self.assertFlags("license must be exactly")

    def test_unquoted_colon_in_front_matter(self):
        self.edit(SKILL, "compatibility: Requires Fabric", "compatibility: Requires: Fabric")
        self.assertFlags("unquoted value holds ': '")

    def test_name_must_match_directory(self):
        self.edit(SKILL, "name: switching-accounts", "name: switching-account")
        self.assertFlags("must equal the directory name")

    def test_description_over_limit(self):
        self.edit(SKILL, "  Use when a coding agent", "  Use when " + "very " * 220 + "a coding agent")
        self.assertFlags("over 1024")

    def test_unknown_tool_named_in_skill(self):
        self.edit(SKILL, "| `switchboard_status` | no |", "| `switchboard_statsu` | no |")
        self.assertFlags("'switchboard_statsu'")

    def test_unknown_prefixed_tool_named_in_skill(self):
        self.edit(SKILL, "mcp__switchboard__switchboard_status", "mcp__switchboard__switchboard_login")
        self.assertFlags("'switchboard_login'")

    def test_undocumented_server_tool(self):
        self.edit(check_plugin.MCP_SOURCE, '"name":"switchboard_status"',
                  '"name":"switchboard_status"}), (false, json!({"name":"switchboard_history"')
        self.assertFlags("'switchboard_history'")

    def test_undocumented_apply_action(self):
        self.edit(check_plugin.APPLY_SOURCE, '"other_pool",', '"wrong_pool",')
        self.assertFlags("'wrong_pool'")

    def test_home_path_in_plugin_file(self):
        self.edit("plugins/switchboard/skills/switching-accounts/references/cli.md",
                  "/path/to/project", "/Users/example/project")
        self.assertFlags("absolute home path")

    def test_windows_home_path_in_evals(self):
        self.edit("test/evals/switching-accounts/triggers.json", "/path/to/project", "C:\\\\Users\\\\example")
        self.assertFlags("absolute home path")

    def test_eval_file_must_parse(self):
        (self.root / "test/evals/switching-accounts/scenarios.json").write_text("{", encoding="utf-8")
        self.assertFlags("not valid JSON")

    def test_triggers_need_negative_queries(self):
        self.edit_json("test/evals/switching-accounts/triggers.json",
                       lambda d: d.update(queries=[q for q in d["queries"] if q["shouldTrigger"]]))
        self.assertFlags("both should and should-not")

    def test_stray_skill_file(self):
        (self.root / "docs").mkdir()
        (self.root / "docs/SKILL.md").write_text("---\nname: x\n---\n", encoding="utf-8")
        self.assertFlags("stray skill")

    def test_mcp_server_command(self):
        self.edit_json("plugins/switchboard/.mcp.json",
                       lambda d: d["mcpServers"]["switchboard"].update(args=["serve"]))
        self.assertFlags(".mcp.json")

    def test_missing_reference_link(self):
        self.edit(SKILL, "- `references/cli.md` — read", "- `references/cli-missing.md` — read")
        self.assertFlags("cli-missing.md does not exist")


if __name__ == "__main__":
    unittest.main()
