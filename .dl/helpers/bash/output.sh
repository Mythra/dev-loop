#!/usr/bin/env bash

die() {
  echo "ERROR: $1" >&2
  shift

  while [[ -n $1 ]]; do
    echo "  $1" >&2
    shift
  done

  exit 1
}