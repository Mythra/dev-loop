#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"
ensureRustToolchainConfig

echo "Running Cargo Fmt..."
cargo fmt --all
echo "Ran."