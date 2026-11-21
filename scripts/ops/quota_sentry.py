#!/usr/bin/env python3
"""Quota sentry: warn ahead of time that a group with an agent on duty will run out of
live subscription accounts.

Operator tooling for the machine that runs Switchboard, not part of the product: it reads
Switchboard through its own CLI (`switchboard --json accounts list`, `feed status`), reads
the relay counters for each group's spend of the last 24 hours, forecasts hour by hour and
reports every duty unit (a group here, or another machine's space) whose runway is shorter
than the warning line (12 hours by default). It changes nothing in Switchboard.

    quota_sentry.py check              # the forecast as JSON, nothing sent
    quota_sentry.py run [--dry-run]    # forecast, then deliver new warnings once
    quota_sentry.py install|uninstall  # the hourly LaunchAgent of this machine

Model (report 2026-11-21-subscription-runway, section "Model"):
- an account's weekly window holds CAP list-price dollars (calibrated: median 19.9 dollars
  per 1 % over 28 accounts on 2026-11-21) and a five-hour window H5 of it (an assumption);
- a group spends what the proxy counted for it in the last 24 hours, never less than FLOOR;
- a group draws on its own accounts first (soonest reset first), then on the default
  group's lendable accounts in feed priority order, as the feed does;
- another machine's space spends at the pace its accounts' weekly windows show.

Delivery is once per episode: a unit is announced when it crosses the line, again only
after REPEAT hours while it stays under it, and once when it recovers. The state lives in
the sentry's own folder, never in Switchboard's data folder.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import subprocess
import sys
import time

CAP = 2000.0  # list-price dollars one account's weekly window holds
H5 = 0.10  # share of the weekly window one five-hour window holds (assumption)
FLOOR = 24.0  # dollars a day a duty group is assumed to spend at least
WARN_HOURS = 12
HORIZON_HOURS = 72
REPEAT_HOURS = 6
WEEK = 7 * 86400
LABEL = "me.sshlg.switchboard-quota-sentry"
HOME = pathlib.Path.home()
STATE_DIR = HOME / "Library" / "Application Support" / "switchboard-quota-sentry"
DATA_DIR = HOME / "Library" / "Application Support" / "ai.passioncode.fabric-switchboard"
# Which groups and machines carry an agent on duty: this machine's own file, never in Git
# (it names private agents and hosts); `quota_sentry.duty.example.json` shows its shape.
DUTY_FILE = STATE_DIR / "duty.json"
SWITCHBOARD = os.environ.get("SWITCHBOARD_BIN") or str(HOME / ".local" / "bin" / "switchboard")
REMOTE_SB = (
    'SB=switchboard; A="/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard"; '
    '[ -x "$A" ] && SB="$A"; "$SB" --json accounts list'
)


# ---------------------------------------------------------------- reading

def cli(*args: str, timeout: int = 60):
    """One Switchboard CLI read; None when it did not answer (never an empty answer)."""
    try:
        out = subprocess.run([SWITCHBOARD, "--json", *args], capture_output=True, text=True, timeout=timeout)
        doc = json.loads(out.stdout)
        return doc.get("data") if doc.get("ok") else None
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return None


def remote_accounts(machine: str, timeout: int = 25):
    try:
        out = subprocess.run(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", machine, REMOTE_SB],
            capture_output=True, text=True, timeout=timeout,
        )
        data = json.loads(out.stdout).get("data")
        return data.get("accounts") if isinstance(data, dict) else data
    except (OSError, ValueError, AttributeError, subprocess.TimeoutExpired):
        return None


def spend_last_day(now: int, path: pathlib.Path | None = None) -> dict[str, float] | None:
    """List-price dollars per group over today and yesterday (UTC), scaled to 24 hours."""
    path = path or DATA_DIR / "statistics.json"
    try:
        tallies = json.loads(path.read_text())["tallies"]
    except (OSError, ValueError, KeyError):
        return None
    today = dt.datetime.fromtimestamp(now, dt.timezone.utc)
    days = {today.date().isoformat(), (today.date() - dt.timedelta(days=1)).isoformat()}
    hours = 24 + today.hour + today.minute / 60
    spend: dict[str, float] = {}
    for key, row in tallies.items():
        part = key.split("\t")
        if len(part) >= 3 and part[0] in days:
            spend[part[2]] = spend.get(part[2], 0.0) + row.get("cost_micros", 0) / 1e6
    return {pool: value * 24 / hours for pool, value in spend.items()}


def week_window(account: dict) -> dict:
    windows = (account.get("usage") or {}).get("windows") or []
    for window in windows:
        if window.get("name") == "seven_day":
            return window
    return {}


def as_units(accounts: list[dict], now: int, pool_of=lambda a: a.get("pool")) -> list[dict]:
    """Subscription accounts as forecast units: dollars left and the weekly reset."""
    units = []
    for a in accounts:
        if a.get("provider") != "claude" or a.get("kind", "oauth") != "oauth":
            continue
        if not a.get("enabled", True) or a.get("elsewhere"):
            continue
        week = week_window(a)
        used = week.get("used_percent")
        units.append({
            "label": a.get("label"),
            "pool": pool_of(a),
            "known": used is not None,
            "left": (100.0 - (used or 0.0)) / 100.0 * CAP,
            "reset": week.get("resets_at") or now + WEEK,
            "lendable": not a.get("assigned_to"),
            "recent": [0.0] * 5,
        })
    return units


# ---------------------------------------------------------------- forecast

def draw(accounts: list[dict], need: float) -> float:
    """Take `need` dollars for one hour, soonest reset first, inside each five-hour cap."""
    for a in sorted(accounts, key=lambda a: a["reset"]):
        if need <= 1e-9:
            break
        room = min(a["left"], CAP * H5 - sum(a["recent"]))
        take = max(0.0, min(room, need))
        a["left"] -= take
        a["recent"][-1] += take
        need -= take
    return need


def forecast(units: list[dict], demand: dict[str, float], priority: dict[str, int],
             now: int, cli_account: str | None = None, horizon: int = HORIZON_HOURS) -> dict[str, int | None]:
    """Hours until each group in `demand` first has an hour it cannot serve; None when the
    horizon holds. Groups are served in feed priority order (1 first); a group outside the
    feed draws on its own accounts only."""
    units = [dict(u, recent=list(u["recent"])) for u in units]
    first: dict[str, int | None] = {pool: None for pool in demand}
    order = sorted(demand, key=lambda pool: (priority.get(pool, 99), pool))
    for hour in range(horizon):
        at = now + hour * 3600
        for u in units:
            u["recent"] = u["recent"][1:] + [0.0]
            if at >= u["reset"]:
                u["left"], u["reset"] = CAP, u["reset"] + WEEK
        for pool in order:
            need = demand[pool] / 24
            need = draw([u for u in units if u["pool"] == pool], need)
            if need > 1e-9 and pool in priority:
                need = draw([u for u in units if u["pool"] == "default" and u["lendable"]
                             and u["label"] != cli_account], need)
            if need > demand[pool] / 48 and first[pool] is None:
                first[pool] = hour
    return first


def machine_runway(accounts: list[dict], now: int, horizon: int = HORIZON_HOURS) -> tuple[int | None, float]:
    """A remote space: every account serves it; it spends at its weekly windows' pace."""
    units = as_units(accounts, now, pool_of=lambda a: "space")
    pace = 0.0
    for a in accounts:
        week = week_window(a)
        if week.get("used_percent") and week.get("resets_at") and a.get("enabled", True) and not a.get("elsewhere"):
            elapsed = max(6.0, (WEEK - (week["resets_at"] - now)) / 3600)
            pace += week["used_percent"] / 100.0 * CAP / elapsed * 24
    demand = {"space": max(pace, FLOOR)}
    return forecast(units, demand, {}, now, horizon=horizon)["space"], demand["space"]


