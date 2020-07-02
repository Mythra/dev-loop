#!/usr/bin/env bash

data=$(< ./build/nested-pipelines/state)
echo -n "${data}o " > ./build/nested-pipelines/state
