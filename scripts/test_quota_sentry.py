"""The quota sentry's forecast and once-only delivery, on synthetic accounts."""
import importlib.util
import json
import pathlib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "quota_sentry", pathlib.Path(__file__).with_name("ops") / "quota_sentry.py"
)
sentry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sentry)

NOW = 1_795_000_000
HOUR = 3600


def account(label, pool, used, reset_in_hours, **extra):
    return {
        "label": label, "pool": pool, "provider": "claude", "kind": "oauth", "enabled": True,
        "usage": {"windows": [{"name": "seven_day", "used_percent": used, "resets_at": NOW + reset_in_hours * HOUR}]},
        **extra,
    }


class Forecast(unittest.TestCase):
    def test_a_group_with_room_holds_the_horizon(self):
        units = sentry.as_units([account("a", "support", 10, 100)], NOW)
        self.assertEqual(sentry.forecast(units, {"support": 100.0}, {}, NOW), {"support": None})

    def test_a_spent_group_outside_the_feed_is_out_now_until_its_reset(self):
        units = sentry.as_units([account("a", "support", 100, 30), account("d", "default", 0, 100)], NOW)
        self.assertEqual(sentry.forecast(units, {"support": 100.0}, {}, NOW)["support"], 0)

    def test_a_fed_group_draws_on_the_default_group(self):
        units = sentry.as_units([account("a", "support", 100, 200), account("d", "default", 0, 100)], NOW)
        self.assertIsNone(sentry.forecast(units, {"support": 100.0}, {"support": 1}, NOW)["support"])

    def test_the_ordinary_cli_account_and_an_assigned_one_are_never_lent(self):
        units = sentry.as_units(
            [account("a", "support", 100, 200), account("cli", "default", 0, 100),
             account("given", "default", 0, 100, assigned_to="Frame Agent")], NOW)
        self.assertEqual(sentry.forecast(units, {"support": 100.0}, {"support": 1}, NOW, cli_account="cli")["support"], 0)

    def test_a_higher_priority_group_is_served_first_from_the_reserve(self):
        accounts = [account("s", "support", 100, 200), account("h", "heavy", 100, 200),
                    account("d", "default", 90, 200)]  # 200 dollars left in the reserve
        hours = sentry.forecast(sentry.as_units(accounts, NOW), {"support": 48.0, "heavy": 2400.0},
                                {"support": 1, "heavy": 2}, NOW)
        # Hour 0 serves both (2 and 100 dollars), hour 1 leaves heavy 4 short (under half, not
        # counted), hour 2 finds the reserve empty: the heavy group is out no later than support.
        self.assertEqual(hours["heavy"], 2)
        self.assertGreaterEqual(hours["support"], hours["heavy"])

    def test_the_runway_is_counted_in_hours_and_a_reset_refills(self):
        # 5 % of 2000 is 100 dollars; at 240 a day (10 an hour) it lasts 10 hours.
        units = sentry.as_units([account("a", "support", 95, 500)], NOW)
        self.assertEqual(sentry.forecast(units, {"support": 240.0}, {}, NOW)["support"], 10)
        units = sentry.as_units([account("a", "support", 95, 5)], NOW)
        self.assertIsNone(sentry.forecast(units, {"support": 240.0}, {}, NOW)["support"])

    def test_the_five_hour_window_caps_one_account(self):
        # 200 dollars per five hours is 960 a day at most from one account.
        units = sentry.as_units([account("a", "support", 0, 500)], NOW)
        self.assertIsNotNone(sentry.forecast(units, {"support": 2400.0}, {}, NOW)["support"])

    def test_an_account_off_or_on_another_machine_is_not_counted(self):
        accounts = [account("a", "support", 0, 100, enabled=False), account("b", "support", 0, 100, elsewhere=True)]
        self.assertEqual(sentry.as_units(accounts, NOW), [])

    def test_a_remote_space_spends_at_its_windows_pace(self):
        # 60 % in 2 of 7 days is 600 dollars a day; 800 dollars left last about 32 hours,
        # and the window resets only after 120.
        runway, pace = sentry.machine_runway([account("a", "default", 60, 120)], NOW)
        self.assertAlmostEqual(pace, 600.0, delta=1.0)
        self.assertTrue(30 <= runway <= 34)


class Delivery(unittest.TestCase):
    REPORT = {"warn_hours": 12, "degraded": [], "rows": [
        {"unit": "group:support", "agents": ["bot"], "runway_hours": 5},
        {"unit": "group:calm", "agents": ["pm"], "runway_hours": None},
        {"unit": "machine:far", "agents": ["x"], "runway_hours": "unknown"},
    ]}

    def test_a_short_runway_is_announced_once_then_after_the_repeat(self):
        alerts, _, state = sentry.due(self.REPORT, {}, NOW)
        self.assertEqual([r["unit"] for r in alerts], ["group:support"])
        self.assertEqual(sentry.due(self.REPORT, state, NOW + HOUR)[0], [])
        again = sentry.due(self.REPORT, state, NOW + sentry.REPEAT_HOURS * HOUR)[0]
        self.assertEqual(len(again), 1)

    def test_recovery_is_said_once_and_unknown_is_neither(self):
        state = {"warned": {"group:calm": NOW - HOUR, "machine:far": NOW - HOUR}}
        alerts, recovered, new = sentry.due(self.REPORT, state, NOW)
        self.assertEqual(recovered, ["group:calm"])
        self.assertIn("machine:far", new["warned"])
        self.assertEqual(sentry.due(self.REPORT, new, NOW + HOUR)[1], [])

    def test_the_message_has_no_em_dash_and_names_the_agents(self):
        text = sentry.message([dict(self.REPORT["rows"][0], own_accounts=2, own_with_room=0, spend_per_day=90)],
                              ["group:calm"], dict(self.REPORT, degraded=["x did not answer"]))
        self.assertNotIn(chr(0x2014), text)
        self.assertIn("bot", text)
        self.assertIn("через 5 ч", text)


class Spend(unittest.TestCase):
    def test_spend_reads_today_and_yesterday_and_scales_to_a_day(self):
        with tempfile.TemporaryDirectory() as folder:
            path = pathlib.Path(folder) / "statistics.json"
            now = 1_795_267_200  # 2026-11-21T13:20:00Z
            path.write_text(json.dumps({"tallies": {
                "2026-11-21\tacc\tsupport\tclaude-code\trepo": {"cost_micros": 37_333_333},
                "2026-11-20\tacc\tsupport\tclaude-code": {"cost_micros": 0},
                "2026-11-19\tacc\tsupport\tclaude-code": {"cost_micros": 9_000_000_000},
            }}))
            spend = sentry.spend_last_day(now, path)
            self.assertAlmostEqual(spend["support"], 24.0, delta=0.1)
            self.assertIsNone(sentry.spend_last_day(now, path.with_name("absent.json")))


if __name__ == "__main__":
    unittest.main()
