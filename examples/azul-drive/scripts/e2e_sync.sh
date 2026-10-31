#!/bin/sh
# AzDrive's sync scenarios (examples/azul-drive/e2e/sync/*.json, commands in e2e/COMMANDS.md),
# run by azul's E2E runner inside a headless AzDrive: one process per scenario file.
#
#   examples/azul-drive/scripts/e2e_sync.sh [file-or-folder] [AzDrive binary]
#
# A folder runs every scenario in it and in its subfolders (e2e/sync/catalogue/); a test marked
# "pending" or "not_applicable" runs no step and is reported as such, never as a failure.
# Defaults: every scenario in e2e/sync, target/e2e-unwind/AzDrive (the e2e-unwind profile:
# release with unwinding panics, which the scenarios' crash points need; `cargo build --profile
# e2e-unwind -p AzDrive --features fake-providers`). libazul must be built with the debug server
# (the `debug-server` feature, as for every AZ_E2E run).
#
# Every file runs in an AzDrive of its own, with folders of its own (the data root, the drives
# file, Home, the caches, the keyring file, the scenarios' sandboxes, the process's own system
# temp folder; the tests of one file each start from a fresh sandbox, see `_test` in
# e2e/COMMANDS.md): the
# files run side by side (AZDRIVE_E2E_JOBS at once, default the cores less one), and an AzDrive
# keeps its pairings and settings in its data root, so two files sharing one would see each
# other's devices. All of it lies in one temporary folder, removed at the end unless
# AZDRIVE_E2E_KEEP=1. Each file's output goes to AZDRIVE_E2E_LOGS (default target/e2e/logs under
# the current folder), one line per file says how it went, and the last line sums the tests.
# Exit status: 0 when no test failed.
set -eu

