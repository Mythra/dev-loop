#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"
ensureRustToolchainConfig

echo "Linting with cargo fmt..."
cargo fmt --all -- --check
echo "Linting with cargo clippy..."
cargo clippy -- -W clippy::pedantic
echo "Done."