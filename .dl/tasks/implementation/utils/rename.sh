#!/usr/bin/env bash

set -e

echo "===> Moving: [$1] to [$2]"
mv -f "$1" "$2"
echo "===> Done"
