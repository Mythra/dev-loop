#!/usr/bin/env bash

data=$(< ./build/nested-pipelines/state)
echo -n "${data}world" > ./build/nested-pipelines/state
