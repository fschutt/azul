#!/bin/bash
# Run a prebuilt azul app (or a script that starts one) under a memory cap.
#
#   scripts/waves/tools/run_capped.sh [--cap-mb 1500] [--seconds 120] [--log FILE] -- CMD ARGS...
#
# - The whole PROCESS TREE counts (a python harness + the app it starts): killed
#   the moment the tree's RSS passes the cap (default 1500 MB) -> "CAPPED <MB>", exit 137;
#   or after --seconds -> "TIMEOUT", exit 124; otherwise the command's own exit code.
# - ONE capped run at a time on this machine (a lock under $AZ_WORK/locks): the 8 GB
#   Mac kernel-panicked on 2026-09-30 when an app reached 17.7 GB, and with a dozen
#   agents in parallel "one at a time" needs a lock, not a promise. A stale lock (its
#   holder pid gone) is taken over.
# - DYLD_LIBRARY_PATH defaults to the main checkout's target/azul-lib.
cap=1500; secs=120; log=/dev/null
while [ $# -gt 0 ]; do
  case "$1" in
    --cap-mb) cap=$2; shift 2;;
    --seconds) secs=$2; shift 2;;
    --log) log=$2; shift 2;;
    --) shift; break;;
    *) break;;
  esac
done
AZ_WORK=${AZ_WORK:-$HOME/Development/azul-work}
lock=$AZ_WORK/locks/run_capped.lock
mkdir -p "$AZ_WORK/locks"
waited=0
until mkdir "$lock" 2>/dev/null; do
  holder=$(cat "$lock/pid" 2>/dev/null)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then rm -rf "$lock"; continue; fi
  [ $((waited % 60)) -eq 0 ] && echo "run_capped: waiting for the lock (held by pid ${holder:-?})" >&2
  sleep 2; waited=$((waited + 2))
done
echo $$ > "$lock/pid"
trap 'rm -rf "$lock"' EXIT
export DYLD_LIBRARY_PATH=${DYLD_LIBRARY_PATH:-/Users/fschutt/Development/azul/target/azul-lib}
"$@" > "$log" 2>&1 &
pid=$!
start=$(date +%s)
tree() { echo $1; for c in $(pgrep -P $1 2>/dev/null); do tree $c; done; }
while kill -0 $pid 2>/dev/null; do
  pids=$(tree $pid | paste -sd, -)
  rss=$(ps -o rss= -p "$pids" 2>/dev/null | awk '{s+=$1} END {print s+0}')
  if [ -n "$rss" ] && [ "$rss" -gt $((cap * 1024)) ]; then
    kill -9 $(echo $pids | tr , ' ') 2>/dev/null; echo "CAPPED $((rss / 1024))MB" >&2; exit 137
  fi
  if [ $(( $(date +%s) - start )) -ge $secs ]; then
    kill $(echo $pids | tr , ' ') 2>/dev/null; sleep 1; kill -9 $(echo $pids | tr , ' ') 2>/dev/null
    echo "TIMEOUT ${secs}s" >&2; exit 124
  fi
  sleep 1
done
wait $pid
