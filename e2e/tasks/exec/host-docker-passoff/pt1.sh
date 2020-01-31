#!/usr/bin/env bash

set -e

if [[ ! -d "./build/lol-mnt/host-docker-passoff/" ]]; then
  mkdir ./build/lol-mnt/host-docker-passoff/
fi
echo "hello " > ./build/lol-mnt/host-docker-passoff/state
