#!/bin/bash
# Test script for scroll_to API in Azul
# This script verifies that the scroll_to JSON API works correctly

PORT=8765
API="http://localhost:$PORT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Azul Scroll API Test ===${NC}"

# Check if server is running
if ! curl -s "$API/" > /dev/null 2>&1; then
    echo -e "${RED}Debug server not running on port $PORT${NC}"
    echo "Please start the scrolling example with: AZUL_DEBUG=$PORT cargo run --release --example scrolling"
    exit 1
fi

echo -e "${GREEN}Debug server is running${NC}"

# Get initial scroll state
echo -e "\n${YELLOW}1. Getting initial scroll state...${NC}"
INITIAL=$(curl -s -X POST "$API/" -d '{"op":"get_scroll_states"}')
echo "$INITIAL" | python3 -c "import json,sys; d=json.load(sys.stdin); s=d['data']['value']['scroll_states'][0]; print(f'  Node {s[\"node_id\"]}: scroll_y={s[\"scroll_y\"]}, max_scroll_y={s[\"max_scroll_y\"]}')"

# Reset scroll to 0
echo -e "\n${YELLOW}2. Resetting scroll to (0, 0)...${NC}"
curl -s -X POST "$API/" -d '{"op":"scroll_node_to", "node_id": 1, "x": 0, "y": 0}' | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'  Result: {d[\"status\"]}')"
sleep 0.3

# Animate scroll in 10 steps
echo -e "\n${YELLOW}3. Animating scroll (10 steps of 5 pixels each)...${NC}"
for i in $(seq 1 10); do
    Y=$((i * 5))
    RESULT=$(curl -s -X POST "$API/" -d "{\"op\":\"scroll_node_to\", \"node_id\": 1, \"x\": 0, \"y\": $Y}")
    STATUS=$(echo "$RESULT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['status'])")
    if [ "$STATUS" = "ok" ]; then
        echo -e "  ${GREEN}✓${NC} Step $i: scroll_y = $Y"
    else
        echo -e "  ${RED}✗${NC} Step $i: FAILED"
    fi
    sleep 0.2
done

# Get final scroll state
echo -e "\n${YELLOW}4. Getting final scroll state...${NC}"
FINAL=$(curl -s -X POST "$API/" -d '{"op":"get_scroll_states"}')
echo "$FINAL" | python3 -c "import json,sys; d=json.load(sys.stdin); s=d['data']['value']['scroll_states'][0]; print(f'  Final scroll_y={s[\"scroll_y\"]} (expected: 50.0)')"

# Verify scroll state
SCROLL_Y=$(echo "$FINAL" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['value']['scroll_states'][0]['scroll_y'])")
if [ "$SCROLL_Y" = "50.0" ]; then
    echo -e "\n${GREEN}✓ TEST PASSED: Scroll API works correctly!${NC}"
else
    echo -e "\n${RED}✗ TEST FAILED: Expected scroll_y=50.0, got $SCROLL_Y${NC}"
    exit 1
fi

# Test scroll_node_by
echo -e "\n${YELLOW}5. Testing scroll_node_by (delta scroll)...${NC}"
curl -s -X POST "$API/" -d '{"op":"scroll_node_by", "node_id": 1, "delta_x": 0, "delta_y": 25}' | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'  Result: {d[\"status\"]}')"
sleep 0.2

AFTER_DELTA=$(curl -s -X POST "$API/" -d '{"op":"get_scroll_states"}')
SCROLL_Y_AFTER=$(echo "$AFTER_DELTA" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['value']['scroll_states'][0]['scroll_y'])")
if [ "$SCROLL_Y_AFTER" = "75.0" ]; then
    echo -e "${GREEN}✓ scroll_node_by works: scroll_y = $SCROLL_Y_AFTER (50 + 25 = 75)${NC}"
else
    echo -e "${YELLOW}! scroll_node_by result: scroll_y = $SCROLL_Y_AFTER (expected 75.0)${NC}"
fi

echo -e "\n${GREEN}=== Test Complete ===${NC}"
