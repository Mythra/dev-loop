#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"

rustup component add rustfmt --toolchain stable-2020-01-30-x86_64-unknown-linux-gnu

echo "Running Cargo Fmt..."
cargo fmt --all
echo "Ran."