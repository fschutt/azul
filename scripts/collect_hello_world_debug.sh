#!/usr/bin/env bash
# Starts hello-world with AZUL_DEBUG, collects all debug API data via curl,
# saves each response to individual JSON files + a combined dump.
set -euo pipefail

AZUL_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${1:-8765}"
API="http://localhost:${PORT}/"
OUT_DIR="$AZUL_ROOT/doc/target/hello-world-debug"
HELLO="$AZUL_ROOT/target/c-examples/hello-world"

mkdir -p "$OUT_DIR"

# ── 1. Build hello-world if needed ────────────────────────────────────────
if [ ! -f "$HELLO" ] || [ "$AZUL_ROOT/examples/c/hello-world.c" -nt "$HELLO" ]; then
  echo "[*] Building hello-world.c ..."
  mkdir -p "$AZUL_ROOT/target/c-examples"
  cc -o "$HELLO" "$AZUL_ROOT/examples/c/hello-world.c" \
    -I"$AZUL_ROOT/target/codegen/v2" \
    -framework Cocoa -framework OpenGL -framework IOKit \
    -framework CoreFoundation -framework CoreGraphics \
    -L"$AZUL_ROOT/target/release" -lazul \
    -Wl,-rpath,"$AZUL_ROOT/target/release"
fi

# ── 2. Kill any previous instance on the port ─────────────────────────────
lsof -ti :${PORT} 2>/dev/null | xargs -r kill 2>/dev/null || true
sleep 0.3

# ── 3. Start hello-world with debug API ──────────────────────────────────
echo "[*] Starting hello-world on port ${PORT} ..."
AZUL_DEBUG=${PORT} "$HELLO" &
APP_PID=$!

# Wait for the debug server to come up
for i in $(seq 1 30); do
  if curl -s --max-time 1 "$API" >/dev/null 2>&1; then
    echo "[*] Debug server ready (attempt $i)"
    break
  fi
  sleep 0.5
done

# Verify it's alive
if ! kill -0 $APP_PID 2>/dev/null; then
  echo "[!] hello-world exited before debug server was ready"
  exit 1
fi

# ── 4. Helper: POST a command, save to file ───────────────────────────────
post() {
  local name="$1"
  local body="$2"
  local out_file="$OUT_DIR/${name}.json"
  echo "  → $name"
  curl -s --max-time 10 -X POST "$API" -d "$body" > "$out_file" 2>/dev/null || echo '{"status":"error","message":"curl failed"}' > "$out_file"
}

echo "[*] Collecting debug data ..."

# ── 5. Window / state inspection ──────────────────────────────────────────
post "get_state"              '{"op":"get_state"}'
post "get_dom"                '{"op":"get_dom"}'
post "get_dom_tree"           '{"op":"get_dom_tree"}'
post "get_html_string"        '{"op":"get_html_string"}'
post "get_layout_tree"        '{"op":"get_layout_tree"}'
post "get_display_list"       '{"op":"get_display_list"}'
post "get_all_nodes_layout"   '{"op":"get_all_nodes_layout"}'
post "get_node_hierarchy"     '{"op":"get_node_hierarchy"}'
post "get_scroll_states"      '{"op":"get_scroll_states"}'
post "get_scrollable_nodes"   '{"op":"get_scrollable_nodes"}'
post "get_logs"               '{"op":"get_logs"}'

# ── 6. Per-node CSS / layout ─────────────────────────────────────────────
post "css_body"               '{"op":"get_node_css_properties","selector":"body"}'
post "css_html"               '{"op":"get_node_css_properties","selector":"html"}'
post "layout_body"            '{"op":"get_node_layout","selector":"body"}'
post "layout_html"            '{"op":"get_node_layout","selector":"html"}'
post "layout_title"           '{"op":"get_node_layout","text":"Hello"}'
post "layout_button"          '{"op":"get_node_layout","text":"Increase count"}'
post "layout_counter"         '{"op":"get_node_layout","text":"5"}'
post "find_hello"             '{"op":"find_node_by_text","text":"Hello"}'
post "find_button"            '{"op":"find_node_by_text","text":"Increase count"}'
post "find_counter"           '{"op":"find_node_by_text","text":"5"}'

# ── 7. Hit-test at key locations ──────────────────────────────────────────
post "hit_test_title"         '{"op":"hit_test","x":200,"y":50}'
post "hit_test_button"        '{"op":"hit_test","x":200,"y":150}'
post "hit_test_counter"       '{"op":"hit_test","x":50,"y":170}'
post "hit_test_center"        '{"op":"hit_test","x":200,"y":200}'

# ── 8. App state before click ────────────────────────────────────────────
post "app_state_before"       '{"op":"get_app_state"}'

# ── 9. Click the button, wait, check state ───────────────────────────────
post "click_button"           '{"op":"click","text":"Increase count"}'
post "wait_frame"             '{"op":"wait_frame"}'
post "app_state_after"        '{"op":"get_app_state"}'
post "get_logs_after_click"   '{"op":"get_logs"}'

# ── 10. Screenshots ──────────────────────────────────────────────────────
post "take_screenshot"        '{"op":"take_screenshot"}'
post "take_native_screenshot" '{"op":"take_native_screenshot"}'

# ── 11. Combine everything into one JSON blob ─────────────────────────────
echo "[*] Merging into combined dump ..."
COMBINED="$OUT_DIR/hello_world_debug_dump.json"
echo '{' > "$COMBINED"
first=true
for f in "$OUT_DIR"/*.json; do
  name="$(basename "$f" .json)"
  [ "$name" = "hello_world_debug_dump" ] && continue
  [ "$name" = "hello_world_gemini_request" ] && continue
  [ "$name" = "hello_world_gemini_prompt" ] && continue
  [ "$name" = "hello_world_gemini_response" ] && continue
  if $first; then first=false; else echo ',' >> "$COMBINED"; fi
  printf '  "%s": ' "$name" >> "$COMBINED"
  cat "$f" >> "$COMBINED"
done
echo '' >> "$COMBINED"
echo '}' >> "$COMBINED"

echo "[*] Debug dump saved to: $COMBINED"
echo "[*] Individual files in: $OUT_DIR/"
ls -la "$OUT_DIR"/*.json | awk '{print "     ", $NF, $5}'

# ── 12. Shut down the app ────────────────────────────────────────────────
echo "[*] Closing hello-world ..."
curl -s --max-time 5 -X POST "$API" -d '{"op":"close"}' >/dev/null 2>&1 || true
sleep 0.5
kill $APP_PID 2>/dev/null || true

echo "[*] Done!"
