#!/usr/bin/env bash

export LDFLAGS="-L/usr/local/opt/openssl/lib"
export CPPFLAGS="-I/usr/local/opt/openssl/include"
export CFLAGS="-I/usr/local/opt/openssl/include"

ensureBinaryExists "cargo"

if [[ "$1" != "release" ]]; then
  cargo build --no-default-features --features "static-ssl"
else
  cargo build --release --no-default-features --features "static-ssl"
fi
