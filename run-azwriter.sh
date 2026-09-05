#!/usr/bin/env bash
# Run AzWriter against THIS tree's libazul, not the stale /lib/libazul.so
# (June 1 copy, 41 MB — `ldd target/release/AzWriter` resolves there by default).
# Usage: ./run-azwriter.sh [args...]   e.g. AZ_BACKEND=x11 ./run-azwriter.sh
cd "$(dirname "$0")"
export LD_LIBRARY_PATH="$PWD/target/azul-lib:$PWD/target/release:${LD_LIBRARY_PATH:-}"
exec ./target/release/AzWriter "$@"
