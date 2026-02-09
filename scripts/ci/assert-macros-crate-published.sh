#!/bin/bash

# This script checks if the version of the macros crate has been published to crates.io.
# It is used in the CI pipeline to ensure that the version of the macros crate is published
# before publishing the version of the lib crate that depends on it.

CRATE="pyo3-opentelemetry-macros"

if cargo publish --dry-run -p "$CRATE" 2>&1 | tee /dev/stderr \
  | grep -Eq "warning: crate ${CRATE}@.+ already exists"; then
  echo "Current version of $CRATE is published."
  exit 0
fi

echo "Current version of $CRATE is not yet published."
exit 1

