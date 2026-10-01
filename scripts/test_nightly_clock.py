"""Tests for scripts/nightly_clock.py: one full nightly run per Warsaw date, whatever the delay."""
import datetime as dt
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from nightly_clock import should_run  # noqa: E402

UTC = dt.timezone.utc


def at(text: str) -> dt.datetime:
    return dt.datetime.fromisoformat(text).replace(tzinfo=UTC)


class NightlyClock(unittest.TestCase):
    def test_summer_runs_the_21_utc_line_even_when_github_starts_it_late(self):
        # Issue #5: the 2026-09-30 run started at 00:50 UTC for the 21:00 line.
        self.assertTrue(should_run("0 21 * * *", at("2026-09-30T00:50:00")))
        self.assertTrue(should_run("0 21 * * *", at("2026-09-29T21:00:30")))

    def test_summer_skips_the_22_utc_line_as_the_dst_duplicate(self):
        self.assertFalse(should_run("0 22 * * *", at("2026-10-01T00:53:46")))
        self.assertFalse(should_run("0 22 * * *", at("2026-09-29T22:05:00")))

    def test_winter_runs_the_22_utc_line_and_skips_the_21_utc_line(self):
        self.assertTrue(should_run("0 22 * * *", at("2026-12-02T01:30:00")))
        self.assertFalse(should_run("0 21 * * *", at("2026-12-02T01:30:00")))

    def test_the_planned_slot_decides_across_the_dst_change(self):
        # Summer time ends 2026-10-25 01:00 UTC. The 21:00 UTC slot of 10-24 is 23:00 CEST;
        # a start delayed past the change still belongs to that slot.
        self.assertTrue(should_run("0 21 * * *", at("2026-10-25T02:10:00")))
        self.assertFalse(should_run("0 22 * * *", at("2026-10-25T02:10:00")))
        # The next evening is winter: 22:00 UTC is 23:00 CET.
        self.assertTrue(should_run("0 22 * * *", at("2026-10-25T23:40:00")))

    def test_a_manual_dispatch_always_runs(self):
        self.assertTrue(should_run("", at("2026-09-30T09:26:16")))

    def test_an_unknown_schedule_line_is_refused(self):
        with self.assertRaises(ValueError):
            should_run("30 4 * * 1", at("2026-09-30T04:30:00"))


if __name__ == "__main__":
    unittest.main()
