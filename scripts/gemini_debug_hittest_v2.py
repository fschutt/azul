#!/usr/bin/env python3
"""
Comprehensive context collection script for Gemini debugging.
Collects all relevant files for hit-test, cursor, and selection issues.

Usage:
    python3 scripts/gemini_debug_hittest_v2.py > gemini_context_v2.txt
"""

import os
import sys

# Base directory
BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Files to include with their descriptions
FILES = [
    # === CORE HIT-TEST ARCHITECTURE ===
    ("core/src/hit_test.rs", "Core hit-test structures and algorithms"),
    ("core/src/hit_test_tag.rs", "HitTestTag enum with namespace system (DOM=0x0100, Scrollbar=0x0200)"),
    
    # === CURSOR TYPE RESOLUTION ===
    ("dll/src/desktop/shell2/common/event_v2.rs", "Event processing with CursorTypeHitTest - lines 1111-1645 have DEBUG output"),
    
    # === DISPLAY LIST GENERATION ===
    ("layout/src/solver3/display_list.rs", "Display list builder - push_hit_test_area, paint_inline_shape, paint_inline_content"),
    
    # === TEXT LAYOUT (text3 engine) ===
    ("layout/src/text3/cache.rs", "InlineContent, StyledRun, InlineShape with source_node_id"),
    ("layout/src/text3/glyphs.rs", "GlyphRun structure - NO source_node_id currently"),
    
    # === STYLED DOM & TAG ASSIGNMENT ===
    ("core/src/styled_dom.rs", "StyledNode with tag_id, TagIdToNodeIdMapping"),
    ("core/src/prop_cache.rs", "Tag assignment logic - when nodes get tag_ids (hover, cursor, callbacks, etc.)"),
    
    # === DOM STRUCTURE ===
    ("core/src/dom.rs", "DOM node types, TagId generation"),
    
    # === BUTTON WIDGET ===
    ("layout/src/widgets/button.rs", "Button widget implementation with cursor:pointer"),
    
    # === EXAMPLE BEING DEBUGGED ===
    ("examples/rust/src/hello-world.rs", "Hello-world example with Button"),
    
    # === INLINE CONTENT GENERATION ===
    ("layout/src/solver3/fc.rs", "Formatting context - generates InlineContent with source_node_id"),
    
    # === SELECTION SYSTEM ===
    ("core/src/selection.rs", "Text selection structures"),
    
    # === LAYOUT HIT TEST (CursorTypeHitTest implementation) ===
    ("layout/src/hit_test.rs", "CursorTypeHitTest::new() - depth-based cursor resolution"),
]

def read_file_with_line_numbers(filepath):
    """Read file and add line numbers."""
    try:
        with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
            lines = f.readlines()
        result = []
        for i, line in enumerate(lines, 1):
            result.append(f"{i:5d} | {line.rstrip()}")
        return '\n'.join(result)
    except Exception as e:
        return f"ERROR reading file: {e}"

