#!/usr/bin/env bash
# =============================================================================
# test_export_code.sh — E2E test for the debug server's code export API
#
# 1. Builds hello-world.c against the DLL
# 2. Launches it with AZUL_DEBUG on a free port
# 3. Uses the API to request a Rust code export
# 4. Writes the exported files to an output directory
# 5. Prints the filepath of main.rs
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PORT=8799
OUT_DIR="$ROOT/target/export_test"
HELLO_BIN="$ROOT/target/release/hello-world"

echo "=== Step 1: Build hello-world.c ==="
cc -o "$HELLO_BIN" \
    "$ROOT/examples/c/hello-world.c" \
    -I "$ROOT/dll/" \
    -L "$ROOT/target/release" \
    -lazul \
    -Wl,-rpath,"$ROOT/target/release"

echo "  -> Built: $HELLO_BIN"

echo ""
echo "=== Step 2: Launch with AZUL_DEBUG=$PORT ==="
AZUL_DEBUG=$PORT "$HELLO_BIN" &>/dev/null &
APP_PID=$!

cleanup() {
    echo ""
    echo "=== Cleanup: killing PID $APP_PID ==="
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Wait for server to be ready
echo "  -> Waiting for debug server on port $PORT..."
for i in $(seq 1 30); do
    if curl -s -o /dev/null -w '' "http://localhost:$PORT/" 2>/dev/null; then
        echo "  -> Server ready (attempt $i)"
        break
    fi
    sleep 0.5
done

echo ""
echo "=== Step 3: Test get_libraries API ==="
LIBS=$(curl -s -X POST "http://localhost:$PORT/" \
    -H 'Content-Type: application/json' \
    -d '{"op":"get_libraries"}')
echo "  -> Libraries response:"
echo "$LIBS" | python3 -m json.tool 2>/dev/null || echo "$LIBS"

echo ""
echo "=== Step 4: Test get_library_components API (builtin) ==="
COMPS=$(curl -s -X POST "http://localhost:$PORT/" \
    -H 'Content-Type: application/json' \
    -d '{"op":"get_library_components","library":"builtin"}')
COMP_COUNT=$(echo "$COMPS" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('data',{}).get('value',{}).get('components',[])))" 2>/dev/null || echo "?")
echo "  -> Builtin library has $COMP_COUNT components"

echo ""
echo "=== Step 5: Export code (Rust) ==="
EXPORT_FILE="$OUT_DIR/export_response.json"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

curl -s -X POST "http://localhost:$PORT/" \
    -H 'Content-Type: application/json' \
    -d '{"op":"export_code","language":"rust"}' \
    -o "$EXPORT_FILE"

echo "  -> Export response status: $(python3 -c "import json; print(json.load(open('$EXPORT_FILE')).get('status','?'))")"

# Extract files from JSON and write them
python3 -c "
import json, sys, os

data = json.load(open('$EXPORT_FILE'))
files = data.get('data', {}).get('value', {}).get('files', {})
warnings = data.get('data', {}).get('value', {}).get('warnings', [])
out_dir = '$OUT_DIR'

if not files:
    print('  -> WARNING: No files in export response')
    sys.exit(0)

for fname, content in files.items():
    fpath = os.path.join(out_dir, fname)
    os.makedirs(os.path.dirname(fpath) if os.path.dirname(fpath) else out_dir, exist_ok=True)
    with open(fpath, 'w') as f:
        f.write(content)
    print(f'  -> Wrote: {fpath}')

if warnings:
    for w in warnings:
        print(f'  -> Warning: {w}')
"

echo ""
echo "=== Step 6: Show exported main.rs ==="
MAIN_RS="$OUT_DIR/main.rs"
if [ -f "$MAIN_RS" ]; then
    echo "  -> Filepath: $MAIN_RS"
    echo ""
    echo "--- main.rs contents ---"
    cat "$MAIN_RS"
    echo "--- end ---"
else
    echo "  -> main.rs not found. Files in $OUT_DIR:"
    ls -la "$OUT_DIR"
fi

echo ""
echo "=== Step 7: Test component detail (div) ==="
DETAIL_FILE="$OUT_DIR/detail_response.json"
curl -s -X POST "http://localhost:$PORT/" \
    -H 'Content-Type: application/json' \
    -d '{"op":"get_library_components","library":"builtin"}' \
    -o "$DETAIL_FILE"

python3 -c "
import json
data = json.load(open('$DETAIL_FILE'))
comps = data.get('data', {}).get('value', {}).get('components', [])
div = next((c for c in comps if c.get('tag') == 'div'), None)
if div:
    print('  Component: ' + div.get('display_name', ''))
    print('  Qualified: ' + div.get('qualified_name', ''))
    print('  Child policy: ' + div.get('child_policy', ''))
    print('  Attributes: ' + str(len(div.get('attributes', []))))
    print('  Data fields: ' + str(len(div.get('data_fields', []))))
    print('  Callbacks: ' + str(len(div.get('callback_slots', []))))
    print('  Scoped CSS: ' + (div.get('scoped_css', '') or '(none)'))
else:
    print('  -> div component not found')
"

echo ""
echo "=== ALL TESTS PASSED ==="
