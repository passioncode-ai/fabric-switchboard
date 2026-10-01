#!/usr/bin/env python3
"""Decide whether a nightly workflow run is the one full run of its Warsaw date.

    python3 scripts/nightly_clock.py "<cron line that fired, empty for a manual dispatch>"

Prints `run=true` or `run=false` (append it to $GITHUB_OUTPUT) and a one-line reason on stderr.

The workflow has two cron lines, 21:00 and 22:00 UTC, because 23:00 Europe/Warsaw is one or
the other depending on daylight saving time. GitHub starts scheduled runs late — by up to two
hours and twenty minutes in September 2026 (issue #5) — so the hour at which the run starts says
nothing. What decides is the slot the line was planned for: the latest occurrence of its UTC hour
at or before now. That slot is 23:00 in Warsaw for exactly one of the two lines on any date.
"""
from __future__ import annotations

import datetime as dt
import sys
from zoneinfo import ZoneInfo

WARSAW = ZoneInfo("Europe/Warsaw")
LINES = {"0 21 * * *": 21, "0 22 * * *": 22}


def should_run(schedule: str, now: dt.datetime) -> bool:
    if not schedule:
        return True  # workflow_dispatch: the operator asked for this run
    if schedule not in LINES:
        raise ValueError(f"unknown schedule line {schedule!r}; expected one of {sorted(LINES)}")
    planned = now.astimezone(dt.timezone.utc).replace(hour=LINES[schedule], minute=0, second=0, microsecond=0)
    if planned > now:
        planned -= dt.timedelta(days=1)
    return planned.astimezone(WARSAW).hour == 23


def main(argv: list[str]) -> int:
    schedule = argv[1] if len(argv) > 1 else ""
    now = dt.datetime.now(dt.timezone.utc)
    try:
        run = should_run(schedule, now)
    except ValueError as exc:
        print(f"nightly_clock: {exc}", file=sys.stderr)
        return 2
    why = "manual dispatch" if not schedule else (
        f"line {schedule!r} is the 23:00 Warsaw slot" if run
        else f"line {schedule!r} is the daylight-saving duplicate of tonight's slot")
    print(f"run={'true' if run else 'false'}")
    print(f"nightly_clock: {why} (started {now:%Y-%m-%d %H:%M} UTC)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
