#!/usr/bin/env python3
"""
Send 4 remaining issues report to Gemini with screenshot.
"""

import os
import sys
import json
import base64
import subprocess
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

def get_git_diff():
    try:
        result = subprocess.run(
            ["git", "diff", "HEAD~1"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30
        )
        return result.stdout
    except:
        return "[Could not get git diff]"

def read_file(path):
    try:
        return Path(path).read_text()
    except:
        return f"[Could not read {path}]"

# Collect all data
print("Collecting data...")

# Current screenshot
screenshot_path = ROOT / "target/test_results/after-diff-2/image.png"
screenshot_b64 = encode_image(screenshot_path)

# Git diff of our fixes
git_diff = get_git_diff()

# Test file
test_file = read_file(ROOT / "tests/e2e/text_area.c")

# Build the prompt
prompt = f"""# TextArea Bug Report: 4 Remaining Issues

## Summary of Fixed Issues

We successfully fixed:
✅ Scrollbar now appears with correct size
✅ Scrolling works correctly  
✅ Container width respects CSS min-width: 600px constraint
✅ Text doesn't soft-wrap (correct for white-space: pre)

## 4 Remaining Issues to Debug

### Issue 1: Content Clipping / Display List Truncation
**Symptom:** Only ~3 lines are rendered to the display list. Text ends after "Third line with more" and doesn't layout beyond that.

**Expected:** All 15 lines should be in the display list and rendered (scrollable).

**Hypothesis:** The text layout engine generates all 15 lines, but something is clipping/filtering items when pushing to the display list.

### Issue 2: Cursor Artifacts
**Symptom:** Cursors show as "rectangles" or weird visual artifacts instead of proper thin cursor lines.

**Expected:** Cursor should be a thin vertical line at text insertion point.

### Issue 3: Selection/Cursor Position Errors  
**Symptom:** In selection.c test and selection.sh script, selection rectangles and cursors appear at wrong positions.

**Expected:** Selection rects and cursors should align with the actual text positions.

**Related:** This may be a coordinate space transformation issue between layout coordinates and display coordinates.

### Issue 4: No Text Input/Editing
**Symptom:** No text input is possible, no on_change or text input changeset gets triggered.

**Expected:** User should be able to type text, and callbacks should fire.

## Current State

Screenshot shows:
- Scrollbar visible and functional ✓
- Container width correct ✓
- Only first 3 lines visible (Issue 1)
- Cursor/selection issues visible (Issues 2, 3)

## Our Recent Fixes (Git Diff)

```diff
{git_diff[:8000]}
```

## Test File: text_area.c

```c
{test_file}
```

## Key Architecture Points

1. **Text Layout Pipeline:**
   - `collect_and_measure_inline_content()` collects InlineContent items
   - `split_text_for_whitespace()` splits by \\n for white-space: pre
   - `layout_flow()` creates UnifiedLayout with all positioned items
   - `paint_inline_content()` converts layout to display list items

2. **Display List Building:**
   - `paint_node()` calls `paint_inline_content()` for nodes with `inline_layout_result`
   - `paint_inline_content()` iterates over layout.items and calls `push_text_run()`
   - Items may be clipped by scroll frame clip bounds

3. **Scroll Frame:**
   - `PushScrollFrame` establishes clip bounds (200px height)
   - Content inside should be laid out with full height
   - Display list items should still be generated for all content

## Analysis Questions

1. **Issue 1 (Clipping):** Where is the display list being limited to only 3 lines?
   - Is `paint_inline_content()` receiving a truncated layout?
   - Is there clipping happening in the display list builder?
   - Is the scroll frame's clip rect being applied during layout instead of rendering?

2. **Issue 2 (Cursor Artifacts):** 
   - Where are cursor rectangles generated?
   - What determines cursor width/height?
   - Are cursor bounds being calculated correctly?

3. **Issue 3 (Position Errors):**
   - What coordinate transformation is applied to selection rects?
   - Is there an offset being missed (scroll offset, container offset)?
   - Are glyphs positioned correctly but selections using wrong baseline?

4. **Issue 4 (No Input):**
   - What events need to be registered for text input?
   - Is the TextArea receiving focus?
   - Are virtual key events being processed?

## Please Provide

1. Root cause analysis for each of the 4 issues
2. Specific file and line numbers to investigate
3. Code fixes in diff format for the most critical issues
4. Priority order for fixing (which issue should be tackled first)
"""

print(f"Prompt length: {len(prompt)} chars")

# Build Gemini request
parts = []

if screenshot_b64:
    parts.append({"text": "Current Screenshot (after our fixes - scrollbar works, but 4 remaining issues):"})
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
            output_path = ROOT / "scripts/gemini_4_issues_response.md"
            output_path.write_text(text)
            print(f"\nSaved to: {output_path}")
        else:
            print(f"Unexpected response: {json.dumps(result, indent=2)}")
            
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
