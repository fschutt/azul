#!/usr/bin/env python3
"""
Ask Gemini about scroll frame architecture and clipping in WebRender.
"""

import os
import sys
import json
import base64
from pathlib import Path

ROOT = Path(__file__).parent.parent.absolute()

# Read Gemini API key
try:
    GEMINI_API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()
except:
    print("ERROR: Could not read GEMINI_API_KEY.txt")
    sys.exit(1)

def encode_image(path):
    try:
        with open(path, "rb") as f:
            return base64.standard_b64encode(f.read()).decode("utf-8")
    except:
        return None

# Current screenshot
screenshot_path = ROOT / "target/test_results/after-diff-2/image.png"
screenshot_b64 = encode_image(screenshot_path)

prompt = """# WebRender Scroll Frame Architecture Question

## Current Situation

We have a TextArea with:
- CSS: `height: 200px; overflow-y: auto; white-space: pre;`
- 15 lines of text at font-size 36px
- Total content height: ~1000px
- Visible viewport: 200px

## What We've Verified with Debug Output

1. **Inline Content Collection:** ✅ Correct
   - 29 items collected (15 text items + 14 LineBreak items)

2. **Text Layout Engine:** ✅ Correct  
   - `Fragment 'main' has 428 items, bounds=Rect { width: 623, height: 999 }`
   - All 15 lines are laid out correctly

3. **Display List Generation:** ❓ Problem Area
   - `paint_inline_content` receives layout with 428 items
   - BUT `get_glyph_runs_simple` returns only **1 glyph_run** (all glyphs merged)
   - `container_rect = 659x200 @ (56, 91)` ← Height is 200px (viewport), not 999px (content)

## The Bug

Only ~3 lines of text are rendered. The rest are invisible even when scrolling.

## Display List Structure (Current)

```
PushClip { bounds: 200px height }
PushScrollFrame { clip_bounds: 200px, content_size: 999px }
  Text { glyphs: [428 glyphs], clip_rect: 200px height }  ← Problem?
PopScrollFrame
PopClip
```

## Our Proposed Fix

We want to extend `container_rect` to the full content height (999px) before calling `paint_inline_content`. This way the `clip_rect` passed to `Text` display items would be 999px, not 200px.

```rust
// For scrollable containers, extend the content rect to the full content size.
let content_size = get_scroll_content_size(node);
if content_size.height > content_box_rect.size.height {
    content_box_rect.size.height = content_size.height;
}
```

## Questions for Gemini

1. **Is our diagnosis correct?** Is the problem that Text items have a clip_rect of 200px, causing glyphs at y > 200px to be clipped?

2. **WebRender Architecture:**
   - In WebRender, who is responsible for clipping scroll content?
   - Does `PushScrollFrame` establish a clip? Or does it just establish a spatial coordinate system for scrolling?
   - Should `Text` display items inside a scroll frame have their own clip_rect, or should clipping be handled entirely by the scroll frame?

3. **W3C Model:**
   - According to CSS Overflow Module Level 3, how should scroll containers clip their content?
   - Is content beyond the viewport still "rendered" (just clipped), or should it not be generated at all?

4. **Is our fix correct?**
   - Should the `container_rect` for painting match the CONTENT size (999px) or the VIEWPORT size (200px)?
   - Or should we remove the clip_rect from Text items entirely when inside a scroll frame?

5. **Alternative Fixes:**
   - Should `get_glyph_runs_simple` break runs at line boundaries?
   - Is the problem in WebRender rendering, not display list generation?
   - Could the issue be that glyphs with y > 200px are being filtered somewhere?

## Code Context

The `paint_inline_content` function receives:
- `container_rect`: Currently the border-box of the node (200px height)
- `layout`: The UnifiedLayout with all 428 positioned items

It then:
1. Calls `get_glyph_runs_simple(layout)` to get glyph runs
2. For each run, offsets glyph positions by `container_rect.origin`
3. Pushes a `Text` display item with `clip_rect = container_rect`

The clip_rect in Text is used by the renderer. If glyphs are at y=500px but clip_rect.height=200px, they may be clipped out.

## Please Provide

1. Confirmation of root cause
2. The correct architectural solution based on WebRender/W3C
3. Specific code changes needed
"""

print(f"Prompt length: {len(prompt)} chars")

# Build Gemini request
parts = []

if screenshot_b64:
    parts.append({"text": "Current Screenshot (scrollbar works, but only 3 lines visible):"})
    parts.append({
        "inline_data": {
            "mime_type": "image/png",
            "data": screenshot_b64
        }
    })

parts.append({"text": prompt})

request_body = {
    "contents": [{"parts": parts}],
    "generationConfig": {
        "temperature": 0.2,
        "maxOutputTokens": 16384
    }
}

# Send to Gemini
import urllib.request

url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={GEMINI_API_KEY}"

print("Sending to Gemini...")
req = urllib.request.Request(
    url,
    data=json.dumps(request_body).encode("utf-8"),
    headers={"Content-Type": "application/json"},
    method="POST"
)

try:
    with urllib.request.urlopen(req, timeout=180) as response:
        result = json.loads(response.read().decode("utf-8"))
        
        if "candidates" in result and result["candidates"]:
            text = result["candidates"][0]["content"]["parts"][0]["text"]
            print("\n" + "="*60)
            print("GEMINI RESPONSE:")
            print("="*60)
            print(text)
            
            # Save response
            output_path = ROOT / "scripts/gemini_scroll_architecture_response.md"
            output_path.write_text(text)
            print(f"\nSaved to: {output_path}")
        else:
            print(f"Unexpected response: {json.dumps(result, indent=2)}")
            
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
