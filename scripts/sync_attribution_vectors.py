#!/usr/bin/env python3
"""Copy the contract's folder-attribution vectors (DEC-0089, SB-141) into this repository.

Usage: scripts/sync_attribution_vectors.py [CONTRACT_CHECKOUT] [COMMIT]
Defaults: ~/DATA/fabric-agent-contract and its origin/main. The file is written as the contract
wrote it, with `contract` (the commit) and `about` added; it is never edited by hand.
"""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "crates/switchboard-runtime/tests/fixtures/folder-attribution.json"


def main() -> int:
    checkout = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "~/DATA/fabric-agent-contract").expanduser()
    commit = sys.argv[2] if len(sys.argv) > 2 else "origin/main"
    run = lambda *args: subprocess.run(["git", "-C", str(checkout), *args], check=True, capture_output=True, text=True).stdout
    short = run("rev-parse", "--short=8", commit).strip()
    vectors = json.loads(run("show", f"{commit}:fixtures/vectors/folder-attribution.json"))
    out = {
        "about": f"Copied from fabric-agent-contract fixtures/vectors/folder-attribution.json at {short} (DEC-0089); never edited here. Refresh with scripts/sync_attribution_vectors.py. Their own note: {vectors['about']}",
        "contract": short,
        "origin": vectors["origin"],
        "attribution": vectors["attribution"],
        "record": vectors["record"],
    }
    TARGET.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{TARGET.relative_to(ROOT)}: {short}, origin {len(out['origin'])}, attribution {len(out['attribution'])}, record {len(out['record'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
