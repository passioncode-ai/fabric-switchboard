#!/bin/sh
# Runs the workspace's Rust tests on Linux in Docker (SB-88), as an ordinary user (root ignores the
# file permissions several tests rely on), with a D-Bus session and an unlocked GNOME Keyring so
# the Secret Service vault is exercised for real. Usage: scripts/linux/check.sh [cargo test args]
set -eu
cd "$(dirname "$0")/../.."
image=fabric-switchboard-linux-check
docker build -q -t "$image" scripts/linux >/dev/null
docker volume create sb-linux-target >/dev/null
docker volume create sb-linux-cargo >/dev/null
docker run --rm --user 0 -v sb-linux-target:/target -v sb-linux-cargo:/usr/local/cargo/registry "$image" \
  chown -R 1000:1000 /target /usr/local/cargo/registry
exec docker run --rm -v "$PWD":/src -v sb-linux-target:/target -v sb-linux-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/target -e HOME=/home/builder -e SWITCHBOARD_SECRET_SERVICE_TEST=1 "$image" \
  dbus-run-session -- sh -c 'printf "%s" synthetic-unlock | gnome-keyring-daemon --unlock --components=secrets >/dev/null && cargo test --locked --no-fail-fast "$@"' sh "$@"
