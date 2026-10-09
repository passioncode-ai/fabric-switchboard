# The Linux check environment (SB-88): the Rust crates' build dependencies, a D-Bus session
# and GNOME Keyring for the Secret Service vault test, and the WebKitGTK stack the desktop app
# builds against. `scripts/linux/check.sh` runs the workspace tests in it as an ordinary user.
FROM rust:1.92-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libdbus-1-dev dbus dbus-x11 gnome-keyring libsecret-tools \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
      libssl-dev file xdg-utils xvfb xauth python3 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt
RUN useradd --create-home --uid 1000 builder
USER builder
WORKDIR /src
