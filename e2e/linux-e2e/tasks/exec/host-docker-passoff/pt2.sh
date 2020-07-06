#!/usr/bin/env bash

set -e

if [[ ! -f "./build/lol-mnt/host-docker-passoff/state" ]]; then
  echo "Host -> [Docker] -> Host Passoff NO FILE EXISTS!"
  exit 10
fi

# Check it also mounted in at the correct location
if [[ ! -f "/mnt/lol-mnt/host-docker-passoff/state" ]]; then
  echo "Host -> [Docker] -> Host Passoff NO FILE EXISTS on FIRST MOUNT!"
  exit 11
fi

# Check the other mount also worked
if [[ ! -f "/mnt/build/lol-mnt/host-docker-passoff/state" ]]; then
  echo "Host -> [Docker] -> Host Passoff NO FILE EXISTS on SECOND MOUNT!"
  exit 12
fi

data=$(< ./build/lol-mnt/host-docker-passoff/state)
echo "${data}world" > ./build/lol-mnt/host-docker-passoff/state
