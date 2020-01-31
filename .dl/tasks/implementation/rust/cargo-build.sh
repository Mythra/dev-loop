#!/usr/bin/env bash

set -e

ensureBinaryExists "cargo"

if [[ "$1" != "release" ]]; then
  echo "Building Project"
  cargo --color always build --target x86_64-unknown-linux-musl
else
  echo "Building Project in Release"
  cargo --color always build --release --target x86_64-unknown-linux-musl
fi
