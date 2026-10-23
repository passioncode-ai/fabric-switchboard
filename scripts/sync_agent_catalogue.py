#!/usr/bin/env python3
"""Copy the contract's coding-agent catalogue (DEC-0093, SB-154) into catalog/coding-agents.contract.json.

Usage: scripts/sync_agent_catalogue.py [CONTRACT_CHECKOUT] [COMMIT]
Defaults: ~/DATA/fabric-agent-contract and its origin/main. agent_catalog::tests then checks that
catalog/agents.json names the same agents with the same name, maker, URL, binary and MCP support.
"""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "catalog/coding-agents.contract.json"


def main() -> int:
    checkout = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "~/DATA/fabric-agent-contract").expanduser()
    commit = sys.argv[2] if len(sys.argv) > 2 else "origin/main"
    run = lambda *args: subprocess.run(["git", "-C", str(checkout), *args], check=True, capture_output=True, text=True).stdout
    short = run("log", "-1", "--format=%h", "--abbrev=8", commit, "--", "catalogue/coding-agents.json").strip()
    catalogue = json.loads(run("show", f"{commit}:catalogue/coding-agents.json"))
    old = json.loads(TARGET.read_text(encoding="utf-8"))
    out = {"about": old["about"].replace(old["contract"], short), "contract": short, "catalogue": catalogue}
    TARGET.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{TARGET.relative_to(ROOT)}: {short}, {len(catalogue['agents'])} agents")
    return 0


if __name__ == "__main__":
    sys.exit(main())
