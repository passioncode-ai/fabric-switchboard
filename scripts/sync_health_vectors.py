#!/usr/bin/env python3
"""Copy the contract's agent-health/1 vectors (DEC-0179, SB-393) into this repository.

Usage: scripts/sync_health_vectors.py [CONTRACT_CHECKOUT] [COMMIT]
Defaults: ~/DATA/fabric-agent-contract and its origin/main. The file is written as the contract
wrote it, with `contract` (the commit) and `about` added; it is never edited by hand.
"""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "crates/switchboard-runtime/tests/fixtures/agent-health.json"


def main() -> int:
    checkout = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "~/DATA/fabric-agent-contract").expanduser()
    commit = sys.argv[2] if len(sys.argv) > 2 else "origin/main"
    run = lambda *args: subprocess.run(["git", "-C", str(checkout), *args], check=True, capture_output=True, text=True).stdout
    short = run("rev-parse", "--short=8", commit).strip()
    vectors = json.loads(run("show", f"{commit}:fixtures/vectors/agent-health.json"))
    # SB-550 (review round 3 of PR #861): the closed list of check areas (HL-3), so a test can hold
    # every area our report emits to the contract's schema.
    schema = json.loads(run("show", f"{commit}:schemas/agent-health.schema.json"))
    areas = schema["$defs"]["check"]["properties"]["area"]["enum"]
    out = {
        "about": f"Copied from fabric-agent-contract fixtures/vectors/agent-health.json at {short} (DEC-0179), with areas: the check area enum of schemas/agent-health.schema.json at the same commit; never edited here. Refresh with scripts/sync_health_vectors.py. Their own note: {vectors['about']}",
        "contract": short,
        "worst": vectors["worst"],
        "read": vectors["read"],
        "episodes": vectors["episodes"],
        "areas": areas,
    }
    TARGET.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{TARGET.relative_to(ROOT)}: {short}, worst {len(out['worst'])}, read {len(out['read'])}, episodes {len(out['episodes'])}, areas {len(out['areas'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
