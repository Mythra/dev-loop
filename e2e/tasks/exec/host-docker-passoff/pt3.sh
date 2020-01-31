#!/usr/bin/env bash

set -e

if [[ ! -f "./build/lol-mnt/host-docker-passoff/state" ]]; then
  echo "Host -> Docker -> [Host] Passoff NO FILE EXISTS!"
  exit 10
fi

data=$(< ./build/lol-mnt/host-docker-passoff/state)
if [[ "$data" != "hello world" ]]; then
  echo "DATA: [$data] IS NOT EQUAL TO [hello world]!"
  exit 11
fi
