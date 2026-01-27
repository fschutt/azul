#!/usr/bin/env python3
"""
Send TextArea NoWrap bug analysis to Gemini
"""
import json
import base64
import requests
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()

# Get screenshot
screenshot_path = ROOT / "target/test_results/image.png"
if screenshot_path.exists():
    screenshot_b64 = base64.b64encode(screenshot_path.read_bytes()).decode()
    print(f"Found screenshot: {screenshot_path}")
else:
    screenshot_b64 = ""
    print("WARNING: No screenshot found")

# Get git diff
diff_result = subprocess.run(
    ["git", "diff", "HEAD~3", "layout/src/text3/cache.rs", "layout/src/solver3/fc.rs", "layout/src/solver3/sizing.rs"],
    capture_output=True, text=True, cwd=ROOT
)
git_diff = diff_result.stdout

# Get scrollable nodes info (from Debug API if running)
try:
    resp = requests.post("http://localhost:8765/", json={"op": "get_scrollable_nodes"}, timeout=3)
    scrollable_nodes = resp.json()
except:
    scrollable_nodes = {"error": "Debug API not available"}

try:
    resp = requests.post("http://localhost:8765/", json={"op": "get_display_list"}, timeout=3)
    display_list = resp.json()
except:
    display_list = {"error": "Debug API not available"}

# Build prompt
prompt = f"""# Bug Analysis: TextArea content_size Still Incorrect After NoWrap Fix

## Problem Summary

We fixed `TextWrap::NoWrap` in `get_line_constraints()` to use unlimited width (f32::MAX / 2.0) 
when white-space: pre is set. However, the scrollbar STILL doesn't appear.

**Expected:** 15 lines at 36px font with 1.4 line-height = ~756px content height
**Actual:** scrollable_node_count = 0, meaning content_size <= viewport height (200px)

## Current Debug Output

### Scrollable Nodes
```json
{json.dumps(scrollable_nodes, indent=2)}
```

### Display List (excerpt)
```json
{json.dumps(display_list, indent=2)[:8000]}
```

## Git Diff (Recent Changes)

```diff
{git_diff}
```

## Key Architecture Points

### What We Fixed
1. `split_text_for_whitespace()` in fc.rs - splits text by `\\n` for white-space: pre, creates InlineContent::LineBreak
2. `create_logical_items()` in cache.rs - handles InlineContent::LineBreak, creates LogicalItem::Break
3. `get_line_constraints()` in cache.rs - uses f32::MAX/2.0 width for NoWrap to prevent soft wrapping

### What Should Happen
1. Text "Line 1...\\nLine 2...\\n..." should be split into 15 text segments + 14 LineBreak items
2. Each LineBreak should create a ShapedItem::Break
3. `break_one_line()` should return one line at each Break
4. `position_one_line()` should accumulate line_height for each line
5. `perform_fragment_layout()` should return overflow_size.height ~ 756px
6. Scroll frame should be created with content_size > viewport_size

### The Bug
Looking at display list: `glyph_count: 121` (should be ~200+ for 15 lines)
Text height: 200.0 (= viewport height, NOT content height)

This suggests:
- Either LineBreaks aren't being created/processed correctly
- Or content_size is being clamped to viewport size somewhere
- Or the inline layout result isn't being used for scroll sizing

## Questions for Analysis

1. **Why is glyph_count only 121?** 
   - 15 lines with ~15 chars each = ~225 glyphs expected
   - Is text being truncated after certain lines?

2. **Where is content_size calculated?**
   - Is it using the inline_layout_result.bounds()?
   - Is there a clamp to viewport height happening?

3. **Is split_text_for_whitespace working?**
   - It should produce 15 Text items + 14 LineBreak items
   - Are all 29 items being processed through the pipeline?

4. **Is the fallback line_height being used correctly?**
   - For break-only lines, line_height comes from fragment_constraints.line_height
   - Is this value correct (50.4px for 36px font x 1.4)?

Please provide:
1. The exact location where content_size gets incorrectly calculated
2. Specific code fix (Rust code)
3. Why the current fix isn't sufficient
"""

# Call Gemini
url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key={API_KEY}"

payload = {
    "contents": [{
        "parts": [
            {"text": prompt},
        ]
    }],
    "generationConfig": {
        "temperature": 0.2,
        "maxOutputTokens": 16000
    }
}

# Add screenshot if available
if screenshot_b64:
    payload["contents"][0]["parts"].append({
        "inline_data": {
            "mime_type": "image/png",
            "data": screenshot_b64
        }
    })

print(f"Sending {len(prompt)} chars + screenshot to Gemini...")
resp = requests.post(url, json=payload, timeout=120)

if resp.status_code == 200:
    result = resp.json()
    if "candidates" in result and result["candidates"]:
        text = result["candidates"][0]["content"]["parts"][0]["text"]
        output_path = ROOT / "target/textarea_debug/gemini_response_nowrap.md"
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(text)
        print(f"\n{'='*60}")
        print("GEMINI RESPONSE:")
        print('='*60)
        print(text)
    else:
        print("No candidates in response:", result)
else:
    print(f"Error {resp.status_code}: {resp.text}")
