#!/bin/bash
# REFACTOR13: run ONE cargo command under the machine-wide build lock
# (/tmp/az_app_run.lock, shared with the other sessions on this Mac), with the
# refactor's own target dir outside the repo and no debuginfo (debuginfo only
# costs disk here; the frame measurements say which profile they used).
#
#   scripts/refactor/locked_cargo.sh cargo test -p azul-layout --lib -j 4
#
# Never builds below 6 GB of free disk: it waits (lock released, so the other
# sessions can finish and free space) up to LOCKED_CARGO_WAIT_MIN minutes
# (default 90), then gives up with exit 99. The lock is released on any exit.
MIN_KB=$((6 * 1024 * 1024))
free_kb() { df -k /System/Volumes/Data | awk 'NR==2 {print $4}'; }
deadline=$(($(date +%s) + ${LOCKED_CARGO_WAIT_MIN:-90} * 60))
while :; do
    until [ "$(free_kb)" -ge "$MIN_KB" ]; do
        if [ "$(date +%s)" -ge "$deadline" ]; then
            echo "locked_cargo: below 6 GB free for too long - not building" >&2
            exit 99
        fi
        sleep 30
    done
    until mkdir /tmp/az_app_run.lock 2>/dev/null; do sleep 10; done
    # Again with the lock held: the wait can be long and other builds fill the disk.
    if [ "$(free_kb)" -ge "$MIN_KB" ]; then
        break
    fi
    rmdir /tmp/az_app_run.lock 2>/dev/null
    echo "locked_cargo: $(($(free_kb) / 1024 / 1024)) GB free with the lock held - waiting" >&2
done
trap 'rmdir /tmp/az_app_run.lock 2>/dev/null' EXIT INT TERM HUP
echo "locked_cargo: lock taken $(date +%H:%M:%S), $(($(free_kb) / 1024 / 1024)) GB free" >&2
export CARGO_TARGET_DIR="${REFACTOR_TARGET_DIR:-/Users/fschutt/Development/azul-refactor-target}"
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
# The incremental cache of the debug test builds alone grew to 2.2 GB on a
# disk with < 8 GB free; a clean rebuild of azul-layout is minutes.
export CARGO_INCREMENTAL=0
"$@"
rc=$?
echo "locked_cargo: done $(date +%H:%M:%S) rc=$rc" >&2
exit $rc