def main():
    print("=" * 100)
    print("GEMINI DEBUG CONTEXT - HIT-TEST, CURSOR, AND SELECTION ISSUES v2")
    print("=" * 100)
    print()
    
    # Print the detailed problem description and what was tried
    print("=" * 100)
    print("PROBLEM DESCRIPTION AND WHAT WAS TRIED")
    print("=" * 100)
    print("""
## Current Issues

1. **cursor:pointer NOT working on button hover**
   - Button has callbacks (onClick works correctly when clicked)
   - Button has tag_id (verified - callbacks work)
   - But cursor does NOT change to pointer when hovering over button
   
2. **I-beam cursor appears on body instead of text only**
   - Moving mouse over empty body area shows I-beam cursor
   - I-beam should only appear when hovering over actual text content
   
3. **Drag-to-select does NOT work on button text**
   - Cannot select text inside button by dragging
   - Character-by-character selection doesn't work

## What Was Tried (and didn't fully fix the issue)

### Fix 1: Depth-based cursor selection in CursorTypeHitTest
- Modified `CursorTypeHitTest::new()` to find the DEEPEST node with a cursor property
- Before: Was taking first node with cursor (could be parent)
- After: Iterates all hit nodes and picks deepest one
- Result: Partial improvement - more nodes found in hit test, but cursor still wrong

### Fix 2: Text children detection for I-beam cursor
- Added logic to check if a node has actual Text children before showing I-beam
- If node has no text children but cursor:text is set, fall back to default
- Result: Didn't fix the issue - body still shows I-beam

### Fix 3: Hit-test area for inline-block elements (paint_inline_shape)
- Added `builder.push_hit_test_area(border_box_bounds, tag_id)` to `paint_inline_shape()`
- This should make inline-block elements (like buttons) receive mouse events
- Result: Hit test now finds NodeId=0,2,3 instead of just NodeId=0, BUT cursor still wrong

### Fix 4: Scrollbar namespace filtering
- Added check to filter out scrollbar tags (0x0200) from DOM hit-test
- Ensures only DOM nodes (0x0100) are considered for cursor resolution
- Result: Filtering works correctly, not the root cause

## Key Observations from Debug Output

```
hit_test_for_dispatch: 1 DOMs, DOM 0 has 3 nodes - NodeId=0, NodeId=2, NodeId=3
```

This shows that hit-test now finds 3 nodes, but:
- NodeId=0 is body
- NodeId=2 is p element  
- NodeId=3 is button

The button IS being hit, but cursor:pointer is not being applied.

## Suspected Root Causes

1. **Cursor property lookup may be wrong**
   - `get_cursor()` may not be finding the cursor:pointer on the button
   - The CSS cascade for cursor may not be working

2. **Hit-test ordering issue**
   - Even though button is hit, the cursor resolution may be picking wrong node
   - Depth calculation in CursorTypeHitTest may be incorrect

3. **Text runs don't have individual hit-test areas**
   - GlyphRun struct has no source_node_id
   - Text content can't be traced back to DOM nodes for selection
   - This affects drag-to-select functionality

4. **Button's inline-block hit area may not be registered correctly**
   - paint_inline_shape is called, but bounds may be wrong
   - The tag_id lookup may fail for some reason

## Architecture Understanding

### Display List Hit-Test Flow:
1. DOM nodes are styled → StyledDom with tag_ids
2. Layout tree is built → PositionedTree
3. Display list is generated with HitTestArea items
4. WebRender does hit-testing against display list
5. Results come back as (u64, u16) tuples
6. We decode these to HitTestTag::DomNode or HitTestTag::Scrollbar
7. CursorTypeHitTest finds cursor for deepest hit node

### Tag Assignment (prop_cache.rs):
A node gets a tag_id if ANY of these are true:
- Has non-window callbacks (onClick, etc.)
- Has :hover/:active/:focus CSS
- Has non-default cursor property
- Has overflow: scroll/auto
- Has selectable text children

### Text Layout (text3 engine):
- InlineContent::Text(StyledRun) - text runs, NO source_node_id
- InlineContent::Shape(InlineShape) - inline-blocks, HAS source_node_id
- GlyphRun - rendered glyphs, NO source_node_id
- Text runs cannot be traced back to DOM nodes!

## Questions for Analysis

1. Why doesn't cursor:pointer work even though button is in hit-test results?
2. How should text runs get their own hit-test areas for selection?
3. Is the depth calculation in CursorTypeHitTest correct?
4. Why does body show I-beam when it doesn't directly contain text?

## Files Included Below

The following files contain the complete implementation. Please analyze:
- How hit-test areas are generated
- How cursor type is resolved from hit-test results  
- Why text runs don't have hit-test areas
- How to fix the cursor and selection issues
""")
    print()
    
    # Print each file
    for filepath, description in FILES:
        full_path = os.path.join(BASE, filepath)
        print()
        print("=" * 100)
        print(f"FILE: {filepath}")
        print(f"DESCRIPTION: {description}")
        print("=" * 100)
        print()
        
        if os.path.exists(full_path):
            content = read_file_with_line_numbers(full_path)
            print(content)
        else:
            print(f"FILE NOT FOUND: {full_path}")
        
        print()
    
    # Print additional context - the current display list debug output
    print()
    print("=" * 100)
    print("ADDITIONAL DEBUG: Display List Analysis Command")
    print("=" * 100)
    print("""
To get the display list from a running app, use:

curl -s -X POST http://localhost:8765/ -d '{"op": "get_display_list"}' | python3 -c "
import sys,json
d=json.load(sys.stdin)
items=d['data']['value']['items']
print('=== HIT TEST AREAS ===')
for i in items:
    if i.get('type') == 'hit_test_area':
        print(f\"  {i['debug_info']:10} at ({i['x']:6.1f}, {i['y']:5.1f}) size {i['width']:7.2f} x {i['height']:5.2f}\")
"

Expected output should show hit_test_area entries for:
- NodeId=0 (body) - full window size
- NodeId=2 (p element) - paragraph bounds  
- NodeId=3 (button) - button bounds with cursor:pointer
""")

if __name__ == "__main__":
    main()
