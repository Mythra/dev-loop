#!/usr/bin/env bash

data=$(< ./build/nested-pipelines/state)
echo -n "${data}ll" > ./build/nested-pipelines/state
