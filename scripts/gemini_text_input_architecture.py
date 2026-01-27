#!/usr/bin/env python3
"""
Text Input Architecture Review - Gemini Request

This script collects ~150k lines of source code to send to Gemini for
architectural review of text input, cursor positioning, and focus management.

Problems to solve:
1. Text input in contenteditable doesn't update the DOM/display
2. Cursor doesn't reposition on clicking in contenteditable text
3. Cursor doesn't move with text input (can't verify because text isn't changing)
4. Clicking on another text input doesn't properly transfer focus/cursor

The goal is to get a comprehensive architectural review and implementation plan.
"""
import json
import requests
import subprocess
import sys
from pathlib import Path
from datetime import datetime

ROOT = Path(__file__).parent.parent
API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()

# Gemini API
GEMINI_API_URL = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"

# Target: 150k lines of source code
TARGET_LINES = 150_000

def read_file_safe(path: Path) -> str:
    """Read file, return empty string if not found"""
    try:
        return path.read_text()
    except Exception as e:
        return f"# Error reading {path}: {e}\n"

def count_lines(content: str) -> int:
    return content.count('\n') + 1

def get_git_diff() -> str:
    """Get the current git diff"""
    result = subprocess.run(
        ["git", "diff", "HEAD~3"],  # Last 3 commits for context
        cwd=ROOT,
        capture_output=True,
        text=True
    )
    return result.stdout

def collect_source_files() -> list[tuple[str, str]]:
    """Collect source files in priority order for text input review"""
    
    # Priority order - focus on text input, cursor, selection, focus
    file_patterns = [
        # ============ TEXT EDITING (HIGHEST PRIORITY) ============
        "layout/src/text3/edit.rs",           # Text editing operations
        "layout/src/text3/selection.rs",      # Selection handling
        "layout/src/text3/cache.rs",          # Text cache (inline content storage)
        "layout/src/text3/mod.rs",            # Text module overview
        
        # ============ MANAGERS ============
        "layout/src/managers/text_input.rs",  # TextInputManager - changesets
        "layout/src/managers/cursor.rs",      # CursorManager - cursor positioning
        "layout/src/managers/selection.rs",   # SelectionManager
        "layout/src/managers/focus_cursor.rs",# FocusManager
        "layout/src/managers/undo_redo.rs",   # Undo/redo for text operations
        "layout/src/managers/changeset.rs",   # Text changesets
        "layout/src/managers/clipboard.rs",   # Clipboard operations
        "layout/src/managers/mod.rs",         # Manager module overview
        
        # ============ WINDOW (CORE COORDINATION) ============
        "layout/src/window.rs",               # Central coordination
        
        # ============ EVENT HANDLING ============
        "dll/src/desktop/shell2/common/event_v2.rs",  # Event processing
        "core/src/events.rs",                 # Event types
        
        # ============ CALLBACKS ============
        "layout/src/callbacks.rs",
        "layout/src/default_actions.rs",
        "core/src/callbacks.rs",
        
        # ============ HIT TESTING ============
        "core/src/hit_test.rs",
        "layout/src/hit_test.rs",
        
        # ============ CORE TYPES ============
        "core/src/dom.rs",                    # DOM types, NodeType::Text
        "core/src/selection.rs",              # Selection types
        "core/src/styled_dom.rs",             # StyledDom structure
        
        # ============ DISPLAY LIST ============
        "layout/src/solver3/display_list.rs", # Cursor/selection painting
        "layout/src/solver3/getters.rs",      # is_node_contenteditable, caret style
        
        # ============ PLATFORM (macOS for reference) ============
        "dll/src/desktop/shell2/macos/events.rs",
        
        # ============ TESTS ============
        "tests/e2e/contenteditable.c",
        
        # ============ PREVIOUS ANALYSIS ============
        "scripts/gemini_w3c_response_latest.md",
    ]
    
    result = []
    for rel_path in file_patterns:
        full_path = ROOT / rel_path
        if full_path.exists():
            content = read_file_safe(full_path)
            result.append((rel_path, content))
    
    return result

