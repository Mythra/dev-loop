#!/usr/bin/env bash

set -e

echo "===> Moving: [$1] to [$2]"
mv -f "$1" "$2" >/dev/null 2>&1 || {
  sudo -n mv -f "$1" "$2"
}
echo "===> Done"
