#!/usr/bin/env python3
"""Quota monitor: one view of every subscription account, here and on the other machines,
every minute; stuck accounts found, refreshed and signalled; the plan of resets; and accounts
moved to a duty group or machine that has none with room.

Operator tooling beside `quota_sentry.py` (same private folder, same duty file, which also
names each group's tier: 1 the release, 2 support, 3 the rest). It reads Switchboard through
its CLI here and over ssh there, and acts only through `switchboard usage <id>` (one refresh
a run, inside the provider's budget, which is Switchboard's to keep), `accounts move` and
`accounts send`. It never changes the ordinary Claude Code sign-in.

    quota_monitor.py run [--dry-run]   # one pass: collect, detect, fix, distribute, write status
    quota_monitor.py status            # the last status as text
    quota_monitor.py install|uninstall # the one-minute LaunchAgent

Freshness, stated as it is: an account that carries traffic is read from response headers
(seconds old). An idle account is checked by Switchboard inside the provider's budget of two
checks a minute, so N idle accounts on one machine cannot each be younger than N / 2 minutes.
The status prints that floor and the ages measured; this tool does not go around the budget.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import importlib.util
import json
import pathlib
import subprocess
import sys
import time

_SPEC = importlib.util.spec_from_file_location("quota_sentry", pathlib.Path(__file__).with_name("quota_sentry.py"))
sentry = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(sentry)

LABEL = "me.sshlg.switchboard-quota-monitor"
STATE_DIR = sentry.STATE_DIR
BUDGET_PER_MINUTE = 2  # Switchboard's own budget of usage checks per provider
FULL = 97.0  # a window at or past this share has no room
OVERDUE = {"here": 1800, "there": 1200}  # seconds without a check before an account is stuck
MOVE_EVERY = 900  # one move into the same group at most this often
SEND_EVERY = 3600  # one move to another machine at most this often
SIGNAL_EVERY = 6 * 3600
DONOR_MIN_LEFT = 40.0  # a donor sent to another machine keeps at least this share of its week


# ---------------------------------------------------------------- one view

def row(account: dict, where: str) -> dict:
    usage = account.get("usage") or {}
    health = account.get("usage_health") or {}
    return {
        "where": where, "id": account.get("id"), "label": account.get("label"),
        "pool": account.get("pool"), "provider": account.get("provider"),
        "kind": account.get("kind", "oauth"), "enabled": account.get("enabled", True),
        "elsewhere": bool(account.get("elsewhere")), "assigned": bool(account.get("assigned_to")),
        "observed_at": usage.get("observed_at"), "source": usage.get("source"),
        "windows": {w["name"]: {"used": w.get("used_percent"), "reset": w.get("resets_at")}
                    for w in usage.get("windows") or []},
        "health": health.get("status"),
    }


def collect(machines: list[str]) -> tuple[list[dict], list[str]]:
    """Every account in use: this machine's own, and each other machine's own figures. The
    copy this machine keeps of an account that moved away is left out: the row from the
    machine that uses it replaces it."""
    degraded, rows = [], []
    here = sentry.cli("accounts", "list")
    if here is None:
        degraded.append("here: switchboard accounts list did not answer")
    for a in (here or {}).get("accounts", []):
        if not a.get("elsewhere"):
            rows.append(row(a, "here"))
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for machine, accounts in zip(machines, pool.map(sentry.remote_accounts, machines)):
            if accounts is None:
                degraded.append(f"{machine}: did not answer over ssh")
                continue
            rows.extend(row(a, machine) for a in accounts if not a.get("elsewhere") and a.get("enabled", True))
    return rows, degraded


def subscription(r: dict) -> bool:
    return r["kind"] == "oauth" and r["enabled"]


def effective(r: dict, now: int) -> dict[str, float | None]:
    """Used share per window as it is now: a window whose reset passed after the last
    observation is empty, whatever the stale figure says."""
    out = {}
    for name, w in r["windows"].items():
        passed = w["reset"] and w["reset"] <= now and (r["observed_at"] or 0) < w["reset"]
        out[name] = 0.0 if passed else w["used"]
    return out


def has_room(r: dict, now: int) -> bool:
    used = effective(r, now)
    return bool(used) and all(v is not None and v < FULL for v in used.values())


# ---------------------------------------------------------------- stuck accounts

def stuck(r: dict, now: int) -> list[str]:
    """Why an account's figures cannot be trusted; empty when they can."""
    if not subscription(r):
        return []
    reasons = []
    if r["health"] not in ("ok", None):
        reasons.append("sign_in")  # the provider refused the check: it needs a person
    if r["observed_at"] is None:
        return reasons + ["never_checked"]
    if any(w["reset"] and w["reset"] <= now and r["observed_at"] < w["reset"] for w in r["windows"].values()):
        reasons.append("reset_passed")
    week = r["windows"].get("seven_day") or {}
    spent = (week.get("used") or 0) >= 100 and (week.get("reset") or 0) > now
    # A week that is spent cannot change before its reset: an old figure there is still true,
    # and a check of it would only take the budget from an account that can change.
    if not spent and now - r["observed_at"] > OVERDUE["here" if r["where"] == "here" else "there"]:
        reasons.append("overdue")
    return reasons


