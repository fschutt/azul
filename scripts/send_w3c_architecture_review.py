#!/usr/bin/env python3
"""
W3C Architecture Review - Gemini Request

After implementing the "flag and defer" pattern for cursor initialization,
we ask Gemini to review our architectural changes for W3C conformance.

This script:
1. Collects ~200k lines of source code from key files
2. Includes the current git diff showing our changes
3. Asks Gemini to verify W3C conformance and identify remaining issues
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

# Target: 200k lines of source code
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
        ["git", "diff", "HEAD"],
        cwd=ROOT,
        capture_output=True,
        text=True
    )
    return result.stdout

def collect_source_files() -> list[tuple[str, str]]:
    """Collect source files in priority order"""
    
    # Priority order of directories and files
    file_patterns = [
        # ============ MANAGERS (HIGHEST PRIORITY) ============
        "layout/src/managers/cursor.rs",
        "layout/src/managers/focus_cursor.rs", 
        "layout/src/managers/selection.rs",
        "layout/src/managers/text_input.rs",
        "layout/src/managers/scroll_into_view.rs",
        "layout/src/managers/scroll_state.rs",
        "layout/src/managers/hover.rs",
        "layout/src/managers/gesture.rs",
        "layout/src/managers/undo_redo.rs",
        "layout/src/managers/clipboard.rs",
        "layout/src/managers/drag_drop.rs",
        "layout/src/managers/mod.rs",
        
        # ============ WINDOW (CORE COORDINATION) ============
        "layout/src/window.rs",
        
        # ============ EVENT HANDLING ============
        "dll/src/desktop/shell2/common/event_v2.rs",
        "dll/src/desktop/shell2/common/debug_server.rs",
        
        # ============ CALLBACKS ============
        "layout/src/callbacks.rs",
        "layout/src/default_actions.rs",
        
        # ============ CORE TYPES ============
        "core/src/events.rs",
        "core/src/dom.rs",
        "core/src/callbacks.rs",
        "core/src/selection.rs",
        "core/src/task.rs",
        "core/src/styled_dom.rs",
        
        # ============ TEXT LAYOUT ============
        "layout/src/text3/cache.rs",
        "layout/src/text3/edit.rs",
        "layout/src/text3/selection.rs",
        "layout/src/text3/mod.rs",
        
        # ============ TIMER ============
        "layout/src/timer.rs",
        
        # ============ SOLVER/LAYOUT ============
        "layout/src/solver3/display_list.rs",
        "layout/src/solver3/getters.rs",
        "layout/src/solver3/mod.rs",
        "layout/src/solver3/sizing.rs",
        "layout/src/solver3/layout_tree.rs",
        
        # ============ HIT TESTING ============
        "core/src/hit_test.rs",
        "layout/src/hit_test.rs",
        
        # ============ EVENT DETERMINATION ============
        "layout/src/event_determination.rs",
        
        # ============ PLATFORM (macOS reference) ============
        "dll/src/desktop/shell2/macos/events.rs",
        "dll/src/desktop/shell2/macos/mod.rs",
        
        # ============ TESTS ============
        "tests/e2e/contenteditable.c",
        "tests/e2e/test_contenteditable.sh",
        "tests/e2e/test_contenteditable_v2.sh",
        
        # ============ PREVIOUS GEMINI ANALYSIS ============
        "scripts/gemini_w3c_response_latest.md",
    ]
    
    result = []
    for rel_path in file_patterns:
        full_path = ROOT / rel_path
        if full_path.exists():
            content = read_file_safe(full_path)
            result.append((rel_path, content))
    
    return result

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

def build_prompt(git_diff: str) -> str:
    """Build the architecture review prompt"""
    
    prompt = f"""# W3C Architecture Review: Focus/Cursor/Selection Implementation

## 1. Context: Recent Architectural Changes

We've implemented significant architectural changes to make our contenteditable/focus/cursor system conform to the W3C model. This is a code review request to verify our changes are correct and identify any remaining issues.

### Summary of Changes Made

Based on your previous analysis, we implemented the following:

1. **"Flag and Defer" Pattern for Cursor Initialization**
   - Added `cursor_needs_initialization` flag to `FocusManager`
   - Added `PendingContentEditableFocus` struct to track pending cursor init
   - Cursor is NO LONGER initialized during focus event handling
   - New `finalize_pending_focus_changes()` called after event processing

2. **W3C-Conformant ContentEditable Inheritance**
   - Added `is_node_contenteditable_inherited()` that traverses ancestors
   - Added `find_contenteditable_ancestor()` helper
   - Respects `contenteditable="false"` to block inheritance

3. **Separate FocusManager and CursorManager**
   - `FocusManager` tracks `document.activeElement` (keyboard focus)
   - `CursorManager` tracks `Selection.focusNode` (text cursor position)
   - This mirrors the W3C separation of focus and selection

4. **Reserved System Timer IDs**
   - `CURSOR_BLINK_TIMER_ID = 0x0001`
   - User timers start at `0x0100` to avoid conflicts

5. **Debug API Extensions**
   - Added `GetFocusState` to query focused node
   - Added `GetCursorState` to query cursor position/blink state

## 2. Git Diff of All Changes