def build_prompt() -> str:
    """Build the comprehensive architecture review prompt"""
    
    prompt = """# Text Input Architecture Review: Complete Implementation Plan

## Executive Summary

We need a comprehensive architectural review and implementation plan for text input
in contenteditable elements. Several interconnected problems need to be solved together.

## Current Problems

### Problem 1: Text Input Doesn't Update Display
**Symptom:** When typing in a contenteditable div, no characters appear on screen.

**Root Cause Found:** The `update_text_cache_after_edit()` function is a TODO stub:
```rust
pub fn update_text_cache_after_edit(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    new_inline_content: Vec<InlineContent>,
) {
    // TODO: Update the text cache with the new inline content
    let _ = (dom_id, node_id, new_inline_content);
}
```

The text is being transformed via `edit_text()` in `text3/edit.rs`, but never persisted.

### Problem 2: Cursor Doesn't Reposition on Click
**Symptom:** Clicking in contenteditable text doesn't move the cursor to the click position.

**Likely Cause:** Hit testing returns the container node, but we need to:
1. Hit test to find which text cluster was clicked
2. Calculate the cursor position within that cluster
3. Update CursorManager with the new position

### Problem 3: Focus Transfer Between Inputs
**Symptom:** Clicking on a second contenteditable doesn't properly transfer focus/cursor.

**Likely Cause:** Focus is being set, but cursor initialization for the new element
isn't happening correctly.

### Problem 4: Cursor Doesn't Move with Text Input
**Symptom:** Can't verify cursor movement because text isn't updating.

**Related to Problem 1:** Once text updates work, cursor should advance via
the `new_selections` returned from `edit_text()`.

## Architecture Requirements

### 1. Text Update Pipeline

When the user types a character:

```
Platform (macOS/Windows/Linux)
    ↓ record_text_input(text)
    ↓
TextInputManager.record_input()
    ↓ stores changeset
    ↓
Event Processing (event_v2.rs)
    ↓ fires On::TextInput callback (if registered)
    ↓ if !preventDefault
    ↓
apply_text_changeset()
    ↓ calls edit_text() to get new InlineContent
    ↓ calls update_text_cache_after_edit() <-- BROKEN
    ↓ updates CursorManager position
    ↓
Relayout + Repaint
    ↓
Display shows new text + cursor
```

### 2. Text Storage Architecture

Current (broken):
- Text is stored in `StyledDom.node_data[node_id].node_type = NodeType::Text(AzString)`
- `get_text_before_textinput()` reads from this
- But `update_text_cache_after_edit()` never writes back

Options:
A) **Mutate StyledDom directly** - Modify the NodeType::Text content
B) **Maintain shadow text cache** - Store edited text in a separate HashMap
C) **Use InlineContent as source of truth** - Store the shaped text, regenerate on layout

### 3. Callback Architecture for Text Input

W3C-like model for user callbacks:

```
beforeinput event (can preventDefault)
    ↓
if !defaultPrevented:
    apply text change
    ↓
input event (informational, after change applied)
```

User should be able to:
- Intercept text before it's applied
- Modify or reject the input
- Read the current text content afterward

### 4. Cursor Click Positioning

When user clicks in contenteditable:

```
Mouse Click
    ↓
Hit Test → find node under cursor
    ↓
If node has inline_layout_result:
    ↓
    hit_test_text_at_point(click_position)
    ↓ returns TextCursor { cluster_id, affinity }
    ↓
CursorManager.move_cursor_to(cursor)
    ↓
Repaint cursor at new position
```

### 5. Focus Transfer

When focus changes between contenteditables:

```
Click on Element B (while Element A is focused)
    ↓
FocusManager.set_focus(B)
    ↓
Stop cursor blink timer for A
Start cursor blink timer for B
    ↓
Initialize cursor in B:
    - If selection exists: use it
    - Else: place at end of text (or start, based on click position)
    ↓
CursorManager.cursor_location = (dom_id, B, cursor)
```

## Questions for Gemini

### Q1: What is the correct architecture for text storage?

Should we:
A) Mutate `NodeType::Text` in StyledDom directly
B) Keep a separate `HashMap<(DomId, NodeId), String>` for edited text
C) Use InlineContent and rebuild during layout
D) Some other approach

Consider:
- Undo/redo needs to work
- Layout needs to see the updated text
- Callbacks need to be able to read current text
- We need to handle multi-node selections (delete across nodes)

### Q2: How should `update_text_cache_after_edit()` work?

Provide the full implementation. It needs to:
- Update text so `get_text_before_textinput()` returns new text
- Trigger relayout of the affected node
- Handle the case where text is deleted entirely

### Q3: How should cursor click positioning work?

The `get_cursor_rect()` function exists to convert cursor → rect.
We need the inverse: point → cursor.

Provide implementation for:
```rust
fn hit_test_text_at_point(
    &self,
    layout: &ShapedLayout,
    point: LogicalPosition,
) -> Option<TextCursor>
```

### Q4: How should focus transfer work?

When clicking from one contenteditable to another:
1. What order do events fire?
2. How is the old cursor cleaned up?
3. How is the new cursor initialized?
4. How does this interact with the blink timer?

### Q5: Complete Implementation Plan

Provide a step-by-step implementation plan with:
1. Order of changes (dependencies)
2. Specific functions to add/modify
3. Test cases to verify each step
4. Potential edge cases

## Source Code Reference

The following source files are provided for analysis.
Pay special attention to:
- `layout/src/text3/edit.rs` - the edit_text function
- `layout/src/window.rs` - apply_text_changeset and update_text_cache_after_edit
- `layout/src/managers/text_input.rs` - TextInputManager
- `layout/src/managers/cursor.rs` - CursorManager
- `layout/src/text3/cache.rs` - get_cursor_rect and text layout

"""
    return prompt

