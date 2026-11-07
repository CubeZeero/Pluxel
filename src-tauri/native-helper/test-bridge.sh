#!/bin/sh
# Standalone policy/FFI test. Never registers a service or invokes authentication.
set -eu
cd "$(dirname "$0")"
bin=$(mktemp -t px-bridge-selftest)
trap 'rm -f "$bin"' EXIT HUP INT TERM
xcrun clang -fobjc-arc -fblocks -Wall -Wextra -Werror \
  -Wno-deprecated-declarations -mmacosx-version-min=10.15 \
  BridgeSelftest.m -framework Foundation -framework LocalAuthentication \
  -framework ServiceManagement -framework Security -o "$bin"
"$bin"
