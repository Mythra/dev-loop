#!/usr/bin/env bash

if [[ "$@" != "hello world" ]]; then
  echo "ARGS: [$@] != [hello world]"
  exit 10
fi