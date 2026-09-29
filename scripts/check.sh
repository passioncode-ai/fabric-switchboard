#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
node scripts/check-brand.mjs
npm run build
node scripts/test-read-deadline.mjs
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/check_docs.py
python3 scripts/third_party_notices.py --check
git diff --check
