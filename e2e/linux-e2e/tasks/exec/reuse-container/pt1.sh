#!/usr/bin/env bash

# Write to a path that doesn't get sync'd. You probably
# don't want to ever do this, even for a cache because it
# makes it very hard to actually debug. It's not something
# you can just look at.
#
# This allows us to confirm we're using the same container
# but ideally you would never do this.

mkdir -p /opt/reuse-container/
echo "data" > /opt/reuse-container/non-mounted
