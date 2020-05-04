#!/usr/bin/env bash

set -e

if [[ "x$DL_SPACES_BUILD_KEY" == "x" ]] || [[ "x$DL_SPACES_BUILD_SECRET" == "x" ]]; then
  echo "Cannot find Digital Ocean Credentials in Environment: [DL_SPACES_BUILD_KEY/DL_SPACES_BUILD_SECRET]."
  exit 1
fi

ensureBinaryExists "s3cmd"

if [[ ! -f "$1" ]]; then
  echo "Could not find file to upload: [$1]"
  exit 2
fi
if [[ "x$2" == "x" ]]; then
  echo "Couldn't find location in bucket to upload file too: [$2]"
  exit 3
fi

s3cmd \
  --access_key="$DL_SPACES_BUILD_KEY" --secret_key="$DL_SPACES_BUILD_SECRET" \
  --ssl --acl-public --force --host="sfo2.digitaloceanspaces.com" --host-bucket="%(bucket)s.sfo2.digitaloceanspaces.com" \
  put "$1" "s3://dev-loop-builds/$2"