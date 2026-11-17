import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_names  # noqa: E402


class TestNames(unittest.TestCase):
    def test_only_test_lines_are_names(self):
        output = (
            "tests::a_fact: test\n"
            "benches::speed: bench\n"
            "mod::with: colon: test\n"
            "\n"
            "2 tests, 1 benchmark\n"
        )
        self.assertEqual(test_names.parse_list(output), ["tests::a_fact", "mod::with: colon"])

    def test_a_list_round_trips_with_its_system(self):
        names = {"core lib:core tests::b", "core lib:core tests::a"}
        text = test_names.render("darwin", names)
        self.assertTrue(text.startswith("# "))
        self.assertEqual(text.splitlines()[-2:], ["core lib:core tests::a", "core lib:core tests::b"])
        self.assertEqual(test_names.parse_file(text), ("darwin", names))

    def test_a_gone_name_is_reported_and_a_new_one_apart(self):
        gone, new = test_names.compare({"a", "b"}, {"b", "c"})
        self.assertEqual((gone, new), (["a"], ["c"]))

    def test_the_committed_list_names_its_system(self):
        os_name, names = test_names.parse_file(test_names.LIST.read_text(encoding="utf-8"))
        self.assertIsNotNone(os_name)
        self.assertGreater(len(names), 1000)


if __name__ == "__main__":
    unittest.main()
