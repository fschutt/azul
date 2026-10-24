#!/bin/sh
# AzDrive's sync scenarios (examples/azul-drive/e2e/sync/*.json, commands in e2e/COMMANDS.md),
# run by azul's E2E runner inside a headless AzDrive: one process per scenario file.
#
#   examples/azul-drive/scripts/e2e_sync.sh [file-or-folder] [AzDrive binary]
#
# A folder runs every scenario in it and in its subfolders (e2e/sync/catalogue/); a test marked
# "pending" or "not_applicable" runs no step and is reported as such, never as a failure.
# Defaults: every scenario in e2e/sync, target/release/AzDrive. libazul must be built with the
# debug server (the `debug-server` feature, as for every AZ_E2E run). Everything AzDrive reads
# and writes goes to one temporary folder (the data root, the drives file, Home, the caches,
# the scenarios' sandboxes, the process's own system temp folder), removed at the end unless
# AZDRIVE_E2E_KEEP=1. Exit status: the runner's (0 = every scenario passed).
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
scenarios=${1:-$here/e2e/sync}
bin=${2:-${AZDRIVE_BIN:-$here/../../target/release/AzDrive}}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/azdrive-e2e-sync.XXXXXX")
cleanup() {
    if [ "${AZDRIVE_E2E_KEEP:-0}" != 1 ]; then
        chmod -R u+rwX "$tmp" 2>/dev/null || true
        rm -rf "$tmp"
    else
        echo "kept: $tmp"
    fi
}
trap cleanup EXIT INT TERM
mkdir -p "$tmp/data" "$tmp/home" "$tmp/downloads" "$tmp/cache" "$tmp/work" "$tmp/systemp"
status=0
AZ_BACKEND=headless \
AZ_E2E="$scenarios" \
AZLIN_DATA="$tmp/data" \
AZLIN_CONFIG="$tmp/no-azlin-config.json" \
AZUL_DRIVES="$tmp/drives.json" \
AZDRIVE_HOME="$tmp/home" \
AZDRIVE_DOWNLOADS="$tmp/downloads" \
AZDRIVE_CACHE="$tmp/cache" \
AZDRIVE_E2E_WORK="$tmp/work" \
AZDRIVE_DIALOGS=inline \
AZDRIVE_SYNC_POLL=3600 \
AZDRIVE_SYNC_SETTLE_MS=0 \
TMPDIR="$tmp/systemp" \
    "$bin" || status=$?
exit "$status"
