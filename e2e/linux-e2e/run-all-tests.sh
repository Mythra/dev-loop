#!/usr/bin/env bash

set -e

SCRIPT_DIR=$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )

cd "$SCRIPT_DIR"

DL_COMMAND="${DL_COMMAND:-"dl"}"
if [[ "x$DL_COMMAND" == "x" ]]; then
  echo "FAILED to find DL Command!"
fi

rm -rf ./build/
$DL_COMMAND exec exec-pipeline

# Run inside another directory.
if [[ "$DL_COMMAND" =~ "../" ]]; then
  (cd tasks && ../$DL_COMMAND exec exec-pipeline)
else
  (cd tasks && $DL_COMMAND exec exec-pipeline)
fi

CACHED_DL_WORKER_COUNT="$DL_WORKER_COUNT"

# Run with parallelism
export DL_WORKER_COUNT=3
$DL_COMMAND exec parallel-pipeline
ppipeline_multi_data=$(< ./build/ppipeline/echo-nums)
if [[ "$ppipeline_multi_data" != "1
5
7
10" ]]; then
  echo "Data: [$ppipeline_multi_data] is not: [1
5
7
10]"
  exit 1
fi

rm -f ./build/ppipeline/echo-nums

# Run without parallelism
export DL_WORKER_COUNT=1
$DL_COMMAND exec parallel-pipeline

if [[ "x$CACHED_DL_WORKER_COUNT" == "x" ]]; then
  unset DL_WORKER_COUNT
else
  export DL_WORKER_COUNT="$CACHED_DL_WORKER_COUNT"
fi
ppipeline_single_data=$(< ./build/ppipeline/echo-nums)
if [[ "$ppipeline_single_data" != "1
7
5
10" ]]; then
  echo "Data: [$ppipeline_single_data] is not: [1
7
5
10]"
  exit 2
fi

$DL_COMMAND run run

data=$(< ./build/run/state)

if [[ "$data" != "1
1
2
2
3
3" ]]; then
  echo "Data: [$data] from run is not: [1
1
2
2
3
3]"
  exit 3
fi