def refresh(r: dict) -> bool:
    """Ask the provider about one account through its own machine's Switchboard."""
    try:
        if r["where"] == "here":
            return sentry.cli("usage", r["id"], timeout=90) is not None
        remote = sentry.REMOTE_SB.replace("accounts list", f"usage {r['id']}")
        out = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", r["where"], remote],
                             capture_output=True, text=True, timeout=60)
        return bool(json.loads(out.stdout).get("ok"))
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return False


# ---------------------------------------------------------------- resets and distribution

def resets(rows: list[dict], now: int) -> list[dict]:
    """When each account without room comes back, soonest first."""
    plan = []
    for r in rows:
        if not subscription(r):
            continue
        used = effective(r, now)
        waits = [w["reset"] for name, w in r["windows"].items()
                 if (used.get(name) or 0) >= FULL and w["reset"] and w["reset"] > now]
        if waits:
            plan.append({"at": max(waits), "where": r["where"], "pool": r["pool"], "label": r["label"],
                         "window": "seven_day" if (used.get("seven_day") or 0) >= FULL else "five_hour"})
    return sorted(plan, key=lambda p: p["at"])


def week_left(r: dict, now: int) -> float:
    return 100.0 - (effective(r, now).get("seven_day") or 0.0)


def distribute(rows: list[dict], tiers: dict[str, int], machines: list[str], now: int,
               state: dict, cli_account: str | None) -> list[dict]:
    """Moves that give a duty unit an account with room, highest tier first. A group of tier
    1 or 2 with none takes the default group's best account, else a tier-3 group's; another
    machine with none, and no reset within the hour, is sent one from the default group."""
    last = state.get("moved", {})
    taken: set[str] = set()

    def donors(pools_of_tier_three: bool) -> list[dict]:
        found = [r for r in rows if r["where"] == "here" and subscription(r) and r["provider"] == "claude"
                 and not r["assigned"] and r["id"] != cli_account and r["id"] not in taken and has_room(r, now)
                 and (r["pool"] == "default" or (pools_of_tier_three and tiers.get(r["pool"], 3) == 3
                                                 and r["pool"] != "default" and not r["pool"].startswith("machine_")))]
        return sorted(found, key=lambda r: (r["pool"] != "default", -week_left(r, now)))

    moves = []
    for pool in sorted((p for p, t in tiers.items() if t in (1, 2)), key=lambda p: (tiers[p], p)):
        own = [r for r in rows if r["where"] == "here" and r["pool"] == pool and subscription(r)]
        # A group with no account of its own is the feed's to serve, not this rule's.
        if not own or any(has_room(r, now) for r in own) or now - last.get(f"group:{pool}", 0) < MOVE_EVERY:
            continue
        pick = donors(True)
        if pick:
            taken.add(pick[0]["id"])
            moves.append({"act": "move", "to": pool, "account": pick[0]["id"], "label": pick[0]["label"],
                          "from": pick[0]["pool"], "unit": f"group:{pool}",
                          "reason": f"tier {tiers[pool]} group {pool} has no account with room"})
    for machine in machines:
        there = [r for r in rows if r["where"] == machine and subscription(r)]
        if not there or any(has_room(r, now) for r in there):
            continue
        soon = any(p["where"] == machine and p["at"] - now <= 3600 for p in resets(rows, now))
        if soon or now - last.get("send", 0) < SEND_EVERY:
            continue
        pick = [r for r in donors(False) if week_left(r, now) >= DONOR_MIN_LEFT]
        if pick:
            taken.add(pick[0]["id"])
            moves.append({"act": "send", "to": machine, "account": pick[0]["id"], "label": pick[0]["label"],
                          "from": pick[0]["pool"], "unit": "send",
                          "reason": f"{machine} has no account with room and no reset within the hour"})
            break
    return moves