# One file, in its own folders (the script runs itself per file).
if [ "${1:-}" = --one ]; then
    file=$2
    name=$(printf '%s' "${file#"$E2E_SCENARIOS"/}" | sed 's|\.json$||; s|/|-|g')
    dir=$E2E_TMP/$name
    mkdir -p "$dir/data" "$dir/home" "$dir/downloads" "$dir/cache" "$dir/work" "$dir/systemp"
    log=$E2E_LOGS/$name.log
    status=0
    AZ_BACKEND=headless \
    AZ_E2E="$file" \
    AZLIN_DATA="$dir/data" \
    AZLIN_CONFIG="$dir/no-azlin-config.json" \
    AZUL_DRIVES="$dir/drives.json" \
    AZDRIVE_HOME="$dir/home" \
    AZDRIVE_DOWNLOADS="$dir/downloads" \
    AZDRIVE_CACHE="$dir/cache" \
    AZDRIVE_E2E_WORK="$dir/work" \
    AZDRIVE_DIALOGS=inline \
    AZDRIVE_SYNC_POLL=3600 \
    AZDRIVE_SYNC_SETTLE_MS=0 \
    TMPDIR="$dir/systemp" \
    AZ_KEYRING_FILE="$dir/keyring.json" \
        "$E2E_BIN" > "$log" 2>&1 || status=$?
    # The runner's own tally of the file: "test result: ... N passed; N failed; ...".
    esc=$(printf '\033')
    summary=$(sed "s/${esc}\[[0-9;]*m//g" "$log" | grep '^test result:' | tail -n 1 || true)
    count() { printf '%s' "$summary" | sed -n "s/.* \([0-9][0-9]*\) $1.*/\1/p"; }
    passed=$(count passed); failed=$(count failed); xfailed=$(count xfailed)
    xpassed=$(count xpassed); skipped=$(count skipped); pending=$(count pending)
    passed=${passed:-0} failed=${failed:-0} xfailed=${xfailed:-0}
    xpassed=${xpassed:-0} skipped=${skipped:-0} pending=${pending:-0}
    if [ -z "$summary" ]; then
        # A process that ended before its report: one failure at least.
        passed=0 failed=1 xfailed=0 xpassed=0 skipped=0 pending=0
        verdict="FAILED (no report, exit $status)"
    elif [ "$status" != 0 ] || [ "${failed:-0}" != 0 ] || [ "${xpassed:-0}" != 0 ]; then
        verdict=FAILED
    elif [ "$((passed + xfailed))" = 0 ] && [ "${pending:-0}" != 0 ]; then
        verdict=PENDING
    elif [ "$((passed + xfailed))" = 0 ]; then
        verdict=SKIP
    else
        verdict=ok
    fi
    echo "test $name ... $verdict ($passed passed, $failed failed, $pending pending, $skipped skipped)"
    echo "$name $passed $failed $xfailed $xpassed $skipped $pending" >> "$E2E_TMP/tally"
    # A test that failed before its drive_cleanup leaves the volumes it made mounted in its
    # sandbox (drive_volume): they are detached before the folder goes.
    real=$(cd "$dir" && pwd -P)
    mount | sed -n -e 's/^.* on \(.*\) type .*$/\1/p' -e 't' -e 's/^.* on \(.*\) (.*$/\1/p' |
        while read -r point; do
            case "$point" in
                "$real"/*)
                    if command -v hdiutil > /dev/null 2>&1; then
                        hdiutil detach "$point" -force > /dev/null 2>&1 || true
                    else
                        umount "$point" 2> /dev/null || sudo -n umount "$point" 2> /dev/null || true
                    fi
                    ;;
            esac
        done
    if [ "${AZDRIVE_E2E_KEEP:-0}" != 1 ]; then
        chmod -R u+rwX "$dir" 2>/dev/null || true
        rm -rf "$dir"
    fi
    exit 0
fi

here=$(cd "$(dirname "$0")/.." && pwd)
scenarios=${1:-$here/e2e/sync}
scenarios=$(cd "$(dirname "$scenarios")" && pwd)/$(basename "$scenarios")
bin=${2:-${AZDRIVE_BIN:-$here/../../target/e2e-unwind/AzDrive}}
jobs=${AZDRIVE_E2E_JOBS:-$(( $(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2) - 1 ))}
[ "$jobs" -ge 1 ] || jobs=1
tmp=$(mktemp -d "${TMPDIR:-/tmp}/azdrive-e2e-sync.XXXXXX")
logs=${AZDRIVE_E2E_LOGS:-$PWD/target/e2e/logs}
mkdir -p "$logs"
cleanup() {
    if [ "${AZDRIVE_E2E_KEEP:-0}" != 1 ]; then
        chmod -R u+rwX "$tmp" 2>/dev/null || true
        rm -rf "$tmp"
    else
        echo "kept: $tmp"
    fi
}
trap cleanup EXIT INT TERM
: > "$tmp/tally"
if [ -d "$scenarios" ]; then
    root=$scenarios
    find "$scenarios" -name '*.json' -type f | sort > "$tmp/files"
else
    root=$(dirname "$scenarios")
    echo "$scenarios" > "$tmp/files"
fi
# Each file's run reports itself; one that ended early (xargs says so) still leaves its line in
# the tally, so the sum below is always written.
E2E_SCENARIOS=$root E2E_TMP=$tmp E2E_LOGS=$logs E2E_BIN=$bin \
    xargs -n 1 -P "$jobs" "$0" --one < "$tmp/files" ||
    echo "e2e_sync: a file's run ended early (xargs: $?)" >&2
awk '{ p += $2; f += $3; xf += $4; xp += $5; s += $6; pe += $7; n++ }
     END { printf "test result: %s. %d passed; %d failed; %d xfailed; %d xpassed; %d skipped; %d pending (%d files, logs in '"$logs"')\n",
           (f + xp == 0 ? "ok" : "FAILED"), p, f, xf, xp, s, pe, n }' "$tmp/tally"
failed=$(awk '{ f += $3 + $5 } END { print f + 0 }' "$tmp/tally")
[ "$failed" = 0 ]
