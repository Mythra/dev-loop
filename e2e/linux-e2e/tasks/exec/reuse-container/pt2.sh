#!/usr/bin/env bash

data=$(< /opt/reuse-container/non-mounted)
if [[ "$data" != "data" ]]; then
  echo "DATA READ is: [$data] which is not: [data]"
  exit 10
fi
