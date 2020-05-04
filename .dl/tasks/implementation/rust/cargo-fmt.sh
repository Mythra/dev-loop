#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"

echo "Running Cargo Fmt..."
cargo fmt --all
echo "Ran."