#!/usr/bin/env python3
"""
Text Input Architecture Review V2 - Comprehensive Gemini Request

This script collects ~200k lines of source code to send to Gemini for
a detailed architectural review of text input, cursor positioning, and focus management.

KEY INSIGHT FROM USER REBUTTAL:
The previous Gemini response suggested mutating StyledDom directly. This is WRONG because:
1. It doesn't work with multi-node editing (e.g., selections spanning <b>bold</b> text)
2. StyledDom is rebuilt on layout() callback, so changes would be overwritten

CORRECT ARCHITECTURE:
- DON'T update StyledDom - keep it as the "source of truth" from the layout() callback
- DO update the text3::LayoutCache (visual cache) directly
- The hierarchy: PositionedItem -> ShapedItem -> ShapedCluster -> text
- Edits are treated as "quick visual updates" that get persisted via onchange callbacks
- User's onchange callback copies changes to their data model
- If layout() is called again, it returns the "committed" state

This is similar to how React's "optimistic updates" work - the visual updates immediately,
but the actual state is managed by the user's data model.

QUESTIONS FOR GEMINI:
1. How should update_text_cache_after_edit() update the LayoutCache (not StyledDom)?
2. How do onchange/oninput callbacks fire? (see TextInput widget pattern)
3. How is PendingTextEdit passed to CallbackInfo.get_text_changeset()?
4. How should cursor positioning work with this architecture?
5. How should focus transfer work?
6. How should inline images (e.g., from clipboard paste) be handled?
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

# Target: 200k lines of source code (increased from 150k)
TARGET_LINES = 200_000

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
        ["git", "diff", "HEAD~5"],  # Last 5 commits for more context
        cwd=ROOT,
        capture_output=True,
        text=True
    )
    return result.stdout

def collect_source_files() -> list[tuple[str, str]]:
    """Collect source files in priority order for text input review
    
    PRIORITY ORDER (most important first):
    1. Text editing core (edit.rs, selection.rs, cache.rs)
    2. Manager layer (text_input.rs, cursor.rs, focus_cursor.rs)
    3. Callbacks and event handling (callbacks.rs, event_v2.rs)
    4. Widget implementations (text_input.rs widget for onchange pattern)
    5. Window coordination (window.rs)
    6. Display list generation
    7. Core types (dom.rs, styled_dom.rs)
    8. Hit testing
    9. Platform layer (macos, windows)
    10. Architecture docs
    """
    
    file_patterns = [
        # ============ TEXT EDITING (HIGHEST PRIORITY) ============
        "layout/src/text3/edit.rs",           # Text editing operations
        "layout/src/text3/selection.rs",      # Selection handling
        "layout/src/text3/cache.rs",          # Text cache - PositionedItem, ShapedItem, ShapedCluster
        "layout/src/text3/mod.rs",            # Text module overview
        
        # ============ MANAGERS (CRITICAL) ============
        "layout/src/managers/text_input.rs",  # TextInputManager - PendingTextEdit
        "layout/src/managers/cursor.rs",      # CursorManager - cursor positioning
        "layout/src/managers/selection.rs",   # SelectionManager
        "layout/src/managers/focus_cursor.rs",# FocusManager
        "layout/src/managers/undo_redo.rs",   # Undo/redo for text operations
        "layout/src/managers/changeset.rs",   # Text changesets (if exists)
        "layout/src/managers/clipboard.rs",   # Clipboard operations
        "layout/src/managers/mod.rs",         # Manager module overview
        
        # ============ CALLBACKS (CRITICAL for onchange pattern) ============
        "layout/src/callbacks.rs",            # CallbackInfo - get_text_changeset()
        "layout/src/default_actions.rs",      # Default event handling
        "core/src/callbacks.rs",              # Core callback types
        
        # ============ WIDGET IMPLEMENTATIONS (onchange pattern) ============
        "layout/src/widgets/text_input.rs",   # TextInput widget - shows onchange pattern!
        "layout/src/widgets/number_input.rs", # NumberInput - validation pattern
        "layout/src/widgets/mod.rs",          # Widget module
        
        # ============ WINDOW (CENTRAL COORDINATION) ============
        "layout/src/window.rs",               # update_text_cache_after_edit lives here
        
        # ============ EVENT HANDLING ============
        "dll/src/desktop/shell2/common/event_v2.rs",  # Event processing pipeline
        "core/src/events.rs",                 # Event types
        
        # ============ DISPLAY LIST ============
        "layout/src/solver3/display_list.rs", # Cursor/selection painting
        "layout/src/solver3/getters.rs",      # is_node_contenteditable, caret style
        "layout/src/solver3/mod.rs",          # Solver3 overview
        "layout/src/solver3/cache.rs",        # LayoutCache structure
        
        # ============ CORE TYPES ============
        "core/src/dom.rs",                    # DOM types, NodeType::Text
        "core/src/selection.rs",              # TextCursor, SelectionRange
        "core/src/styled_dom.rs",             # StyledDom structure
        
        # ============ HIT TESTING ============
        "core/src/hit_test.rs",
        "layout/src/hit_test.rs",
        
        # ============ FONT TRAITS (for type understanding) ============
        "layout/src/font_traits.rs",          # InlineContent type
        
        # ============ PLATFORM - macOS (reference implementation) ============
        "dll/src/desktop/shell2/macos/events.rs",
        "dll/src/desktop/shell2/macos/text_input.rs",
        "dll/src/desktop/shell2/macos/mod.rs",
        
        # ============ PLATFORM - Windows ============
        "dll/src/desktop/shell2/windows/events.rs",
        "dll/src/desktop/shell2/windows/mod.rs",
        
        # ============ PLATFORM - Common ============
        "dll/src/desktop/shell2/common/mod.rs",
        "dll/src/desktop/shell2/common/debug_server.rs",
        
        # ============ APP/COMPOSITOR ============
        "dll/src/desktop/app.rs",
        "dll/src/desktop/compositor2.rs",
    ]
    
    files = []
    for pattern in file_patterns:
        path = ROOT / pattern
        if path.exists():
            content = read_file_safe(path)
            files.append((pattern, content))
    
    return files

def build_prompt(source_context: str, git_diff: str) -> str:
    """Build the comprehensive prompt for Gemini"""
    
    # Read previous response for context
    prev_response_path = ROOT / "scripts/gemini_text_input_response_latest.md"
    prev_response = ""
    if prev_response_path.exists():
        prev_response = read_file_safe(prev_response_path)
    
    # Read ARCHITECTURE.md for context
    arch_path = ROOT / "ARCHITECTURE.md"
    architecture_md = ""
    if arch_path.exists():
        architecture_md = read_file_safe(arch_path)
    
    prompt = f"""# Text Input Architecture Review V2 - Comprehensive Analysis