def survey(now: int, duty: dict) -> dict:
    """Every duty unit with its runway. A source that did not answer is named in
    `degraded` and its units read `unknown`, never as healthy."""
    degraded: list[str] = []
    rows: list[dict] = []
    accounts = cli("accounts", "list")
    feed = cli("feed", "status")
    spend = spend_last_day(now)
    if accounts is None:
        degraded.append("switchboard accounts list did not answer")
    if feed is None:
        degraded.append("switchboard feed status did not answer")
    if spend is None:
        degraded.append("relay counters unreadable: every group assumed at the floor")
    routes = (accounts or {}).get("routes") or {}
    cli_route = routes.get("claude_cli") or {}
    cli_account = cli_route.get("label") if isinstance(cli_route, dict) else None
    priority = {f["pool"]: f["priority"] for f in (feed or {}).get("fed", []) if not f.get("off")}
    local = as_units((accounts or {}).get("accounts", []), now)
    groups = duty.get("groups", {})
    demand = {pool: max((spend or {}).get(pool, 0.0), FLOOR) for pool in set(groups) | set(priority)}
    hours = forecast(local, demand, priority, now, cli_account) if accounts is not None else {}
    for pool, agents in sorted(groups.items()):
        own = [u for u in local if u["pool"] == pool]
        rows.append({
            "unit": f"group:{pool}", "agents": agents,
            "runway_hours": hours.get(pool) if accounts is not None else "unknown",
            "spend_per_day": round(demand.get(pool, FLOOR)),
            "own_accounts": len(own),
            "own_with_room": sum(1 for u in own if u["left"] > CAP * 0.03),
            "fed_priority": priority.get(pool),
        })
    for machine, agents in sorted(duty.get("machines", {}).items()):
        there = remote_accounts(machine)
        if there is None:
            degraded.append(f"{machine} did not answer over ssh")
            rows.append({"unit": f"machine:{machine}", "agents": agents, "runway_hours": "unknown"})
            continue
        runway, pace = machine_runway(there, now)
        live = [a for a in there if a.get("enabled", True) and not a.get("elsewhere")]
        rows.append({
            "unit": f"machine:{machine}", "agents": agents, "runway_hours": runway,
            "spend_per_day": round(pace), "own_accounts": len(live),
            "own_with_room": sum(1 for a in live if (week_window(a).get("used_percent") or 0) < 97),
        })
    return {"at": now, "warn_hours": WARN_HOURS, "horizon_hours": HORIZON_HOURS, "rows": rows, "degraded": degraded}


