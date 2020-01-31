#!/usr/bin/env bash

mkdir -p build/run/
sleep "$1"
echo "$2" >> build/run/state