## CRITICAL CONTEXT: User Rebuttal to Previous Response

The previous Gemini response suggested "Mutate `NodeType::Text` in StyledDom directly". 

**THIS IS WRONG.** Here is the user's rebuttal:

> Your concern about multi-node selections is valid. A simple `NodeType::Text(new_string)` update only works for single-node contenteditables. When edits span multiple nodes (e.g., deleting `<b>bold</b>` from `normal <b>bold</b> text`), you are not just changing text content; you are changing the DOM *structure*.

**User's response:** Yes, and this is why this strategy doesn't work. We don't want to update the StyledDom, we only want to update the "visual result".

The idea is that we DON'T update the StyledDom, but we DO update the text cache (i.e. we just update items in the text3::LayoutCache, but keep the StyledDom untouched). The hierarchy is:

- PositionedItem -> ShapedItem -> ShapedCluster -> text - is stored here

This also makes it trivial to update things like inline images (ex. from a clipboard paste).

> The layout engine is already built to read from `StyledDom`. By mutating it and marking the node as dirty, you leverage the existing layout pipeline without modification.

**User's response:** Yes, and this is wrong, as it doesn't work with multi-node editing. 

The idea is to treat "edits" as "quick", because the user callback (onchange) should *copy* the incoming changes and store them in the data model (which is usually what you want on text input in an application). So, if the window's `layout()` function is called again, it will return the old state if the onchange callback hasn't updated the data model yet.

