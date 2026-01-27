#!/usr/bin/env python3
"""
W3C Text Input Architecture Review - Gemini Request

Step 1: Dump source files to a large text file (~200k lines)
Step 2: Build the prompt with the question
Step 3: Send to Gemini API (gemini-2.5-pro)
"""
import json
import requests
import subprocess
import sys
from pathlib import Path
from datetime import datetime

ROOT = Path(__file__).parent.parent
API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()

# Gemini API - use same model pattern as debug.rs
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

def collect_source_files() -> list[tuple[str, str]]:
    """Collect source files in priority order"""
    
    # Priority order of directories and files
    file_patterns = [
        # Core cursor/focus/selection managers (highest priority)
        "layout/src/managers/cursor.rs",
        "layout/src/managers/focus_cursor.rs", 
        "layout/src/managers/selection.rs",
        "layout/src/managers/text_input.rs",
        "layout/src/managers/mod.rs",
        
        # Window and layout
        "layout/src/window.rs",
        
        # Event handling
        "dll/src/desktop/shell2/common/event_v2.rs",
        "dll/src/desktop/shell2/common/debug_server.rs",
        
        # Default actions
        "layout/src/default_actions.rs",
        
        # Core DOM and events
        "core/src/styled_dom.rs",
        "core/src/events.rs",
        "core/src/dom.rs",
        "core/src/callbacks.rs",
        
        # Text layout
        "layout/src/text3/cache.rs",
        "layout/src/text3/mod.rs",
        
        # Solver
        "layout/src/solver3/mod.rs",
        "layout/src/solver3/display_list.rs",
        "layout/src/solver3/getters.rs",
        "layout/src/solver3/fc.rs",
        
        # CSS parsing
        "css/src/css.rs",
        "css/src/parser2.rs",
        
        # Tests
        "tests/e2e/contenteditable.c",
        "scripts/E2E_CONTENTEDITABLE_TEST_PLAN.md",
        
        # Additional managers
        "layout/src/managers/scroll_into_view.rs",
        "layout/src/managers/scroll_state.rs",
        "layout/src/managers/hover.rs",
        "layout/src/managers/gesture.rs",
        "layout/src/managers/undo_redo.rs",
        "layout/src/managers/clipboard.rs",
        "layout/src/managers/drag_drop.rs",
        
        # Timer/task
        "core/src/task.rs",
        "layout/src/timer.rs",
        
        # Hit testing
        "core/src/hit_test.rs",
        "layout/src/hit_test.rs",
        
        # Selection types
        "core/src/selection.rs",
        
        # Additional core files
        "core/src/window.rs",
        "core/src/resources.rs",
        "core/src/refany.rs",
        
        # Platform-specific event handling (macOS as reference)
        "dll/src/desktop/shell2/macos/events.rs",
        "dll/src/desktop/shell2/macos/mod.rs",
        
        # More layout files
        "layout/src/solver3/sizing.rs",
        "layout/src/solver3/layout_tree.rs",
        "layout/src/solver3/cache.rs",
        
        # Callbacks
        "layout/src/callbacks.rs",
        
        # Event determination
        "layout/src/event_determination.rs",
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

def build_prompt() -> str:
    """Build the analysis prompt (without source code)"""
    
    prompt = """# W3C-Conformant Text Input Architecture Review Request

## 1. Executive Summary: Current Problems

We're implementing contenteditable text input in Azul (a Rust GUI framework). After Tab-focusing on a contenteditable `<div>`, the **Debug API reports**:

```json
{
  "has_focus": true,
  "focused_node": {
    "dom_id": 0,
    "node_id": 3,
    "is_contenteditable": true,
    "text_content": "Initial text here"
  }
}
```

But the **cursor state** returns:

```json
{
  "has_cursor": false,
  "cursor": null
}
```

**Key Problems:**
1. **Cursor not initializing** - After Tab focus lands on contenteditable, `CursorManager.cursor` stays `None`
2. **`:focus` CSS styling not rendering** - The blue border from `:focus` pseudo-class doesn't appear
3. **Timer action not being applied** - `handle_focus_change_for_cursor_blink()` returns `CursorBlinkTimerAction::Start(timer)` but timer doesn't start

## 2. Debug Output Analysis

When Tab is pressed:
```
[DEBUG] handle_focus_change_for_cursor_blink called with new_focus=Some(DomNodeId { dom: DomId { inner: 0 }, node: NodeHierarchyItemId(4) })
[DEBUG] is_new_focus_contenteditable=true, timer_was_active=false
[DEBUG] contenteditable_node_id=NodeId(3)
[DEBUG] text_node_id=NodeId(4) (was contenteditable_node_id=NodeId(3))
[DEBUG] text_layout is_some=false  ← PROBLEM: No text layout available!
[DEBUG] cursor_initialized=true
[DEBUG] Returning CursorBlinkTimerAction::Start
```

The issue: `get_inline_layout_for_node()` returns `None` because the layout hasn't been computed yet for the text node.

## 3. Questions for W3C Conformance Review

### Question 1: Event Target vs Original Target

In the W3C DOM Events specification, there's a distinction between:
- **`event.target`**: The node that dispatched the event (may be the focused node itself)
- **`event.originalTarget`** (Firefox): The node that originally received the event before bubbling

In our implementation:
- Focus lands on the **contenteditable div** (node_id=3)
- But cursor should be placed in the **child Text node** (node_id=4)

**How should this be handled according to W3C?**

### Question 2: Focus vs Selection Model

The W3C defines the Selection API:
```webidl
interface Selection {
  readonly attribute Node? anchorNode;
  readonly attribute unsigned long anchorOffset;
  readonly attribute Node? focusNode;
  readonly attribute unsigned long focusOffset;
};
```

**Questions:**
1. Should `Selection.focusNode` point to the **Text node** with the cursor, or the **contenteditable container**?
2. Is it correct to have separate `FocusManager` (keyboard focus) and `CursorManager` (text cursor)?
3. How does the W3C model handle the case where the contenteditable contains multiple text nodes?

### Question 3: ContentEditable Attribute Inheritance

Per HTML5 spec, `contenteditable` is inherited. Our test has:

```html
<div contenteditable="true">
  Text content here
</div>
```

**Questions:**
1. Should `is_node_contenteditable()` check the node itself or traverse to editable ancestor?
2. When focusing, should we focus the contenteditable ancestor or the innermost text node?

### Question 4: Cursor Initialization Timing

Current flow:
1. `set_focused_node()` is called
2. Then `handle_focus_change_for_cursor_blink()` is called
3. Inside that, `initialize_cursor_at_end()` is called

**But our debug shows `has_cursor: false`.**

**Question:** In the W3C model, when exactly should the cursor (caret) be created after focus changes?

### Question 5: Event Bubbling for Focus

The DOM Events spec says:
- `focus` and `blur` events do NOT bubble
- `focusin` and `focusout` events DO bubble

**Questions:**
1. Are we correctly dispatching both `Focus` (non-bubbling) and `FocusIn` (bubbling)?
2. Should the cursor be initialized during `Focus` or `FocusIn` event handling?

## 4. Specific Bug Analysis

### Bug 1: No Text Layout at Focus Time

When Tab focus happens:
1. Layout has been computed for the DOM
2. But `get_inline_layout_for_node()` returns `None`

Possible causes:
- Text layout is stored by layout node index, not DOM node ID?
- Layout results aren't accessible during event processing?
- The text node layout key is different from what we're querying?

### Bug 2: `:focus` CSS Not Rendering

The `apply_focus_restyle()` function is called, but:
- The blue border (`border-color: #0078d4`) doesn't appear
- `restyle_on_state_change()` may not be finding matching `:focus` rules

Possible causes:
- The StyledNodeState isn't being updated with focus state?
- The display list isn't being regenerated after restyle?
- `:focus` matching logic has a bug?

### Bug 3: Timer Not Starting

Even though `handle_focus_change_for_cursor_blink()` returns `Start(timer)`:
- The cursor blink timer isn't running
- Debug API shows `blink_timer_active: false`

Possible causes:
- `start_timer()` isn't being called on the platform layer?
- Timer is being immediately stopped?
- Platform timer implementation has issues?

## 5. Requested Analysis

Please analyze the source code provided and give:

1. **W3C Conformance Review**
   - How does the W3C model define contenteditable focus behavior?
   - What is the correct relationship between keyboard focus, selection, and cursor?
   - How should `originalTarget` vs `target` work for focus on contenteditable?

2. **Architecture Recommendations**
   - Should cursor initialization happen during focus or after layout?
   - What is the correct way to find the text node for cursor placement?
   - How should the system handle nested contenteditable elements?

3. **Bug Fix Recommendations**
   - Why is `get_inline_layout_for_node()` returning `None`?
   - How can we ensure the cursor is initialized even without layout?
   - What is the correct timing for `:focus` style application?

4. **Code Fix Suggestions**
   - Provide specific code changes to fix cursor initialization
   - Provide specific code changes to fix `:focus` CSS application
   - Provide specific code changes to fix timer start/stop logic

## 6. Source Code Reference

The following source files are provided for analysis (see below).
Key files to focus on:
- `layout/src/window.rs` - `handle_focus_change_for_cursor_blink()`, `find_last_text_child()`
- `layout/src/managers/cursor.rs` - `CursorManager`, `initialize_cursor_at_end()`
- `layout/src/managers/focus_cursor.rs` - `FocusManager`, `resolve_focus_target()`
- `dll/src/desktop/shell2/common/event_v2.rs` - Tab focus processing, restyle application
- `core/src/styled_dom.rs` - `restyle_on_state_change()`, `restyle_nodes_focus()`

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
    print("W3C Text Input Architecture Review")
    print("=" * 60)
    
    # Step 1: Dump source files
    dump_path = ROOT / "scripts/w3c_review_source_dump.txt"
    total_lines = dump_source_files(dump_path)
    print(f"\nSource dump saved to: {dump_path}")
    print(f"Total lines: {total_lines}")
    
    # Step 2: Build prompt
    prompt = build_prompt()
    prompt_path = ROOT / "scripts/w3c_review_prompt.md"
    prompt_path.write_text(prompt)
    print(f"\nPrompt saved to: {prompt_path}")
    
    # Step 3: Read source dump
    source_dump = dump_path.read_text()
    
    # Step 4: Send to Gemini
    response = send_to_gemini(prompt, source_dump)
    
    # Step 5: Save response
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    response_path = ROOT / f"scripts/gemini_w3c_response_{timestamp}.md"
    response_path.write_text(f"# Gemini W3C Review Response\n\n{response}")
    print(f"\nResponse saved to: {response_path}")
    
    # Also save to latest
    latest_path = ROOT / "scripts/gemini_w3c_response_latest.md"
    latest_path.write_text(f"# Gemini W3C Review Response\n\n{response}")
    print(f"Also saved to: {latest_path}")
    
    # Print summary
    print("\n" + "=" * 60)
    print("RESPONSE (first 3000 chars):")
    print("=" * 60)
    print(response[:3000] + ("..." if len(response) > 3000 else ""))
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
