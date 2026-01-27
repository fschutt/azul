#!/usr/bin/env python3
"""
Gemini Query: Cursor/Text Layout Architecture

This script sends a focused question to Gemini about the cursor/text layout
architecture, specifically around the IFC-Root vs Text-Node distinction.

Problem: 
- Text rendering works (uses inline_layout_result on IFC-Root container node)
- Cursor positioning fails (searches for inline_layout_result on text node)
- There are separate code paths that need to be unified

Question:
- How does W3C model Selection/Range vs DOM nodes for contenteditable?
- What is the correct relationship between container node and text node?
- How should cursor position be stored and retrieved?
"""
import json
import requests
import subprocess
import sys
from pathlib import Path
from datetime import datetime

ROOT = Path(__file__).parent.parent
API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()

GEMINI_API_URL = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"

def read_file_safe(path: Path) -> str:
    try:
        return path.read_text()
    except Exception as e:
        return f"# Error reading {path}: {e}\n"

def collect_source_files() -> list[tuple[str, str]]:
    """Collect relevant source files for this specific problem"""
    
    file_patterns = [
        # Core problem files - IFC/text layout architecture
        "layout/src/solver3/layout_tree.rs",        # IfcMembership, inline_layout_result structure
        "layout/src/solver3/display_list.rs",       # paint_inline_content, paint_cursor, paint_selections
        "layout/src/solver3/fc.rs",                 # IFC layout computation
        
        # Cursor/selection managers
        "layout/src/managers/cursor.rs",
        "layout/src/managers/selection.rs",
        "layout/src/managers/focus_cursor.rs",
        
        # Window integration
        "layout/src/window.rs",
        
        # Text layout
        "layout/src/text3/mod.rs",
        "layout/src/text3/selection.rs",
        
        # Core types
        "core/src/selection.rs",
        "core/src/dom.rs",
        
        # Test case
        "tests/e2e/contenteditable.c",
    ]
    
    result = []
    for rel_path in file_patterns:
        full_path = ROOT / rel_path
        if full_path.exists():
            content = read_file_safe(full_path)
            result.append((rel_path, content))
    
    return result

def build_prompt() -> str:
    """Build the specific prompt about cursor/text layout"""
    
    prompt = """# Gemini Review: Cursor vs Text Layout Architecture

## 1. The Problem

We have a contenteditable implementation where:

1. **Text RENDERING works** - The display list correctly renders text
2. **Cursor POSITIONING fails** - `get_inline_layout_for_node(text_node_id)` returns `None`

### Root Cause Analysis

Looking at our layout tree architecture:

```
DOM:                           Layout Tree:
<div contenteditable>          LayoutNode (div) - IFC Root
  ::text "Hello World"           └── inline_layout_result: Some(UnifiedLayout)
</div>                           └── ifc_id: IfcId(5)
                               
                               LayoutNode (::text)
                                 └── inline_layout_result: None
                                 └── ifc_membership: Some(IfcMembership { 
                                       ifc_id: 5, 
                                       ifc_root_layout_index: 0,
                                       run_index: 0 
                                     })
```

The `inline_layout_result` (containing text positions, glyph info) is stored on the **IFC-Root node** (the container `<div>`), NOT on the text node itself. This is by design - the IFC root owns the entire inline formatting context.

### Where Code Paths Diverge

**Text Rendering** (`paint_node` in display_list.rs):
- Iterates through all layout nodes
- If `node.inline_layout_result` exists, calls `paint_inline_content(layout)`
- This works because it finds the layout on the container node (IFC root)

**Cursor Positioning** (`get_inline_layout_for_node` in window.rs):
- Takes a specific `node_id` (the text node)
- Looks for `inline_layout_result` on that specific node
- Returns `None` because text nodes don't have their own `inline_layout_result`

**Selection Painting** (`paint_selections` in display_list.rs):
- Also looks for `inline_layout_result` on the node
- Has the same problem as cursor positioning

## 2. Current W3C Model Understanding

In the W3C DOM/Selection model:

1. **Focus (`document.activeElement`)**: Points to the contenteditable container element
2. **Selection.anchorNode / focusNode**: Points to the TEXT NODE containing the caret
3. **Selection.anchorOffset / focusOffset**: Character offset within that text node

So the W3C model DOES distinguish between:
- The focused element (container)
- The caret position (text node + offset)

### Our Implementation

We have:
- `FocusManager.focused_node` = container node (correct)
- `CursorManager.cursor_location` = `(DomId, NodeId, TextCursor)` where NodeId is the TEXT node

But when we try to get the layout for cursor positioning:
```rust
let text_layout = self.get_inline_layout_for_node(dom_id, text_node_id);
// Returns None because text_node has no inline_layout_result!
```

## 3. Questions for Gemini

### Question 1: W3C Selection Model vs DOM Structure

In the W3C model, when placing a caret in contenteditable:
```html
<div contenteditable>Hello World</div>
```

- Does `Selection.focusNode` point to the TEXT NODE or the DIV element?
- Does the browser internally store selection on the text node or the container?
- How does the browser find the text layout (glyph positions) to position the cursor?

### Question 2: Our Architecture Decision

Should we:

**Option A**: Store `cursor_location` as the CONTAINER node, not the text node
- Pro: inline_layout_result is directly accessible
- Con: Doesn't match W3C where Selection points to text node

**Option B**: Keep `cursor_location` as the text node, but navigate to IFC root for layout
- Pro: Matches W3C Selection model
- Con: Requires extra lookup via `ifc_membership`

**Option C**: Something else?

### Question 3: paint_selections Inconsistency

Currently `paint_selections` iterates through layout nodes and checks:
```rust
let Some(cached_layout) = &node.inline_layout_result else { return Ok(()); };
```

But `ctx.text_selections` is keyed by the TEXT NODE id, not the IFC root.
This seems like a mismatch. Should:
- Selections be keyed by IFC root node?
- Or should we navigate from text node to IFC root during paint?

### Question 4: Unified Cursor/Selection/Text-Rendering Path

All three operations need the same data:
1. Text rendering: glyph positions from `inline_layout_result`
2. Cursor painting: caret position from `inline_layout_result`
3. Selection painting: highlight rects from `inline_layout_result`

Should there be a unified way to:
- Store which node has cursor/selection
- Retrieve the layout for that node (handling IFC membership)
- Paint cursor/selection correctly

## 4. Proposed Fix

I've already added code to `get_inline_layout_for_node` to check `ifc_membership`:

```rust
fn get_inline_layout_for_node(&self, dom_id: DomId, node_id: NodeId) -> Option<&Arc<UnifiedLayout>> {
    // ... lookup layout_node ...
    
    // First, check if this node has its own inline_layout_result
    if let Some(cached) = &layout_node.inline_layout_result {
        return Some(cached.get_layout());
    }
    
    // For text nodes, check if they have ifc_membership pointing to the IFC root
    if let Some(ifc_membership) = &layout_node.ifc_membership {
        let ifc_root_node = layout_result.layout_tree.nodes.get(ifc_membership.ifc_root_layout_index)?;
        if let Some(cached) = &ifc_root_node.inline_layout_result {
            return Some(cached.get_layout());
        }
    }
    
    None
}
```

**Is this the correct approach?** Are there edge cases I'm missing?

## 5. Debug Output

Before the fix:
```
[DEBUG] text_layout available: false
[DEBUG] Cursor initialized: false
```

After the fix:
```
[DEBUG] text_layout available: true  
[DEBUG] Cursor initialized: true, cursor=Some(TextCursor { cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 33 }, affinity: Trailing })
```

But the cursor still doesn't appear in the display list. There may be additional issues in `paint_cursor`.

## 6. Source Code

Please review the following source files for architecture correctness:

"""
    return prompt

