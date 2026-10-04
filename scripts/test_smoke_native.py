"""smoke_native: an ordinary start must show the window, a background start must keep it hidden
and ignore an argument it does not know (SB-30). A fake desktop stands in for the app."""
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from smoke_native import verify  # noqa: E402

FAKE = """#!/usr/bin/env python3
import sys
args = sys.argv[1:]
assert args[0] == '--smoke-test'
hidden = '--background' in args
state = sys.argv[0].rsplit('-', 1)[-1]  # 'honest' or 'liar'
if state == 'liar':
    hidden = not hidden
print('SWITCHBOARD_WINDOW ' + ('hidden' if hidden else 'visible'))
print('SWITCHBOARD_FRONTEND_READY 9.9.9')
"""


class SmokeNativeTest(unittest.TestCase):
    def fake(self, folder, name):
        path = Path(folder) / f'desktop-{name}'
        path.write_text(FAKE)
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def test_window_state_matches_the_launch(self):
        with tempfile.TemporaryDirectory() as folder:
            honest = self.fake(folder, 'honest')
            self.assertEqual(verify(honest, '9.9.9')['window'], 'visible')
            background = verify(honest, '9.9.9', background=True)
            self.assertEqual((background['launch'], background['window']), ('background', 'hidden'))

    def test_a_window_in_the_wrong_state_fails(self):
        with tempfile.TemporaryDirectory() as folder:
            liar = self.fake(folder, 'liar')
            with self.assertRaises(RuntimeError):
                verify(liar, '9.9.9')
            with self.assertRaises(RuntimeError):
                verify(liar, '9.9.9', background=True)


if __name__ == '__main__':
    unittest.main()
