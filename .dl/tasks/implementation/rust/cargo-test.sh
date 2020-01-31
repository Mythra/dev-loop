#!/usr/bin/env bash

set -e

cargo --color always test --target x86_64-unknown-linux-musl
