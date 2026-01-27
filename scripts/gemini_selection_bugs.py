#!/usr/bin/env python3
"""
Ask Gemini about selection and cursor issues in text selection test.
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
    except Exception as e:
        print(f"Could not encode image {path}: {e}")
        return None

def read_file(path):
    try:
        return Path(path).read_text()
    except:
        return f"[Could not read {path}]"

# Current screenshot - try multiple locations
screenshot_paths = [
    ROOT / "target/test_results/selection/image.png",
    ROOT / "target/test_results/after-diff-2/image.png",
]
screenshot_b64 = None
for p in screenshot_paths:
    screenshot_b64 = encode_image(p)
    if screenshot_b64:
        print(f"Using screenshot: {p}")
        break

# Read selection test file
selection_c = read_file(ROOT / "tests/e2e/selection.c")

prompt = f"""# Text Selection Bug Report: 4 Issues

## Current State

We have a selection test with 3 paragraphs:
1. **Green (top)**: Selectable text
2. **Red (middle)**: `user-select: none` - should NOT be selectable  
3. **Blue (bottom)**: Selectable text

## Screenshot Analysis

Looking at the attached screenshot:
- There are **blue selection rectangles** visible
- There is a **cyan cursor** visible on the left side of each paragraph
- The selection rects appear to have wrong coordinates (padding issue?)

## 4 Bugs to Debug

### Bug 1: Selection Rectangles at Wrong Position
**Symptom:** The blue selection rectangles are not aligned with the text. They appear offset, possibly not respecting padding.

**Expected:** Selection rects should exactly overlay the selected text glyphs.

**Hypothesis:** Coordinate space transformation issue between:
- Layout coordinates (relative to content-box)
- Display list coordinates (relative to ?)
- Selection rect coordinates

### Bug 2: Cursor Rendered on Non-Editable Elements
**Symptom:** A cyan cursor is visible on the left side of each paragraph, even though NONE of the elements are `contenteditable`.

**Expected:** Cursor should only appear on elements with:
- `contenteditable="true"`
- Or `<input>` / `<textarea>` elements
- Not on plain `<div>` with text

**Question:** Where is the cursor rendering logic? What triggers cursor display?

### Bug 3: Cursor AND Selection Visible Together
**Symptom:** Both the cursor (cyan vertical line) and selection rectangles (blue) are visible simultaneously.

**Expected:** Per text editing conventions:
- When there's a selection (anchor != focus), show selection rects, hide cursor
- When there's no selection (anchor == focus), show cursor only
- Never show both at the same time

### Bug 4: user-select: none Not Working
**Symptom:** The middle paragraph has CSS `user-select: none` but:
- Selection can still be made on it (or it gets selected when dragging from P1 to P3)
- The CSS property appears to be ignored

**Expected:** Text with `user-select: none` should:
- Not respond to mouse selection
- Be skipped when dragging selection across paragraphs

## Test File: selection.c

```c
{selection_c}
```

## Key CSS

```css
/* Paragraph 1 - Green, selectable */
font-size: 28px; padding: 15px; background-color: #c0ffc0; margin: 8px;

/* Paragraph 2 - Red, NOT selectable */
font-size: 28px; padding: 15px; background-color: #ffc0c0; margin: 8px; user-select: none;

/* Paragraph 3 - Blue, selectable */
font-size: 28px; padding: 15px; background-color: #c0c0ff; margin: 8px;
```

## Questions for Gemini

1. **Bug 1 (Position):** What coordinate spaces are involved in selection rendering?
   - Where should I look to understand how selection rects are positioned?
   - Is padding being applied correctly when converting layout coords to display coords?

2. **Bug 2 (Cursor on non-editable):** 
   - What CSS property or DOM attribute should gate cursor rendering?
   - Where in the code is the decision made to show/hide cursor?

3. **Bug 3 (Cursor + Selection):**
   - Where is the logic that decides between cursor vs selection display?
   - Is `SelectionState` correctly tracking anchor vs focus positions?

4. **Bug 4 (user-select: none):**
   - Where is `user-select` CSS property read and applied?
   - Is hit-testing respecting `user-select: none`?
   - Is selection extension logic checking `user-select`?

## Please Provide

1. Root cause analysis for each bug
2. Specific files and function names to investigate
3. The architectural relationship between:
   - Selection state management
   - Cursor rendering
   - Selection rect rendering
   - Coordinate transformations
4. Priority order for fixing
"""

print(f"Prompt length: {len(prompt)} chars")

# Build Gemini request
parts = []

if screenshot_b64:
    parts.append({"text": "Current Screenshot showing selection bugs:"})
    parts.append({
        "inline_data": {
            "mime_type": "image/png",
            "data": screenshot_b64
        }
    })
else:
    print("WARNING: No screenshot found!")

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
            output_path = ROOT / "scripts/gemini_selection_bugs_response.md"
            output_path.write_text(text)
            print(f"\nSaved to: {output_path}")
        else:
            print(f"Unexpected response: {json.dumps(result, indent=2)}")
            
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