def carry_out(move: dict) -> bool:
    if move["act"] == "move":
        return sentry.cli("accounts", "move", "--pool", move["to"], move["account"]) is not None
    return sentry.cli("accounts", "send", "--to", move["to"], move["account"], timeout=180) is not None


# ---------------------------------------------------------------- status

def freshness(rows: list[dict], now: int) -> dict:
    out = {}
    for where in sorted({r["where"] for r in rows}):
        ages = sorted(now - r["observed_at"] for r in rows
                      if r["where"] == where and subscription(r) and r["observed_at"])
        if not ages:
            continue
        idle = sum(1 for r in rows if r["where"] == where and subscription(r) and r["source"] != "response_headers")
        out[where] = {
            "accounts": len(ages), "under_a_minute": sum(1 for a in ages if a <= 60),
            "median_seconds": ages[len(ages) // 2], "oldest_seconds": ages[-1],
            "checked_by_usage_endpoint": idle,
            "floor_minutes_for_those": round(idle / BUDGET_PER_MINUTE, 1),
        }
    return out


def text(status: dict) -> str:
    def clock(at):
        return time.strftime("%m-%d %H:%M", time.gmtime(at)) if at else "-"
    lines = [f"Quota monitor, {clock(status['at'])} UTC", "", "Freshness (seconds since the last observation):"]
    for where, f in status["freshness"].items():
        lines.append(f"  {where}: {f['accounts']} accounts, {f['under_a_minute']} under a minute, median {f['median_seconds']}, "
                     f"oldest {f['oldest_seconds']}; {f['checked_by_usage_endpoint']} read by usage checks, which the "
                     f"provider's budget of {BUDGET_PER_MINUTE} a minute holds to one round in {f['floor_minutes_for_those']} minutes at best")
    lines += ["", f"Stuck: {len(status['stuck'])}"] + [f"  {s['where']} {s['label']}: {', '.join(s['reasons'])}" for s in status["stuck"]]
    lines += ["", "Resets ahead (UTC):"] + [f"  {clock(p['at'])}  {p['where']}  {p['pool']}  {p['label']}  ({p['window']})" for p in status["resets"]]
    lines += ["", "Moves this pass:"] + [f"  {m['act']} {m['label']} {m['from']} -> {m['to']}: {m['reason']} ({m.get('outcome')})" for m in status["moves"]]
    lines += ["", "Not read:"] + [f"  {d}" for d in status["degraded"]]
    return "\n".join(lines) + "\n"


def run(dry: bool) -> int:
    now = int(time.time())
    duty = json.loads(sentry.DUTY_FILE.read_text())
    machines = sorted(duty.get("machines", {}))
    tiers = duty.get("tiers", {})
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    state_file = STATE_DIR / "monitor-state.json"
    try:
        state = json.loads(state_file.read_text())
    except (OSError, ValueError):
        state = {}
    rows, degraded = collect(machines)
    found = [dict(r, reasons=stuck(r, now)) for r in rows]
    found = sorted((r for r in found if r["reasons"]), key=lambda r: r["observed_at"] or 0)
    # One refresh a pass, the stalest first: the provider's budget is Switchboard's to keep.
    fixable = sorted((r for r in found if r["reasons"] != ["sign_in"]),
                     key=lambda r: (tiers.get(r["pool"], 3) if r["where"] == "here" else 2, r["observed_at"] or 0))
    refreshed = None
    if fixable and not dry:
        refreshed = {"label": fixable[0]["label"], "where": fixable[0]["where"], "ok": refresh(fixable[0])}
    current = (sentry.cli("current") or {}).get("claude") or {}
    moves = distribute(rows, tiers, machines, now, state, current.get("account_id"))
    for move in moves:
        move["outcome"] = "dry run" if dry else ("done" if carry_out(move) else "failed")
        if move["outcome"] == "done":
            state.setdefault("moved", {})[move["unit"]] = now
    # Signal: a sign-in a person must renew, an account stuck for three passes, every move.
    seen = state.get("stuck_passes", {})
    passes = {f"{r['where']}:{r['id']}": seen.get(f"{r['where']}:{r['id']}", 0) + 1 for r in found}
    state["stuck_passes"] = passes
    notes = [f"{r['where']} {r['label']}: вход требует человека" for r in found if "sign_in" in r["reasons"]]
    notes += [f"{r['where']} {r['label']}: цифры не обновляются ({', '.join(r['reasons'])})"
              for r in found if "sign_in" not in r["reasons"] and passes[f"{r['where']}:{r['id']}"] == 10]
    notes += [f"Аккаунт {m['label']} переведён: {m['from']} -> {m['to']}. Причина: {m['reason']}."
              for m in moves if m["outcome"] == "done"]
    notes += [f"Не удалось перевести {m['label']} в {m['to']}: {m['reason']}." for m in moves if m["outcome"] == "failed"]
    signalled = None
    if notes and not dry and (any(m["outcome"] != "dry run" for m in moves) or now - state.get("signalled", 0) >= SIGNAL_EVERY):
        ok, signalled = sentry.deliver("\n".join(notes), f"sb-quota-monitor-{now // 60}", "failure")
        state["signalled"] = now
    status = {
        "at": now, "freshness": freshness(rows, now),
        "stuck": [{k: r[k] for k in ("where", "label", "pool", "reasons", "observed_at")} for r in found],
        "refreshed": refreshed, "resets": resets(rows, now), "moves": moves, "signalled": signalled,
        "degraded": degraded,
        "accounts": [dict({k: r[k] for k in ("where", "pool", "label", "provider", "observed_at", "source", "health")},
                          used=effective(r, now), room=has_room(r, now)) for r in rows if subscription(r)],
    }
    if not dry:
        state_file.write_text(json.dumps(state))
        if moves:
            with (STATE_DIR / "actions.log").open("a") as log:
                for m in moves:
                    log.write(f"{time.strftime('%FT%TZ', time.gmtime(now))} monitor {m['act']} {m['label']} {m['from']} -> {m['to']}: {m['reason']} ({m['outcome']})\n")
    (STATE_DIR / "status.json").write_text(json.dumps(status, ensure_ascii=False, indent=1))
    (STATE_DIR / "status.txt").write_text(text(status))
    print(f"accounts {len(status['accounts'])}, stuck {len(found)}, moves {len(moves)}, not read {len(degraded)}")
    return 0


def install() -> int:
    import os
    plist = sentry.HOME / "Library" / "LaunchAgents" / f"{LABEL}.plist"
    body = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>/usr/bin/python3</string><string>{pathlib.Path(__file__).resolve()}</string><string>run</string></array>
<key>StartInterval</key><integer>60</integer>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Background</string>
<key>EnvironmentVariables</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/bin:/bin:{sentry.HOME}/.local/bin</string></dict>
<key>StandardOutPath</key><string>{STATE_DIR}/monitor-launchd.log</string>
<key>StandardErrorPath</key><string>{STATE_DIR}/monitor-launchd.log</string>
</dict></plist>
"""
    plist.write_text(body)
    domain = f"gui/{os.getuid()}"
    subprocess.run(["/bin/launchctl", "bootout", f"{domain}/{LABEL}"], capture_output=True)
    done = subprocess.run(["/bin/launchctl", "bootstrap", domain, str(plist)], capture_output=True, text=True)
    print(f"installed {plist}" if done.returncode == 0 else f"bootstrap failed: {done.stderr.strip()}")
    return done.returncode


def uninstall() -> int:
    import os
    subprocess.run(["/bin/launchctl", "bootout", f"gui/{os.getuid()}/{LABEL}"], capture_output=True)
    (sentry.HOME / "Library" / "LaunchAgents" / f"{LABEL}.plist").unlink(missing_ok=True)
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["run", "status", "install", "uninstall"])
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "run":
        return run(args.dry_run)
    if args.command == "status":
        print((STATE_DIR / "status.txt").read_text())
        return 0
    return install() if args.command == "install" else uninstall()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
