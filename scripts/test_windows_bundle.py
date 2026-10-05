"""SB-05: the Windows installer embeds switchboard.exe. The resource in the Windows config overlay
must name the CLI the release workflow and the cross build actually produce, and both must
build the CLI before they bundle."""
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLI = 'target/x86_64-pc-windows-msvc/release/switchboard.exe'


class WindowsBundleTest(unittest.TestCase):
    def test_the_overlay_embeds_the_built_cli_beside_the_app(self):
        overlay = json.loads((ROOT / 'src-tauri/tauri.windows.conf.json').read_text(encoding='utf-8'))
        resources = overlay['bundle']['resources']
        self.assertEqual(resources, {'../' + CLI: 'switchboard.exe'})

    def test_the_release_workflow_builds_the_cli_before_bundling_and_checks_the_install(self):
        workflow = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
        built = workflow.index('cargo build --release --locked -p switchboard-cli --target x86_64-pc-windows-msvc')
        bundled = workflow.index('tauri bundle -- --bundles nsis')
        self.assertLess(built, bundled)
        check = workflow[bundled:]
        self.assertIn("'fabric-switchboard.exe', 'switchboard.exe'", check)
        self.assertIn('/S', check)

    def test_the_cross_build_builds_the_cli_first(self):
        script = (ROOT / 'scripts/build_windows_cross.py').read_text(encoding='utf-8')
        cli = script.index("'-p', 'switchboard-cli'")
        bundle = script.index("'--bundles', 'nsis'")
        self.assertLess(cli, bundle)
        self.assertTrue(re.search(r"base = ROOT / 'target' / TARGET / 'release'", script))


if __name__ == '__main__':
    unittest.main()
