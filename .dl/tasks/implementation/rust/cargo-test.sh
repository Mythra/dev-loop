#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"
ensureRustToolchainConfig
cargo --color always test --target x86_64-unknown-linux-musl
