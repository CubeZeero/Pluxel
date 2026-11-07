#!/bin/sh
set -eu
cd "$(dirname "$0")"
bin=$(mktemp -t px-filesystem-selftest)
trap 'rm -f "$bin"' EXIT HUP INT TERM
xcrun clang -fobjc-arc -Wall -Wextra -Werror -fsyntax-only Filesystem.m
xcrun clang -fobjc-arc -Wall -Wextra -Werror -DPX_TESTING -framework Foundation Filesystem.m FilesystemSelftest.m -o "$bin"
"$bin"