# ---------------------------------------------------------------- delivery

def due(report: dict, state: dict, now: int, warn: int = WARN_HOURS) -> tuple[list[dict], list[str], dict]:
    """Which units to announce now, which recovered, and the next state."""
    warned = dict(state.get("warned", {}))
    alerts, recovered = [], []
    for row in report["rows"]:
        runway, unit = row["runway_hours"], row["unit"]
        short = isinstance(runway, int) and runway < warn
        if short:
            if now - warned.get(unit, 0) >= REPEAT_HOURS * 3600:
                alerts.append(row)
                warned[unit] = now
        elif runway != "unknown" and unit in warned:
            recovered.append(unit)
            del warned[unit]
    return alerts, recovered, {"warned": warned, "at": now}


def message(alerts: list[dict], recovered: list[str], report: dict) -> str:
    lines = []
    for row in alerts:
        name = row["unit"].split(":", 1)[1]
        when = "уже сейчас" if row["runway_hours"] == 0 else f"через {row['runway_hours']} ч"
        lines.append(
            f"{name}: живой аккаунт кончится {when}. На дежурстве: {', '.join(row['agents'])}. "
            f"Своих аккаунтов с запасом {row.get('own_with_room', '?')} из {row.get('own_accounts', '?')}, "
            f"расход около ${row.get('spend_per_day', '?')} в день."
        )
    for unit in recovered:
        lines.append(f"{unit.split(':', 1)[1]}: запас снова больше {report['warn_hours']} ч.")
    for note in report["degraded"]:
        lines.append(f"Не прочитано: {note}.")
    return "\n".join(lines)


