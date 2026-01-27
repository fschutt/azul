#!/usr/bin/env python3
"""
Send bug diff analysis to Gemini with before/after screenshots.
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

def read_file(path):
    try:
        return Path(path).read_text()
    except:
        return f"[Could not read {path}]"

def encode_image(path):
    try:
        with open(path, "rb") as f:
            return base64.standard_b64encode(f.read()).decode("utf-8")
    except:
        return None

def get_git_diff():
    try:
        result = subprocess.run(
            ["git", "diff", "HEAD"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30
        )
        return result.stdout
    except:
        return "[Could not get git diff]"

# Collect all data
print("Collecting data...")

# Screenshots
before_screenshot = ROOT / "target/test_results/image.png"
after_screenshot = ROOT / "target/test_results/after-diff/image.png"

before_b64 = encode_image(before_screenshot)
after_b64 = encode_image(after_screenshot)

# Git diff
git_diff = get_git_diff()

# Test file
test_file = read_file(ROOT / "tests/e2e/text_area.c")

# Key source files (relevant excerpts)
fc_rs = read_file(ROOT / "layout/src/solver3/fc.rs")
sizing_rs = read_file(ROOT / "layout/src/solver3/sizing.rs")
cache_rs = read_file(ROOT / "layout/src/text3/cache.rs")

# Build the prompt
prompt = f"""# Bug Diff Analysis: TextArea Scrolling

## Problem Description

We're debugging a multi-line TextArea with the following setup:
- 15 lines of text
- font-size: 36px, line-height: 1.4
- Container height: 200px (fixed)
- CSS: `white-space: pre; overflow-y: auto;`

**Expected behavior:** All 15 lines should be laid out, scrollbar should appear, content_size.height should be ~756px.

## Current State After Our Changes

We applied a fix that:
1. Added `split_text_for_whitespace()` to split text by `\\n` for `white-space: pre`
2. Set `available_height: None` for scrollable containers (overflow: scroll/auto) 
3. Made `TextWrap::NoWrap` use unlimited width (`f32::MAX / 2.0`)

**Result of the fix (NEW BUGS):**
1. The scrollbar now appears ✓
2. BUT the container WIDTH expands beyond what CSS should allow (?)
3. BUT only 4 lines are visible/pushed to display list instead of 15
4. Text no longer wraps within lines (correct for `white-space: pre`)

## Before Screenshot (OLD STATE)
- No scrollbar visible
- Text wraps within container width (incorrect for `white-space: pre`)
- 4 lines visible but they are wrapped versions of fewer logical lines

## After Screenshot (NEW STATE) 
- Scrollbar visible ✓
- Text does NOT wrap (correct for `white-space: pre`) ✓
- Container width expanded beyond expected bounds (BUG?)
- Only 4 lines rendered to display list (BUG - should be 15)

## Key Question

Why does setting `available_height: None` (unlimited) for scroll containers cause:
1. The WIDTH to also expand?
2. Only 4 items to be pushed to the display list instead of all 15?

The set of bugs changed after our fix - please analyze what conceptual problem we're missing.

## Git Diff (Our Changes)

```diff
{git_diff}
```

## Test File: text_area.c

```c
{test_file}
```

## Relevant CSS from text_area.c

```css
.textarea {{
  font-size: 36px;
  font-family: monospace;
  padding: 15px;
  background-color: #2d2d2d;
  color: #ffffff;
  border: 3px solid #555555;
  min-width: 600px;
  height: 200px;
  overflow-y: auto;
  overflow-x: auto;
  white-space: pre;
  line-height: 1.4;
  cursor: text;
}}
```

## Key Code: translate_to_text3_constraints (fc.rs)

The function that creates UnifiedConstraints for the text layout engine.
Our change sets `available_height: None` for scroll containers:

```rust
available_height: match overflow_behaviour {{
    LayoutOverflow::Scroll | LayoutOverflow::Auto => None,
    _ => Some(constraints.available_size.height),
}},
```

## Key Code: get_line_constraints (cache.rs)

Our change makes NoWrap use unlimited width:

```rust
let segment_width = if constraints.text_wrap == TextWrap::NoWrap {{
    // NoWrap: Text should only break at explicit line breaks, not soft wrap
    f32::MAX / 2.0
}} else {{
    match constraints.available_width {{
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MaxContent => f32::MAX / 2.0,
        AvailableSpace::MinContent => 0.0,
    }}
}};
```

## Key Code: Height limiting logic (cache.rs line ~5490)

```rust
if let Some(max_height) = fragment_constraints.available_height {{
    if line_top_y >= max_height {{
        // Column full, break to next column
        break;
    }}
}}
```

With `available_height: None`, this check is skipped, so all lines should be laid out.

## Analysis Questions

1. Why would `available_height: None` cause the WIDTH to expand?
2. Is there a separate constraint for the scroll frame's clip rect vs content rect?
3. Where is the display list being clipped to only show 4 items?
4. Is the problem that we're measuring intrinsic size with unlimited dimensions, but then the final layout uses different constraints?
5. Could there be TWO layout passes - one for intrinsic sizing and one for final layout - and we're only fixing one?

Please provide:
1. Root cause analysis
2. Specific file and line numbers to fix
3. Code fix in diff format
"""

print(f"Prompt length: {len(prompt)} chars")

# Build Gemini request
parts = [{"text": prompt}]

if before_b64:
    parts.insert(0, {
        "inline_data": {
            "mime_type": "image/png",
            "data": before_b64
        }
    })
    parts.insert(1, {"text": "BEFORE Screenshot (OLD STATE - no scrollbar, text wraps):"})

if after_b64:
    parts.append({"text": "AFTER Screenshot (NEW STATE - scrollbar visible, width expanded, only 4 lines):"})
    parts.append({
        "inline_data": {
            "mime_type": "image/png",
            "data": after_b64
        }
    })

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
    with urllib.request.urlopen(req, timeout=120) as response:
        result = json.loads(response.read().decode("utf-8"))
        
        if "candidates" in result and result["candidates"]:
            text = result["candidates"][0]["content"]["parts"][0]["text"]
            print("\n" + "="*60)
            print("GEMINI RESPONSE:")
            print("="*60)
            print(text)
            
            # Save response
            output_path = ROOT / "target/textarea_debug/gemini_diff_response.md"
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_text(text)
            print(f"\nSaved to: {output_path}")
        else:
            print(f"Unexpected response: {json.dumps(result, indent=2)}")
            
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
