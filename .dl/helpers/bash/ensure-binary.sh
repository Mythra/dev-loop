#!/usr/bin/env bash

source ".dl/helpers/bash/output.sh"

# checkIfBinaryExists(binary: String) -> (0 || 1)
#
# determines if a particular binary exists, and returns a 0 code if it does
# or a 1 code if it does not.
#
# if you're looking to hard fail if a binary doesn't exist please use
# `ensureBinaryExists`
checkIfBinaryExists() {
  if hash "$1" >/dev/null 2>&1 ; then
    return 0
  else
    return 1
  fi
}

# ensureBinaryExists(binary: String) -> (0 || panic)
#
# ensure a particular binary exists, exiting if it does not.
ensureBinaryExists() {
  local readonly bin="$1"

  if ! checkIfBinaryExists "$bin" ; then
    die "The binary [$bin] is not installed. Please install it to use this task."
  fi
}