## CORRECT ARCHITECTURE UNDERSTANDING

1. **StyledDom is READ-ONLY during editing** - It represents the "committed" state from the last layout() call
2. **Visual updates go to LayoutCache** - Update PositionedItem/ShapedItem/ShapedCluster directly
3. **onchange callbacks fire** - User callback gets the changeset and updates their data model
4. **If layout() is called** - It rebuilds from the user's data model (now updated)
5. **Inline images work naturally** - Just insert a ShapedItem::Object into the cache

This is similar to:
- React's "optimistic updates" pattern
- Controlled components in React
- The TextInput widget's pattern (see `layout/src/widgets/text_input.rs`)

## ARCHITECTURE.MD

{architecture_md}

## PREVIOUS GEMINI RESPONSE (for reference - but some conclusions were wrong)

{prev_response}

## SOURCE CODE CONTEXT

{source_context}

## RECENT GIT CHANGES

```diff
{git_diff}
```

## QUESTIONS FOR COMPREHENSIVE REVIEW

### Q1: How should `update_text_cache_after_edit()` work with the CORRECT architecture?

The function is in `layout/src/window.rs`. It receives:
- `dom_id: DomId`
- `node_id: NodeId` 
- `new_inline_content: Vec<InlineContent>`

Instead of mutating StyledDom, it should:
1. Find the corresponding entry in the LayoutCache
2. Update the PositionedItem/ShapedItem/ShapedCluster hierarchy
3. NOT trigger a full relayout (that's the whole point)
4. Maybe mark the affected area as needing repaint

Please provide:
- The exact data structures that need to be updated
- How to find the right cache entry
- How to handle cursor position updates
- Code implementation

### Q2: How do onchange/oninput callbacks work?

Looking at `layout/src/widgets/text_input.rs`, the TextInput widget has:
```rust
pub type TextInputOnTextInputCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn;
```

The callback:
1. Receives the current TextInputState (with old text)
2. Gets the changeset via `info.get_text_changeset()` 
3. Returns OnTextInputReturn with update and valid fields

If `valid == TextInputValid::Yes`:
- The visual update is applied
- The widget updates its internal state
- `info.change_node_text()` is called to update the label

The callback receives a TextInputState containing old text, applies the changeset, and returns
whether the update was valid and whether to refresh the DOM.

**Questions:**
- How does `get_text_changeset()` connect to `TextInputManager.pending_changeset`?
- When is the `Input` event fired vs `Change` event?
- How does `prevent_default()` work?
- How should contenteditable elements follow this same pattern?

### Q3: How should cursor click positioning work with LayoutCache architecture?

With the correct architecture:
1. Hit-test happens against the LayoutCache (not StyledDom)
2. Find the PositionedItem at the click position
3. Map click to ShapedCluster within that item
4. Return TextCursor pointing to that cluster

**Questions:**
- What's the best function signature for this?
- How to handle clicks between clusters (affinity)?
- How to handle clicks on empty lines?
- Code implementation

### Q4: How should focus transfer work?

When clicking from contenteditable A to contenteditable B:
1. A loses focus - clear its cursor state
2. B gains focus - hit-test for click position
3. Start blink timer on B
4. Ensure LayoutCache is ready for B

**Questions:**
- How to ensure A's visual state is cleared?
- How to ensure B's cursor appears at click position?
- What about Tab navigation (no click)?

### Q5: How should inline images from clipboard paste work?

With LayoutCache architecture:
1. Paste event triggers
2. Parse clipboard for images
3. Create ShapedItem::Object for each image
4. Insert into LayoutCache at cursor position
5. Fire onchange with the new content

**Questions:**
- How to represent mixed content (text + images) in the changeset?
- How does InlineContent handle this?
- How should the user's onchange callback handle images?

### Q6: Complete implementation plan

Provide a step-by-step implementation plan that:
1. Fixes text input display (update_text_cache_after_edit)
2. Fixes cursor positioning on click
3. Fixes focus transfer
4. Supports inline images
5. Works with multi-node selections (bold, italic, etc.)

For each step, provide:
- File(s) to modify
- Functions to add/change
- Code implementation
- Test cases

## OUTPUT FORMAT

Please provide:
1. Executive summary (2-3 paragraphs)
2. Detailed answer for each Q1-Q6
3. Code implementations with exact file paths
4. Migration path from current (broken) state
5. Test plan
"""
    
    return prompt

def send_to_gemini(prompt: str) -> str:
    """Send prompt to Gemini API"""
    
    headers = {
        "Content-Type": "application/json",
    }
    
    data = {
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 65536,
        }
    }
    
    url = f"{GEMINI_API_URL}?key={API_KEY}"
    
    print("Sending request to Gemini API...")
    print(f"Prompt size: {len(prompt):,} characters")
    
    response = requests.post(url, headers=headers, json=data, timeout=600)
    
    if response.status_code != 200:
        raise Exception(f"Gemini API error: {response.status_code}\n{response.text}")
    
    result = response.json()
    
    if "candidates" not in result or not result["candidates"]:
        raise Exception(f"No candidates in response: {result}")
    
    return result["candidates"][0]["content"]["parts"][0]["text"]

def main():
    print("=" * 60)
    print("Text Input Architecture Review V2 - Comprehensive Analysis")
    print("=" * 60)
    
    # Collect source files
    print("\nCollecting source files (prioritized order)...")
    files = collect_source_files()
    
    # Build source context
    source_parts = []
    total_lines = 0
    
    for path, content in files:
        lines = count_lines(content)
        if total_lines + lines > TARGET_LINES:
            # Try to include partial file if it's important
            remaining = TARGET_LINES - total_lines
            if remaining > 500:  # Worth including partial
                content_lines = content.split('\n')[:remaining]
                content = '\n'.join(content_lines)
                source_parts.append(f"### {path} (partial - first {remaining} lines)\n\n```rust\n{content}\n```\n")
                total_lines += remaining
            break
        
        source_parts.append(f"### {path}\n\n```rust\n{content}\n```\n")
        total_lines += lines
        print(f"  {path}: {lines:,} lines")
    
    print(f"\nTotal source lines collected: {total_lines:,}")
    print(f"Files included: {len(source_parts)}")
    
    source_context = "\n".join(source_parts)
    
    # Get git diff
    print("\nGetting git diff...")
    git_diff = get_git_diff()
    diff_lines = count_lines(git_diff)
    print(f"  Git diff: {diff_lines:,} lines")
    
    # Build prompt
    print("\nBuilding prompt...")
    prompt = build_prompt(source_context, git_diff)
    prompt_lines = count_lines(prompt)
    print(f"  Final prompt: {prompt_lines:,} lines")
    
    # Save prompt for reference
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    prompt_path = ROOT / f"scripts/gemini_text_input_architecture_v2_prompt_{timestamp}.md"
    prompt_path.write_text(prompt)
    print(f"\nPrompt saved to: {prompt_path}")
    
    # Send to Gemini
    print("\nSending to Gemini API (this may take a few minutes)...")
    response = send_to_gemini(prompt)
    
    # Save response
    response_path = ROOT / f"scripts/gemini_text_input_response_v2_{timestamp}.md"
    response_content = f"# Gemini Text Input Architecture Response V2\n\n{response}"
    response_path.write_text(response_content)
    print(f"\nResponse saved to: {response_path}")
    
    # Also save as latest
    latest_path = ROOT / "scripts/gemini_text_input_response_v2_latest.md"
    latest_path.write_text(response_content)
    print(f"Also saved to: {latest_path}")
    
    print("\n" + "=" * 60)
    print("Done!")
    print("=" * 60)

if __name__ == "__main__":
    main()
