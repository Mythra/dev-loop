#!/usr/bin/env bash

set -e

if ! hash nc >/dev/null 2>&1 ; then
  echo "Please install netcat!"
  exit 10
fi

nc -vz localhost 25565
nc -vz -u localhost 25566
