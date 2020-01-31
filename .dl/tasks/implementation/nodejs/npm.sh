#!/usr/bin/env bash

set -e

cd "$1"
shift

ensureBinaryExists "npm"

npm $@
