#!/bin/bash

# This script checks if the version of the macros crate has been published to crates.io.
# It is used in the CI pipeline to ensure that the version of the macros crate is published
# before publishing the version of the lib crate that depends on it.

set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
cd ${DIR}/../../crates/opentelemetry-macros

CRATE_ID=pyo3-opentelemetry-macros
VERSION=$(yq -r -oj .package.version Cargo.toml)

VERSION_DATA=$(curl -vsSL https://crates.io/api/v1/crates/${CRATE_ID}/${VERSION} | yq -p json .version)

if [ "${VERSION_DATA}" == "null" ]; then
  echo "Version ${VERSION} not yet published"
  exit 1
else
  exit 0
fi
