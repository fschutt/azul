#!/bin/bash
set -e

PORT=9222
cd /Users/fschutt/Development/azul/examples/c

# Kill any existing async process
pkill -f "async" 2>/dev/null || true
sleep 0.5

echo "Starting async example with AZUL_DEBUG=$PORT..."
AZUL_DEBUG=$PORT ./async &
APP_PID=$!
sleep 2

echo ""
echo "=== Initial DOM ==="
curl -s -X POST http://localhost:$PORT/ -d '{"op": "get_html_string"}' | python3 -c "
import sys,json,re
d=json.load(sys.stdin)
html=d.get('html','')
m=re.search(r'Progress: ([0-9.]+)%', html)
print('Progress text:', m.group(1) if m else 'NOT FOUND')
m2=re.search(r'progress-bar-bar.*?flex-grow: ([0-9.]+)', html)
print('Green bar flex-grow:', m2.group(1) if m2 else 'NOT FOUND')
"

echo ""
echo "=== Clicking Start button ==="
curl -s -X POST http://localhost:$PORT/ -d '{"op": "click_node", "text": "Start"}'
echo ""

for i in 1 2 3 4 5; do
    sleep 1
    echo ""
    echo "=== After ${i}s ==="
    curl -s -X POST http://localhost:$PORT/ -d '{"op": "get_html_string"}' | python3 -c "
import sys,json,re
d=json.load(sys.stdin)
html=d.get('html','')
m=re.search(r'Progress: ([0-9.]+)%', html)
print('Progress text:', m.group(1)+'%' if m else 'NOT FOUND')
m2=re.search(r'progress-bar-bar.*?flex-grow: ([0-9.]+)', html)
print('Green bar flex-grow:', m2.group(1) if m2 else 'NOT FOUND')
"
done

echo ""
echo "=== Cleaning up ==="
kill $APP_PID 2>/dev/null || true
echo "Done"