def dump_source_files(output_path: Path) -> int:
    """Dump source files to a single text file, return total lines"""
    
    print("Collecting source files...")
    files = collect_source_files()
    
    total_lines = 0
    with open(output_path, 'w') as f:
        for rel_path, content in files:
            lines = count_lines(content)
            total_lines += lines
            
            f.write(f"\n{'='*80}\n")
            f.write(f"FILE: {rel_path}\n")
            f.write(f"LINES: {lines}\n")
            f.write(f"{'='*80}\n\n")
            f.write(content)
            f.write("\n")
            
            print(f"  {rel_path}: {lines} lines (total: {total_lines})")
            
            if total_lines >= TARGET_LINES:
                print(f"\nReached target of {TARGET_LINES} lines")
                break
    
    print(f"\nTotal lines dumped: {total_lines}")
    return total_lines

def send_to_gemini(prompt: str, source_dump: str) -> str:
    """Send prompt + source dump to Gemini API"""
    
    full_text = prompt + "\n\n# SOURCE CODE DUMP\n\n" + source_dump
    
    payload = {
        "contents": [{
            "role": "user",
            "parts": [{
                "text": full_text
            }]
        }],
        "generationConfig": {
            "temperature": 0.7,
            "maxOutputTokens": 65536,
        }
    }
    
    url = f"{GEMINI_API_URL}?key={API_KEY}"
    
    print(f"\nSending to Gemini API ({len(full_text)} chars, ~{len(full_text)//4} tokens)...")
    response = requests.post(
        url,
        headers={"Content-Type": "application/json"},
        json=payload,
        timeout=900  # 15 minute timeout
    )
    
    if response.status_code != 200:
        return f"Error {response.status_code}: {response.text}"
    
    result = response.json()
    
    # Extract response text
    try:
        return result["candidates"][0]["content"]["parts"][0]["text"]
    except (KeyError, IndexError):
        return json.dumps(result, indent=2)

def main():
    print("=" * 60)
    print("Text Input Architecture Review")
    print("=" * 60)
    
    # Step 1: Dump source files
    dump_path = ROOT / "scripts/text_input_architecture_source_dump.txt"
    total_lines = dump_source_files(dump_path)
    print(f"\nSource dump saved to: {dump_path}")
    print(f"Total lines: {total_lines}")
    
    # Step 2: Build prompt
    prompt = build_prompt()
    prompt_path = ROOT / "scripts/text_input_architecture_prompt.md"
    prompt_path.write_text(prompt)
    print(f"\nPrompt saved to: {prompt_path}")
    
    # Step 3: Read source dump
    source_dump = dump_path.read_text()
    
    # Step 4: Send to Gemini
    response = send_to_gemini(prompt, source_dump)
    
    # Step 5: Save response
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    response_path = ROOT / f"scripts/gemini_text_input_response_{timestamp}.md"
    response_path.write_text(f"# Gemini Text Input Architecture Response\n\n{response}")
    print(f"\nResponse saved to: {response_path}")
    
    # Also save to latest
    latest_path = ROOT / "scripts/gemini_text_input_response_latest.md"
    latest_path.write_text(f"# Gemini Text Input Architecture Response\n\n{response}")
    print(f"Also saved to: {latest_path}")
    
    # Print summary
    print("\n" + "=" * 60)
    print("RESPONSE (first 5000 chars):")
    print("=" * 60)
    print(response[:5000] + ("..." if len(response) > 5000 else ""))
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
