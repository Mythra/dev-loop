#!/usr/bin/env bash

set -e

if ! hash socat >/dev/null 2>&1 ; then
  apt-get update
  apt-get install -y socat
fi

socat -v tcp-l:25565,fork exec:'/bin/cat' &
socat -v udp-l:25566,fork exec:'/bin/cat' &
