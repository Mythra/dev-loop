#!/usr/bin/env bash

data=$(< ./build/nested-pipelines/state)
echo -n "${data}e" > ./build/nested-pipelines/state
