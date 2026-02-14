#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/../../target/release"
LOG_DIR="/tmp/azul-run-logs"
export DYLD_LIBRARY_PATH="$BUILD_DIR"
mkdir -p "$LOG_DIR"

echo "=== COMPILING ==="
BINS=()
for src in "$SCRIPT_DIR"/*.c; do
  name="$(basename "${src%.c}")"
  bin="$SCRIPT_DIR/$name.bin"
  printf "  %-30s " "$name"
  if cc -o "$bin" "$src" -lazul -L"$BUILD_DIR" -I"$SCRIPT_DIR/../../dll" 2>"$LOG_DIR/$name.compile.log"; then
    echo "OK"
    BINS+=("$name")
  else
    echo "FAIL"
    head -3 "$LOG_DIR/$name.compile.log"
  fi
done

echo ""
echo "=== RUNNING ALL ${#BINS[@]} IN PARALLEL ==="
PIDS=()
for name in "${BINS[@]}"; do
  (cd "$SCRIPT_DIR" && exec "./$name.bin") \
    >"$LOG_DIR/$name.stdout.log" 2>"$LOG_DIR/$name.stderr.log" &
  PIDS+=("$!:$name")
  printf "  %-30s PID %s\n" "$name" "$!"
done

echo ""
echo "${#BINS[@]} processes launched. Logs in $LOG_DIR/"
echo "Press Enter to kill all..."
read -r

for entry in "${PIDS[@]}"; do
  pid="${entry%%:*}"; name="${entry##*:}"
  kill "$pid" 2>/dev/null
done
wait 2>/dev/null
echo "Done."
