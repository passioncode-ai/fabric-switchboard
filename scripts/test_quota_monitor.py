"""The quota monitor's detector, reset plan and distribution, on synthetic accounts."""
import importlib.util
import pathlib
import unittest

SPEC = importlib.util.spec_from_file_location(
    "quota_monitor", pathlib.Path(__file__).with_name("ops") / "quota_monitor.py"
)
monitor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(monitor)

NOW = 1_795_000_000
HOUR = 3600


def acc(label, pool, week, five=0, where="here", age=60, week_reset=100, five_reset=3, **extra):
    r = {"where": where, "id": label, "label": label, "pool": pool, "provider": "claude", "kind": "oauth",
         "enabled": True, "elsewhere": False, "assigned": False, "observed_at": NOW - age,
         "source": "claude_oauth", "health": "ok",
         "windows": {"five_hour": {"used": five, "reset": NOW + five_reset * HOUR},
                     "seven_day": {"used": week, "reset": NOW + week_reset * HOUR}}}
    r.update(extra)
    return r


class Stuck(unittest.TestCase):
    def test_a_fresh_account_is_not_stuck(self):
        self.assertEqual(monitor.stuck(acc("a", "g", 10), NOW), [])

    def test_a_reset_that_passed_after_the_last_look_is_stuck_and_reads_empty(self):
        r = acc("a", "g", 100, age=2 * HOUR, week_reset=-1)
        self.assertIn("reset_passed", monitor.stuck(r, NOW))
        self.assertEqual(monitor.effective(r, NOW)["seven_day"], 0.0)
        self.assertTrue(monitor.has_room(r, NOW))

    def test_overdue_is_later_here_than_there(self):
        self.assertEqual(monitor.stuck(acc("a", "g", 10, age=1500), NOW), [])
        self.assertEqual(monitor.stuck(acc("a", "g", 10, age=1500, where="far"), NOW), ["overdue"])

    def test_a_spent_week_is_not_overdue_until_its_reset(self):
        self.assertEqual(monitor.stuck(acc("a", "g", 100, age=5 * HOUR, five_reset=-1), NOW), ["reset_passed"])
        spent = acc("a", "g", 100, age=5 * HOUR)
        spent["windows"].pop("five_hour")
        self.assertEqual(monitor.stuck(spent, NOW), [])

    def test_a_refused_check_needs_a_person_and_never_checked_is_named(self):
        self.assertEqual(monitor.stuck(acc("a", "g", 10, health="refused"), NOW), ["sign_in"])
        self.assertEqual(monitor.stuck(acc("a", "g", 10, observed_at=None), NOW), ["never_checked"])

    def test_an_api_key_is_never_stuck(self):
        self.assertEqual(monitor.stuck(acc("k", "tokens", 0, kind="api_key", observed_at=None), NOW), [])


class Resets(unittest.TestCase):
    def test_accounts_without_room_come_back_soonest_first(self):
        rows = [acc("week", "g", 100, week_reset=30), acc("five", "g", 20, five=100, five_reset=2), acc("ok", "g", 5)]
        plan = monitor.resets(rows, NOW)
        self.assertEqual([p["label"] for p in plan], ["five", "week"])
        self.assertEqual(plan[0]["window"], "five_hour")


class Distribute(unittest.TestCase):
    TIERS = {"release": 1, "support": 2, "rest": 3}

    def test_a_duty_group_with_room_gets_nothing(self):
        rows = [acc("r", "release", 10), acc("d", "default", 0)]
        self.assertEqual(monitor.distribute(rows, self.TIERS, [], NOW, {}, None), [])

    def test_the_release_group_is_served_before_support_from_the_reserve(self):
        rows = [acc("r", "release", 100), acc("s", "support", 100), acc("d", "default", 0)]
        moves = monitor.distribute(rows, self.TIERS, [], NOW, {}, None)
        self.assertEqual([(m["to"], m["label"]) for m in moves], [("release", "d")])

    def test_with_the_reserve_empty_a_tier_three_group_gives(self):
        rows = [acc("s", "support", 20, five=100), acc("x", "rest", 30), acc("d", "default", 100)]
        moves = monitor.distribute(rows, self.TIERS, [], NOW, {}, None)
        self.assertEqual([(m["to"], m["label"], m["from"]) for m in moves], [("support", "x", "rest")])

    def test_the_ordinary_sign_in_and_an_assigned_account_are_never_moved(self):
        rows = [acc("r", "release", 100), acc("cli", "default", 0), acc("given", "default", 0, assigned=True)]
        self.assertEqual(monitor.distribute(rows, self.TIERS, [], NOW, {}, "cli"), [])

    def test_a_group_is_not_fed_twice_within_a_quarter_hour(self):
        rows = [acc("r", "release", 100), acc("d", "default", 0)]
        state = {"moved": {"group:release": NOW - 600}}
        self.assertEqual(monitor.distribute(rows, self.TIERS, [], NOW, state, None), [])

    def test_a_machine_with_nothing_and_no_reset_soon_is_sent_one(self):
        rows = [acc("f", "default", 100, where="far", week_reset=40), acc("d", "default", 10)]
        moves = monitor.distribute(rows, self.TIERS, ["far"], NOW, {}, None)
        self.assertEqual([(m["act"], m["to"], m["label"]) for m in moves], [("send", "far", "d")])
        soon = [acc("f", "default", 30, five=100, where="far", five_reset=0.5), acc("d", "default", 10)]
        self.assertEqual(monitor.distribute(soon, self.TIERS, ["far"], NOW, {}, None), [])
        poor = [acc("f", "default", 100, where="far", week_reset=40), acc("d", "default", 70)]
        self.assertEqual(monitor.distribute(poor, self.TIERS, ["far"], NOW, {}, None), [])


class Freshness(unittest.TestCase):
    def test_the_floor_follows_the_budget(self):
        rows = [acc(str(i), "g", 10, age=30 + i) for i in range(40)] + [acc("h", "g", 10, age=5, source="response_headers")]
        f = monitor.freshness(rows, NOW)["here"]
        self.assertEqual((f["accounts"], f["checked_by_usage_endpoint"], f["floor_minutes_for_those"]), (41, 40, 20.0))


if __name__ == "__main__":
    unittest.main()