def main():
    print("=" * 60)
    print("Gemini Query: Cursor/Text Layout Architecture")
    print("=" * 60)
    
    # Collect source files
    print("\nCollecting source files...")
    files = collect_source_files()
    
    # Build source dump
    source_dump = ""
    total_lines = 0
    for rel_path, content in files:
        lines = content.count('\n') + 1
        total_lines += lines
        source_dump += f"\n{'='*80}\n"
        source_dump += f"FILE: {rel_path}\n"
        source_dump += f"LINES: {lines}\n"
        source_dump += f"{'='*80}\n\n"
        source_dump += content
        source_dump += "\n"
        print(f"  {rel_path}: {lines} lines (total: {total_lines})")
    
    # Build full prompt
    prompt = build_prompt()
    full_text = prompt + "\n\n# SOURCE CODE\n\n" + source_dump
    
    print(f"\nTotal lines: {total_lines}")
    print(f"Prompt + source: {len(full_text)} chars (~{len(full_text)//4} tokens)")
    
    # Save prompt for reference
    prompt_path = ROOT / "scripts/gemini_cursor_text_layout_prompt.md"
    prompt_path.write_text(prompt + "\n\n[Source code follows in API request]")
    print(f"\nPrompt saved to: {prompt_path}")
    
    # Send to Gemini
    print("\nSending to Gemini API...")
    
    payload = {
        "contents": [{
            "role": "user",
            "parts": [{"text": full_text}]
        }],
        "generationConfig": {
            "temperature": 0.7,
            "maxOutputTokens": 65536,
        }
    }
    
    url = f"{GEMINI_API_URL}?key={API_KEY}"
    
    response = requests.post(
        url,
        headers={"Content-Type": "application/json"},
        json=payload,
        timeout=600
    )
    
    if response.status_code != 200:
        print(f"Error {response.status_code}: {response.text}")
        return 1
    
    result = response.json()
    
    try:
        response_text = result["candidates"][0]["content"]["parts"][0]["text"]
    except (KeyError, IndexError):
        response_text = json.dumps(result, indent=2)
    
    # Save response
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    response_path = ROOT / f"scripts/gemini_cursor_text_layout_response_{timestamp}.md"
    response_path.write_text(f"# Gemini Response: Cursor/Text Layout Architecture\n\n{response_text}")
    print(f"\nResponse saved to: {response_path}")
    
    # Also save to latest
    latest_path = ROOT / "scripts/gemini_cursor_text_layout_response_latest.md"
    latest_path.write_text(f"# Gemini Response: Cursor/Text Layout Architecture\n\n{response_text}")
    print(f"Also saved to: {latest_path}")
    
    # Print response
    print("\n" + "=" * 60)
    print("RESPONSE:")
    print("=" * 60)
    print(response_text[:6000] + ("..." if len(response_text) > 6000 else ""))
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
