#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
node scripts/check-brand.mjs
npm run build
node scripts/test-read-deadline.mjs
node scripts/test-ui-logic.mjs
node scripts/check-locale.mjs
node scripts/check-error-vocabulary.mjs
cargo fmt --all -- --check
cargo test --workspace --locked
# Windows gives a process's main thread 1 MiB of stack; macOS and Linux give 8. The MCP tests run
# under that limit, so a debug frame that outgrows it fails here and not only in the nightly
# Windows run (0.6.60, nightly 39676565992).
(ulimit -s 1024 && cargo test -p switchboard-cli --test mcp --locked)
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/check_docs.py
python3 scripts/check_plugin.py
python3 -m unittest scripts/test_check_plugin.py
python3 -m unittest scripts/test_nightly_clock.py
python3 -m unittest scripts/test_prune_artifacts.py
python3 -m unittest scripts/test_updater_artifacts.py scripts/test_package_linux.py
python3 -m unittest scripts/test_build_macos.py scripts/test_package_windows.py scripts/test_windows_bundle.py scripts/test_release_preflight.py scripts/test_smoke_native.py
python3 scripts/agents_doc.py --check
python3 scripts/third_party_notices.py --check
git diff --check
