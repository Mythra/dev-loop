#!/usr/bin/env bash

ensureRustToolchainConfig() {
  mkdir -p $HOME/.cargo
  sudo -n chown "$(whoami)" -R ~/.cargo/
  echo "[build]
# Target musl-libc by default when running Cargo.
target = \"x86_64-unknown-linux-musl\"

[target.armv7-unknown-linux-musleabihf]
linker = \"arm-linux-gnueabihf-gcc\"" > $HOME/.cargo/config
}