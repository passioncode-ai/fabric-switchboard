#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
# Each step prints its wall time (`step <name> <seconds>s`), so a slow gate on a loaded build
# machine says which step took the time (plan 2026-11-17, step A1). A failed step prints its time
# and its exit status, then the gate stops.
gate_started=$(date +%s)
step() {
  step_name=$1
  shift
  step_started=$(date +%s)
  if "$@"; then
    echo "step $step_name $(($(date +%s) - step_started))s"
  else
    step_status=$?
    echo "step $step_name $(($(date +%s) - step_started))s failed ($step_status)"
    return "$step_status"
  fi
}
# Windows gives a process's main thread 1 MiB of stack; macOS and Linux give 8. The MCP tests run
# under that limit, so a debug frame that outgrows it fails here and not only in the nightly
# Windows run (0.6.60, nightly 39676565992).
mcp_on_a_small_stack() {
  (ulimit -s 1024 && cargo test -p switchboard-cli --test mcp --locked)
}
# SB-397 D-2: the sessions core never depends on a Switchboard crate, so the service runs on a
# machine without Switchboard.
core_tree() {
  core_tree=$(cargo tree -q -p fabric-sessions-core --locked --edges normal,build --prefix none) || return 1
  if printf '%s\n' "$core_tree" | grep -q '^switchboard-'; then
    echo "fabric-sessions-core depends on a switchboard crate (SB-397 D-2)" >&2
    return 1
  fi
}
# Code that builds only on Unix reached a release tag and failed the nightly Windows jobs (0.7.31,
# SB-303). With cargo-xwin installed the crates are checked for Windows here, their tests too (the
# proxy's tests did not build on Windows until SB-406 item 7); without it the step says it was
# skipped. The desktop crate also needs `llvm-rc` for its Windows resources (Homebrew's `llvm`,
# docs/DISTRIBUTION.md); without it that crate alone is skipped, and the nightly Windows jobs
# still build and test it. Its Windows configuration bundles the release CLI, which a check does
# not build: TAURI_CONFIG takes the resources out for it. A subshell keeps the PATH change here.
windows_check() (
  if ! command -v cargo-xwin >/dev/null 2>&1; then
    echo "Windows check skipped: cargo-xwin is not installed (docs/DISTRIBUTION.md)."
    exit 0
  fi
  if [ -d /opt/homebrew/opt/llvm/bin ]; then
    PATH="/opt/homebrew/opt/llvm/bin:$PATH"
  fi
  # `ring`'s build needs `llvm-lib`: on PATH, or rustup's `llvm-tools` (its `llvm-ar`), which
  # cargo-xwin finds by itself. Without either the check could only fail, so it is skipped.
  if ! command -v llvm-lib >/dev/null 2>&1; then
    sysroot=$(rustc --print sysroot 2>/dev/null) || sysroot=""
    found=""
    for tool in "$sysroot"/lib/rustlib/*/bin/llvm-lib "$sysroot"/lib/rustlib/*/bin/llvm-ar; do
      if [ -n "$sysroot" ] && [ -x "$tool" ]; then
        found=yes
      fi
    done
    if [ -z "$found" ]; then
      echo "Windows check skipped: llvm-lib is not installed (rustup component add llvm-tools, or Homebrew's llvm; docs/DISTRIBUTION.md)."
      exit 0
    fi
  fi
  desktop="-p fabric-switchboard"
  if ! command -v llvm-rc >/dev/null 2>&1; then
    desktop=""
    echo "Windows check of the desktop crate skipped: llvm-rc is not installed (docs/DISTRIBUTION.md)."
  fi
  # shellcheck disable=SC2086 # $desktop is one option and its value, or nothing.
  TAURI_CONFIG='{"bundle":{"resources":null}}' cargo xwin check -q -p switchboard-core -p switchboard-proxy -p switchboard-runtime -p switchboard-cli $desktop -p fabric-sessions-core -p fabric-sessions --target x86_64-pc-windows-msvc --all-targets --locked
)
step brand node scripts/check-brand.mjs
step ui-build npm run build
step read-deadline node scripts/test-read-deadline.mjs
step ui-logic node scripts/test-ui-logic.mjs
step bugs node scripts/test-bugs.mjs
step locale node scripts/check-locale.mjs
step strings-registry-tests node scripts/test-strings-registry.mjs
step strings-registry node scripts/check-strings-registry.mjs
step error-vocabulary node scripts/check-error-vocabulary.mjs
step fmt cargo fmt --all -- --check
step cargo-test cargo test --workspace --locked
# A test that existed must not disappear unseen: reuses the binaries `cargo test` built.
step test-inventory python3 scripts/test_names.py --check
step mcp-small-stack mcp_on_a_small_stack
step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
step core-tree core_tree
step windows windows_check
step docs python3 scripts/check_docs.py
step docs-tests python3 -m unittest scripts/test_check_docs.py
step ux-lint python3 docs/ux/lint.py
step ux-lint-tests python3 -m unittest scripts/test_ux_lint.py
step plugin python3 scripts/check_plugin.py
step plugin-tests python3 -m unittest scripts/test_check_plugin.py
step nightly-clock-tests python3 -m unittest scripts/test_nightly_clock.py
step prune-tests python3 -m unittest scripts/test_prune_artifacts.py
step quota-sentry-tests python3 -m unittest scripts/test_quota_sentry.py scripts/test_quota_monitor.py
step test-inventory-tests python3 -m unittest scripts/test_test_names.py
step packaging-tests python3 -m unittest scripts/test_updater_artifacts.py scripts/test_package_linux.py
step release-tests python3 -m unittest scripts/test_build_macos.py scripts/test_package_windows.py scripts/test_windows_bundle.py scripts/test_release_preflight.py scripts/test_release_bump.py scripts/test_smoke_native.py
step agents-doc python3 scripts/agents_doc.py --check
step notices python3 scripts/third_party_notices.py --check
step diff-check git diff --check
echo "step total $(($(date +%s) - gate_started))s"
