#!/usr/bin/env bash
# The Python extension of the release workflow (`cargo build --profile prod-release -p azul-dll --lib
# --features=python-extension`) killed its Ubuntu runner ('The hosted runner lost communication with the server',
# release run 45803150843): is it memory? This runs the same build with 12 GB of swap and a memory log (one line every
# 20 s), and prints the peak the log saw.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/pyext_memory_probe.sh' --codegen
set -u

echo "=== before"
free -h
df -h /mnt / | tail -2

if [ "${PYEXT_NO_SWAP:-0}" != "1" ]; then
  sudo fallocate -l 12G /mnt/azul-swap && sudo chmod 600 /mnt/azul-swap && sudo mkswap /mnt/azul-swap && sudo swapon /mnt/azul-swap
  echo "=== with swap"
  free -h
fi

log="$(mktemp)"
(
  while true; do
    echo "$(date +%T) $(free -m | awk 'NR==2 {print "mem_used_mb=" $3 " avail_mb=" $7} NR==3 {print "swap_used_mb=" $3}' | tr '\n' ' ')"
    sleep 20
  done
) > "$log" 2>&1 &
sampler=$!

started="$(date +%s)"
cargo build --profile prod-release -p azul-dll --lib --features="python-extension" --target-dir target/pyext > build.log 2>&1
status=$?
kill "$sampler" 2>/dev/null
echo "=== cargo exit $status after $(( $(date +%s) - started )) s"
grep -E "^error" -A6 build.log | head -30
tail -n 3 build.log | cut -c1-200
echo "=== memory samples (last 12) and the peak"
tail -n 12 "$log"
sed -n 's/.*mem_used_mb=\([0-9]*\).*/\1/p' "$log" | sort -n | tail -1 | sed 's/^/peak mem_used_mb=/'
sed -n 's/.*swap_used_mb=\([0-9]*\).*/\1/p' "$log" | sort -n | tail -1 | sed 's/^/peak swap_used_mb=/'
echo "PYEXT PROBE EXIT: $status"
exit "$status"