def deliver(text: str, event: str, kind: str) -> tuple[bool, str]:
    """Through the personal communicator, by a short headless Claude Code run that owns the
    communicator's MCP sign-in; the macOS notification centre when that run fails."""
    claude = os.environ.get("CLAUDE_BIN") or "/opt/homebrew/bin/claude"
    prompt = (
        "Call the tool communicator_notify of the personal-communicator MCP server exactly once with: "
        f"eventId {json.dumps(event)}, kind {json.dumps(kind)}, project \"fabric-switchboard\", "
        "agent \"quota sentry\", subject \"Подписки: запас по группам\", and text exactly the JSON string "
        f"below, decoded. Then answer with the single word sent or failed.\n{json.dumps(text, ensure_ascii=False)}"
    )
    try:
        out = subprocess.run(
            [claude, "-p", prompt, "--allowedTools", "mcp__personal-communicator__communicator_notify", "--max-turns", "4"],
            capture_output=True, text=True, timeout=240, cwd=str(pathlib.Path(__file__).resolve().parents[2]),
        )
        if out.returncode == 0 and "sent" in out.stdout.lower():
            return True, "communicator"
        detail = (out.stdout or out.stderr).strip()[-200:]
    except (OSError, subprocess.TimeoutExpired) as error:
        detail = str(error)[:200]
    script = f"display notification {json.dumps(text[:230], ensure_ascii=False)} with title \"Switchboard: подписки\""
    subprocess.run(["/usr/bin/osascript", "-e", script], capture_output=True, timeout=20)
    return False, f"communicator run failed ({detail}); macOS notification shown"


def run(dry: bool) -> int:
    now = int(time.time())
    duty = json.loads(DUTY_FILE.read_text())
    report = survey(now, duty)
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    state_file = STATE_DIR / "state.json"
    try:
        state = json.loads(state_file.read_text())
    except (OSError, ValueError):
        state = {}
    alerts, recovered, new_state = due(report, state, now)
    (STATE_DIR / "last-report.json").write_text(json.dumps(report, ensure_ascii=False, indent=1))
    outcome = "nothing due"
    if alerts or recovered:
        text = message(alerts, recovered, report)
        if dry:
            outcome = "dry run: " + text
        else:
            ok, outcome = deliver(text, f"sb-quota-sentry-{now // 3600}", "failure" if alerts else "recovery")
            if not ok:
                new_state = state  # undelivered: say it again at the next run
    if not dry:
        state_file.write_text(json.dumps(new_state))
    with (STATE_DIR / "sentry.log").open("a") as log:
        log.write(json.dumps({"at": now, "alerts": [r["unit"] for r in alerts], "recovered": recovered,
                              "degraded": report["degraded"], "outcome": outcome[:300]}, ensure_ascii=False) + "\n")
    print(outcome)
    return 0


def install() -> int:
    plist = HOME / "Library" / "LaunchAgents" / f"{LABEL}.plist"
    body = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>/usr/bin/python3</string><string>{pathlib.Path(__file__).resolve()}</string><string>run</string></array>
<key>StartInterval</key><integer>3600</integer>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Background</string>
<key>EnvironmentVariables</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/bin:/bin:{HOME}/.local/bin</string></dict>
<key>StandardOutPath</key><string>{STATE_DIR}/launchd.log</string>
<key>StandardErrorPath</key><string>{STATE_DIR}/launchd.log</string>
</dict></plist>
"""
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    plist.write_text(body)
    domain = f"gui/{os.getuid()}"
    subprocess.run(["/bin/launchctl", "bootout", f"{domain}/{LABEL}"], capture_output=True)
    done = subprocess.run(["/bin/launchctl", "bootstrap", domain, str(plist)], capture_output=True, text=True)
    print(f"installed {plist}" if done.returncode == 0 else f"bootstrap failed: {done.stderr.strip()}")
    return done.returncode


def uninstall() -> int:
    plist = HOME / "Library" / "LaunchAgents" / f"{LABEL}.plist"
    subprocess.run(["/bin/launchctl", "bootout", f"gui/{os.getuid()}/{LABEL}"], capture_output=True)
    plist.unlink(missing_ok=True)
    print(f"removed {plist}")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["check", "run", "install", "uninstall"])
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "check":
        print(json.dumps(survey(int(time.time()), json.loads(DUTY_FILE.read_text())), ensure_ascii=False, indent=1))
        return 0
    if args.command == "run":
        return run(args.dry_run)
    return install() if args.command == "install" else uninstall()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
