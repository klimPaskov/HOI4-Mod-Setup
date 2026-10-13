#!/usr/bin/env bash
set -euo pipefail

# Explicit baselines keep release compatibility tied to native runner evidence.
source /etc/os-release
case "${ID}:${VERSION_ID}" in
  ubuntu:22.04) fuse_package=libfuse2 ;;
  ubuntu:24.04) fuse_package=libfuse2t64 ;;
  *) echo "Linux native build dependencies support Ubuntu 22.04/24.04 runners" >&2; exit 1 ;;
esac
sudo apt-get update
sudo apt-get install --no-install-recommends -y \
  build-essential pkg-config libayatana-appindicator3-dev libatk1.0-dev \
  libcairo2-dev libgdk-pixbuf-2.0-dev libglib2.0-dev libgtk-3-dev \
  libpango1.0-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev \
  libsecret-1-dev libdbus-1-dev patchelf "$fuse_package" xvfb xauth \
  dbus-x11 gnome-keyring libsecret-tools
