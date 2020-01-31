#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"

rustup component add rustfmt --toolchain stable-2020-01-30-x86_64-unknown-linux-gnu
rustup component add clippy --toolchain stable-2020-01-30-x86_64-unknown-linux-gnu

echo "Linting with cargo fmt..."
cargo fmt --all -- --check
echo "Linting with cargo clippy..."
cargo clippy -- -W clippy::pedantic
echo "Done."