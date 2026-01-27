#!/usr/bin/env python3
"""
Ask Gemini about selection bugs in the text selection test.
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
        print(f"Warning: Could not encode image {path}: {e}")
        return None

def read_file(path):
    try:
        return Path(path).read_text()
    except:
        return f"[Could not read {path}]"

# Screenshot
screenshot_path = ROOT / "target/test_results/selection/image.png"
screenshot_b64 = encode_image(screenshot_path)

# Test file
selection_c = read_file(ROOT / "tests/e2e/selection.c")

prompt = f"""# Text Selection Bug Report - 4 Issues

## Test Setup

We have a selection.c test with 3 paragraphs:
1. **Paragraph 1** (green background) - selectable text
2. **Paragraph 2** (red/pink background) - `user-select: none` - should NOT be selectable
3. **Paragraph 3** (blue/purple background) - selectable text

CSS for each paragraph:
```css
/* Paragraph 1 */
font-size: 28px; padding: 15px; background-color: #c0ffc0; margin: 8px;

/* Paragraph 2 - should NOT be selectable */
font-size: 28px; padding: 15px; background-color: #ffc0c0; margin: 8px; user-select: none;

/* Paragraph 3 */
font-size: 28px; padding: 15px; background-color: #c0c0ff; margin: 8px;
```

## Screenshot Analysis

Looking at the attached screenshot, we can see:
- Blue selection rectangles are visible
- A cursor (caret) is visible
- Selection seems misaligned with actual text

## 4 Bugs to Debug

### Bug 1: Selection Rectangles at Wrong Position
**Symptom:** The blue selection rectangles appear offset from the actual text. They seem to not respect padding.

**Expected:** Selection rectangles should perfectly overlay the selected text glyphs.

**Questions:**
- Is padding being included in the selection rect calculation?
- Are selection rects in content-box coordinates but being rendered in border-box coordinates?
- Is there a coordinate space mismatch between layout and rendering?

### Bug 2: Cursor Visible on Non-Editable Content
**Symptom:** A cursor (text caret) is visible even though NONE of the paragraphs are contenteditable.

**Expected:** Cursor should only appear on:
- `<input>` elements
- `<textarea>` elements  
- Elements with `contenteditable="true"`

Regular text paragraphs should NOT show a cursor.

**Questions:**
- Where is the cursor rendering decision made?
- Is there a check for `contenteditable` or `is_text_input`?
- What triggers cursor display?

### Bug 3: Cursor AND Selection Visible Simultaneously
**Symptom:** Both cursor (caret) and selection rectangles are visible at the same time.

**Expected:** 
- If user is selecting text → show selection rectangles, NO cursor
- If user has focus without selection → show cursor, NO selection
- Never both at the same time

**Questions:**
- What is the selection state model?
- Is there a single selection state or separate cursor/selection states?
- How should these states be mutually exclusive?

### Bug 4: user-select: none Not Working
**Symptom:** The second paragraph has `user-select: none` but selection still appears to include it.

**Expected:** 
- Paragraph 2 should be skipped during selection
- Selection should "jump" from end of P1 to start of P3
- No selection highlight should appear on P2

**Questions:**
- Where is `user-select` CSS property checked during selection?
- Is the selection algorithm respecting this property?
- How do other browsers implement multi-paragraph selection with user-select: none in the middle?

## Source Code: selection.c

```c
{selection_c}
```

## Architecture Questions

1. **Selection State Model:**
   - How is selection represented? (start node + offset, end node + offset?)
   - Is there a `SelectionRange` struct?
   - Where is selection state stored?

2. **Selection Rendering:**
   - How are selection rectangles calculated?
   - What coordinate space are they in?
   - How is the cursor position calculated?

3. **User-Select Property:**
   - Where is CSS `user-select` parsed?
   - How is it stored in the styled DOM?
   - Where is it checked during hit-testing or selection?

4. **Contenteditable Detection:**
   - How does the system know if a node is editable?
   - What property/flag indicates editability?
   - How does this affect cursor display?

## Please Provide

1. **Root cause analysis** for each of the 4 bugs
2. **File and line number locations** to investigate
3. **Priority order** for fixing these bugs
4. **Code structure overview** - where is selection logic implemented?
5. **Specific code changes** needed for the most critical bugs
"""

print(f"Prompt length: {len(prompt)} chars")

# Build Gemini request
parts = []

if screenshot_b64:
    parts.append({"text": "Screenshot of selection.c test showing the 4 bugs:"})
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
            output_path = ROOT / "scripts/gemini_selection_response.md"
            output_path.write_text(text)
            print(f"\nSaved to: {output_path}")
        else:
            print(f"Unexpected response: {json.dumps(result, indent=2)}")
            
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
