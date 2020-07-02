#!/usr/bin/env bash

data=$(< ./build/nested-pipelines/state)
if [[ "$data" != "hello world" ]]; then
  echo "Data from pipelines are: [$data] which is not: [hello world]"
  exit 10
fi
