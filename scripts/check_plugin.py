#!/usr/bin/env python3
"""Validate the Switchboard Claude Code plugin, its skill and its evals.

`claude plugin validate --strict` reads manifests only. This script holds the rest:
versions in step, the skill's front matter read as strictly as a YAML parser reads it,
the skill's tool names checked against the MCP server source, no home paths, evals that
parse. Standard library only; runs on Python 3.9+.

Usage: python3 scripts/check_plugin.py [--root PATH]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

PLUGIN_NAME = "switchboard"
LICENSE_SPDX = "PolyForm-Noncommercial-1.0.0 OR LicenseRef-PolyForm-Internal-Use-1.0.0"
AUTHOR = "PassionCode.ai"
MCP_SOURCE = "crates/switchboard-cli/src/mcp.rs"
APPLY_SOURCE = "crates/switchboard-runtime/src/projects.rs"
DESCRIPTION_LIMIT = 1024
COMPATIBILITY_LIMIT = 500
BODY_LINE_LIMIT = 500
NAME_PATTERN = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
HOME_PATH = re.compile(r"/Users/|[A-Za-z]:\\+Users\\+|/home/[A-Za-z0-9_.-]+/")
TOOL_DECLARATION = re.compile(r'"name"\s*:\s*"(switchboard_[a-z_]+)"')
# A bare tool name, not part of a longer identifier such as a host prefix.
TOOL_MENTION = re.compile(r"(?<![A-Za-z0-9_])(switchboard_[a-z]+(?:_[a-z]+)*)")
PREFIXED_MENTION = re.compile(r"mcp__(?:plugin_switchboard_)?switchboard__([a-z_]+)")
OUTCOME = re.compile(r'outcome\(\s*provider,\s*"([a-z_]+)"')
TUPLE_ACTION = re.compile(r'\(\s*"([a-z_]+)"\s*,\s*"')
SKIP_DIRS = {".git", "node_modules", "target", "dist", ".claude", "__pycache__", "gen"}


# --- strict front matter -------------------------------------------------------------
# A deliberately small YAML subset: a flat mapping whose values are plain, quoted or
# block scalars, plus one level of nested mapping. Anything outside it is an error, never
# a guess, because Claude Code reads front matter leniently and a strict reader drops the
# skill for the same text.
KEY = re.compile(r"^([A-Za-z0-9_-]+):(?:[ \t]+(.*))?$")
PLAIN_FORBIDDEN_FIRST = set("[]{},#&*!|>'\"%@`")


def _plain(value: str, lineno: int) -> str:
    if value[0] in PLAIN_FORBIDDEN_FIRST or (value[0] in "-?:" and value[1:2] in ("", " ")):
        raise ValueError("line %d: a plain value cannot start with %r; quote it" % (lineno, value[0]))
    if re.search(r":(?:[ \t]|$)", value):
        raise ValueError("line %d: an unquoted value holds ': ' or ends with ':'; quote it or use >-" % lineno)
    if re.search(r"[ \t]#", value):
        raise ValueError("line %d: ' #' starts a comment inside an unquoted value; quote it" % lineno)
    return value


def _scalar(value: str, lineno: int) -> str:
    value = value.strip()
    if value.startswith('"'):
        if len(value) < 2 or not value.endswith('"') or '"' in value[1:-1].replace('\\"', ""):
            raise ValueError("line %d: double-quoted value is not closed on its line" % lineno)
        return json.loads(value)
    if value.startswith("'"):
        if len(value) < 2 or not value.endswith("'") or "'" in value[1:-1].replace("''", ""):
            raise ValueError("line %d: single-quoted value is not closed on its line" % lineno)
        return value[1:-1].replace("''", "'")
    return _plain(value, lineno)


def _block(style: str, lines: List[str], start: int, indent: int, lineno: int) -> Tuple[str, int]:
    end = start
    while end < len(lines) and (not lines[end].strip() or
                                len(lines[end]) - len(lines[end].lstrip(" ")) > indent):
        end += 1
    body = [line.strip() for line in lines[start:end]]
    while body and not body[-1]:
        body.pop()
    if not body:
        raise ValueError("line %d: block scalar has no content" % lineno)
    if style.startswith("|"):
        return "\n".join(body), end
    text, paragraph = [], []
    for line in body:
        if line:
            paragraph.append(line)
        else:
            text.append(" ".join(paragraph))
            paragraph = []
    text.append(" ".join(paragraph))
    return "\n".join(text), end


def parse_frontmatter(text: str) -> Tuple[Dict[str, Any], str]:
    """Return (front matter, body); raise ValueError naming the line."""
    if not text.startswith("---\n"):
        raise ValueError("SKILL.md must start with '---' front matter")
    try:
        raw, body = text[4:].split("\n---\n", 1)
    except ValueError:
        raise ValueError("front matter is not closed by '---'") from None
    lines = raw.split("\n")
    data: Dict[str, Any] = {}
    index = 0
    while index < len(lines):
        line, lineno = lines[index], index + 2
        if not line.strip():
            index += 1
            continue
        if "\t" in line[: len(line) - len(line.lstrip())]:
            raise ValueError("line %d: tabs cannot indent YAML" % lineno)
        if line.startswith(" "):
            raise ValueError("line %d: unexpected indentation" % lineno)
        match = KEY.match(line)
        if not match:
            raise ValueError("line %d: expected 'key: value'" % lineno)
        key, value = match.group(1), (match.group(2) or "").strip()
        if key in data:
            raise ValueError("line %d: duplicate key %r" % (lineno, key))
        index += 1
        if value in (">", ">-", "|", "|-"):
            data[key], index = _block(value, lines, index, 0, lineno)
        elif value:
            data[key] = _scalar(value, lineno)
        else:
            nested: Dict[str, str] = {}
            while index < len(lines) and lines[index].startswith("  "):
                sub = KEY.match(lines[index].strip())
                if not sub or not sub.group(2):
                    raise ValueError("line %d: nested entries must be 'key: value'" % (index + 2))
                nested[sub.group(1)] = _scalar(sub.group(2), index + 2)
                index += 1
            if not nested:
                raise ValueError("line %d: %r has no value" % (lineno, key))
            data[key] = nested
    return data, body


# --- checks ---------------------------------------------------------------------------
def _json(path: Path, errors: List[str], root: Path) -> Optional[Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append("%s: missing" % path.relative_to(root))
    except (ValueError, UnicodeDecodeError) as exc:
        errors.append("%s: not valid JSON (%s)" % (path.relative_to(root), exc))
    return None


def _core(version: Any) -> Optional[Tuple[int, int, int]]:
    match = SEMVER.match(version) if isinstance(version, str) else None
    return tuple(int(part) for part in match.groups()[:3]) if match else None  # type: ignore[return-value]


def check_manifests(root: Path, errors: List[str]) -> Optional[str]:
    plugin = _json(root / "plugins" / PLUGIN_NAME / ".claude-plugin/plugin.json", errors, root)
    market = _json(root / ".claude-plugin/marketplace.json", errors, root)
    package = _json(root / "package.json", errors, root)
    mcp = _json(root / "plugins" / PLUGIN_NAME / ".mcp.json", errors, root)
    version = None
    if isinstance(plugin, dict):
        version = plugin.get("version")
        if plugin.get("name") != PLUGIN_NAME:
            errors.append("plugin.json: name must be %r" % PLUGIN_NAME)
        if _core(version) is None or "-" in str(version):
            errors.append("plugin.json: version %r is not a plain semver X.Y.Z" % version)
        for key in ("$schema", "displayName", "description", "homepage", "repository"):
            if not plugin.get(key):
                errors.append("plugin.json: %s is required" % key)
        if plugin.get("license") != LICENSE_SPDX:
            errors.append("plugin.json: license must be %r" % LICENSE_SPDX)
        if (plugin.get("author") or {}).get("name") != AUTHOR:
            errors.append("plugin.json: author.name must be %r" % AUTHOR)
    if isinstance(market, dict):
        if market.get("name") != PLUGIN_NAME:
            errors.append("marketplace.json: name must be %r" % PLUGIN_NAME)
        if (market.get("owner") or {}).get("name") != AUTHOR:
            errors.append("marketplace.json: owner.name must be %r" % AUTHOR)
        entries = [p for p in market.get("plugins") or [] if p.get("name") == PLUGIN_NAME]
        if len(entries) != 1:
            errors.append("marketplace.json: exactly one plugin entry named %r is required" % PLUGIN_NAME)
        else:
            entry = entries[0]
            if entry.get("source") != "./plugins/" + PLUGIN_NAME:
                errors.append("marketplace.json: source must be ./plugins/%s" % PLUGIN_NAME)
            if entry.get("version") != version:
                errors.append("marketplace.json: version %r differs from plugin.json %r" % (entry.get("version"), version))
            if entry.get("license") != LICENSE_SPDX:
                errors.append("marketplace.json: plugin license must be %r" % LICENSE_SPDX)
            if not entry.get("displayName"):
                errors.append("marketplace.json: plugin displayName is required")
    if isinstance(package, dict) and version is not None:
        package_core, plugin_core = _core(package.get("version")), _core(version)
        # The app moves to the plugin's version at 0.4.0; before that the two differ by design.
        if package_core is None:
            errors.append("package.json: version %r is not semver" % package.get("version"))
        elif package_core >= (0, 4, 0) and package_core != plugin_core:
            errors.append("package.json: version %r does not match plugin %r" % (package.get("version"), version))
        if package.get("license") != LICENSE_SPDX:
            errors.append("package.json: license must be %r" % LICENSE_SPDX)
    if isinstance(mcp, dict):
        server = (mcp.get("mcpServers") or {}).get(PLUGIN_NAME)
        if not isinstance(server, dict):
            errors.append(".mcp.json: mcpServers.%s is required" % PLUGIN_NAME)
        elif server.get("command") != "switchboard" or server.get("args") != ["mcp"]:
            errors.append(".mcp.json: the server must run 'switchboard' with args ['mcp']")
    return version if isinstance(version, str) else None


def declared_tools(root: Path, errors: List[str]) -> List[str]:
    try:
        tools = TOOL_DECLARATION.findall((root / MCP_SOURCE).read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append("%s: missing" % MCP_SOURCE)
        return []
    if not tools:
        errors.append("%s: no tool declarations found" % MCP_SOURCE)
    return tools


def apply_actions(root: Path, errors: List[str]) -> List[str]:
    try:
        source = (root / APPLY_SOURCE).read_text(encoding="utf-8")
    except FileNotFoundError:
        errors.append("%s: missing" % APPLY_SOURCE)
        return []
    start = source.find("pub(crate) fn apply(")
    end = source.find("\nfn ", start)
    body = source[start:end] if start >= 0 else ""
    actions = sorted(set(OUTCOME.findall(body)) | set(TUPLE_ACTION.findall(body)))
    if not actions:
        errors.append("%s: no apply actions found" % APPLY_SOURCE)
    return actions


def check_skills(root: Path, version: Optional[str], errors: List[str]) -> List[str]:
    skills_dir = root / "plugins" / PLUGIN_NAME / "skills"
    skills = sorted(p for p in skills_dir.iterdir() if p.is_dir()) if skills_dir.is_dir() else []
    if not skills:
        errors.append("plugins/%s/skills: no skill" % PLUGIN_NAME)
    tools = declared_tools(root, errors)
    actions = apply_actions(root, errors)
    for skill in skills:
        rel = skill.relative_to(root)
        path = skill / "SKILL.md"
        if not path.is_file():
            errors.append("%s: SKILL.md missing" % rel)
            continue
        try:
            front, body = parse_frontmatter(path.read_text(encoding="utf-8"))
        except ValueError as exc:
            errors.append("%s/SKILL.md: %s" % (rel, exc))
            continue
        name, description = front.get("name"), front.get("description")
        if name != skill.name:
            errors.append("%s/SKILL.md: name %r must equal the directory name" % (rel, name))
        if not isinstance(name, str) or not NAME_PATTERN.match(name) or len(name) > 64:
            errors.append("%s/SKILL.md: name must be lowercase words joined by hyphens, at most 64" % rel)
        elif "anthropic" in name or "claude" in name:
            errors.append("%s/SKILL.md: name cannot contain 'anthropic' or 'claude'" % rel)
        if not isinstance(description, str) or not description.strip():
            errors.append("%s/SKILL.md: description is required" % rel)
        else:
            if len(description) > DESCRIPTION_LIMIT:
                errors.append("%s/SKILL.md: description is %d characters, over %d" % (rel, len(description), DESCRIPTION_LIMIT))
            if "<" in description or ">" in description:
                errors.append("%s/SKILL.md: description cannot hold angle brackets" % rel)
            if not description.startswith("Use when"):
                errors.append("%s/SKILL.md: description must start with 'Use when'" % rel)
            if "NOT for" not in description:
                errors.append("%s/SKILL.md: description needs a 'NOT for' boundary" % rel)
        if front.get("license") != LICENSE_SPDX:
            errors.append("%s/SKILL.md: license must be exactly %r" % (rel, LICENSE_SPDX))
        compatibility = front.get("compatibility")
        if not isinstance(compatibility, str) or len(compatibility) > COMPATIBILITY_LIMIT:
            errors.append("%s/SKILL.md: compatibility is required, at most %d characters" % (rel, COMPATIBILITY_LIMIT))
        metadata = front.get("metadata")
        if not isinstance(metadata, dict) or metadata.get("author") != AUTHOR:
            errors.append("%s/SKILL.md: metadata.author must be %r" % (rel, AUTHOR))
        elif metadata.get("version") != version:
            errors.append("%s/SKILL.md: metadata.version %r differs from plugin %r" % (rel, metadata.get("version"), version))
        allowed = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
        for key in sorted(set(front) - allowed):
            errors.append("%s/SKILL.md: front matter key %r is outside the portable set" % (rel, key))
        if body.count("\n") >= BODY_LINE_LIMIT:
            errors.append("%s/SKILL.md: body has %d lines, limit %d" % (rel, body.count("\n"), BODY_LINE_LIMIT))
        for link in re.findall(r"`(references/[^`]+)`", body):
            if not (skill / link).is_file():
                errors.append("%s/SKILL.md: %s does not exist" % (rel, link))
        corpus = "\n".join(p.read_text(encoding="utf-8") for p in sorted(skill.rglob("*.md")))
        mentioned = set(TOOL_MENTION.findall(corpus)) | set(PREFIXED_MENTION.findall(corpus))
        for tool in sorted(mentioned - set(tools)):
            errors.append("%s: names tool %r, which %s does not declare" % (rel, tool, MCP_SOURCE))
        for tool in tools:
            if tool not in mentioned:
                errors.append("%s: tool %r from %s is not documented" % (rel, tool, MCP_SOURCE))
        for action in actions:
            if "`%s`" % action not in body:
                errors.append("%s/SKILL.md: apply action %r from %s is not documented" % (rel, action, APPLY_SOURCE))
    return [s.name for s in skills]


def check_evals(root: Path, skills: List[str], errors: List[str]) -> None:
    evals = root / "test/evals"
    present = sorted(p.name for p in evals.iterdir() if p.is_dir()) if evals.is_dir() else []
    for extra in sorted(set(present) - set(skills)):
        errors.append("test/evals/%s: no skill of that name" % extra)
    for skill in skills:
        triggers = _json(evals / skill / "triggers.json", errors, root)
        if isinstance(triggers, dict):
            queries = triggers.get("queries")
            if triggers.get("skill") != skill or not isinstance(queries, list):
                errors.append("test/evals/%s/triggers.json: needs skill %r and a queries list" % (skill, skill))
            else:
                if not all(isinstance(q, dict) and isinstance(q.get("query"), str) and q["query"].strip()
                           and isinstance(q.get("shouldTrigger"), bool) for q in queries):
                    errors.append("test/evals/%s/triggers.json: every query needs text and a shouldTrigger bool" % skill)
                else:
                    positive = [q["query"] for q in queries if q["shouldTrigger"]]
                    if not positive or len(positive) == len(queries):
                        errors.append("test/evals/%s/triggers.json: needs both should and should-not queries" % skill)
                    if not any(re.search("[Ѐ-ӿ]", q) for q in positive):
                        errors.append("test/evals/%s/triggers.json: needs a Russian should-trigger query" % skill)
        scenarios = _json(evals / skill / "scenarios.json", errors, root)
        if isinstance(scenarios, dict):
            items = scenarios.get("scenarios")
            if scenarios.get("skill") != skill or not isinstance(items, list) or len(items) < 3:
                errors.append("test/evals/%s/scenarios.json: needs skill %r and at least 3 scenarios" % (skill, skill))
            else:
                ids = [s.get("id") for s in items if isinstance(s, dict)]
                if len(ids) != len(items) or len(set(ids)) != len(ids):
                    errors.append("test/evals/%s/scenarios.json: scenario ids must be unique" % skill)
                for item in items:
                    if not (isinstance(item, dict) and all(isinstance(item.get(k), str) and item[k] for k in ("id", "trap", "request"))
                            and isinstance(item.get("expected"), list) and item["expected"]
                            and all(isinstance(e, str) and e for e in item["expected"])):
                        errors.append("test/evals/%s/scenarios.json: %r needs id, trap, request and expected" % (skill, item))


def _files(base: Path) -> List[Path]:
    """Every file under base, pruning build output and dependency trees before descending."""
    out = []
    for folder, dirs, files in os.walk(base):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        out.extend(Path(folder) / name for name in sorted(files))
    return out


def check_privacy_and_layout(root: Path, errors: List[str]) -> None:
    scanned = [root / ".claude-plugin", root / "plugins", root / "test/evals"]
    for base in scanned:
        if not base.is_dir():
            continue
        for path in _files(base):
            try:
                text = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            for lineno, line in enumerate(text.splitlines(), 1):
                if HOME_PATH.search(line):
                    errors.append("%s:%d: absolute home path" % (path.relative_to(root), lineno))
    for path in _files(root):
        rel = path.relative_to(root)
        if path.name == "SKILL.md" and not (len(rel.parts) == 5 and rel.parts[0] == "plugins" and rel.parts[2] == "skills"):
            errors.append("%s: a SKILL.md outside plugins/*/skills/*/ ships as a stray skill" % rel)
    plugin_meta = root / "plugins" / PLUGIN_NAME / ".claude-plugin"
    if plugin_meta.is_dir():
        for path in plugin_meta.iterdir():
            if path.name != "plugin.json":
                errors.append("%s: only plugin.json belongs in .claude-plugin/" % path.relative_to(root))


def check(root: Path) -> List[str]:
    errors: List[str] = []
    version = check_manifests(root, errors)
    skills = check_skills(root, version, errors)
    check_evals(root, skills, errors)
    check_privacy_and_layout(root, errors)
    return errors


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args(argv)
    errors = check(args.root.resolve())
    for error in errors:
        print(error, file=sys.stderr)
    print("switchboard plugin: %d errors" % len(errors))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