```diff
{git_diff}
```

## 3. Questions for Review

### Question 1: Is the "Flag and Defer" Pattern Correctly Implemented?

The W3C model requires:
1. Focus event fires during event handling
2. Selection/cursor placement happens after layout
3. Cursor is drawn during paint

Our implementation:
- `handle_focus_change_for_cursor_blink()` sets `cursor_needs_initialization = true`
- `finalize_pending_focus_changes()` is called at end of `process_window_events_recursive_v2()`
- This initializes the cursor with text layout now available

**Is this correct? Are there edge cases we're missing?**

### Question 2: Is ContentEditable Inheritance Correct?

We now have:
```rust
pub fn is_node_contenteditable_inherited(styled_dom: &StyledDom, node_id: NodeId) -> bool {{
    // Traverses ancestors, returns true if any ancestor has contenteditable="true"
    // Returns false if node or ancestor has contenteditable="false"
}}
```

The W3C spec says:
- `contenteditable` inherits down the DOM tree
- `contenteditable="false"` explicitly blocks inheritance
- Text nodes inside contenteditable are editable

**Is our implementation correct per W3C?**

### Question 3: Focus vs Selection Model Separation

We have:
- `FocusManager.focused_node` = the contenteditable container (like `document.activeElement`)
- `CursorManager.cursor_location` = the text node with the caret (like `Selection.focusNode`)

**Does this correctly model the W3C focus/selection separation?**

### Question 4: Timer Architecture

We use a system timer for cursor blinking:
- Timer ID `0x0001` is reserved for cursor blink
- Timer fires every ~530ms
- `toggle_visibility()` is called in the callback
- Timer starts on focus, stops on blur

**Is this the correct approach for cursor blinking?**

### Question 5: Remaining Conformance Issues

Looking at the full codebase provided below, are there any remaining issues where our implementation does NOT conform to the W3C model for:

1. **Focus events** (`focus`, `blur`, `focusin`, `focusout`)
2. **Selection API** (`Selection`, `Range`, `anchorNode`, `focusNode`)
3. **Keyboard events** (how they should interact with contenteditable)
4. **Input events** (`beforeinput`, `input`, `textInput`)
5. **Caret/cursor positioning** within text nodes
6. **Multi-node selection** across text nodes
7. **ContentEditable attribute** inheritance and behavior

## 4. Specific Areas to Review

Please focus on these files and verify W3C conformance:

1. **`layout/src/managers/focus_cursor.rs`** - FocusManager, PendingContentEditableFocus
2. **`layout/src/managers/cursor.rs`** - CursorManager, cursor positioning
3. **`layout/src/managers/selection.rs`** - SelectionManager, multi-node selection
4. **`layout/src/window.rs`** - handle_focus_change_for_cursor_blink, finalize_pending_focus_changes
5. **`dll/src/desktop/shell2/common/event_v2.rs`** - Event processing, focus handling
6. **`layout/src/solver3/getters.rs`** - is_node_contenteditable_inherited

## 5. Expected Output

Please provide:

1. **Verification** that our "flag and defer" pattern is correct
2. **List of any W3C conformance issues** in the codebase
3. **Specific code changes needed** to fix any issues found
4. **Edge cases** we may have missed
5. **Recommendations** for the test suite to verify correctness

## 6. Source Code Reference

The following source files are provided for analysis (see below).
Focus especially on the manager files and event handling code.

"""
    return prompt

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
        timeout=600  # 10 minute timeout
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
    print("W3C Architecture Review - Post-Implementation Check")
    print("=" * 60)
    
    # Step 1: Get git diff
    print("\nGetting git diff...")
    git_diff = get_git_diff()
    print(f"Git diff: {len(git_diff)} chars, {count_lines(git_diff)} lines")
    
    # Step 2: Dump source files
    dump_path = ROOT / "scripts/w3c_architecture_review_source_dump.txt"
    total_lines = dump_source_files(dump_path)
    print(f"\nSource dump saved to: {dump_path}")
    print(f"Total lines: {total_lines}")
    
    # Step 3: Build prompt
    prompt = build_prompt(git_diff)
    prompt_path = ROOT / "scripts/w3c_architecture_review_prompt.md"
    prompt_path.write_text(prompt)
    print(f"\nPrompt saved to: {prompt_path}")
    
    # Step 4: Read source dump
    source_dump = dump_path.read_text()
    
    # Step 5: Send to Gemini
    response = send_to_gemini(prompt, source_dump)
    
    # Step 6: Save response
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    response_path = ROOT / f"scripts/gemini_w3c_architecture_review_{timestamp}.md"
    response_path.write_text(f"# Gemini W3C Architecture Review Response\n\n{response}")
    print(f"\nResponse saved to: {response_path}")
    
    # Also save to latest
    latest_path = ROOT / "scripts/gemini_w3c_architecture_review_latest.md"
    latest_path.write_text(f"# Gemini W3C Architecture Review Response\n\n{response}")
    print(f"Also saved to: {latest_path}")
    
    # Print summary
    print("\n" + "=" * 60)
    print("RESPONSE (first 4000 chars):")
    print("=" * 60)
    print(response[:4000] + ("..." if len(response) > 4000 else ""))
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
