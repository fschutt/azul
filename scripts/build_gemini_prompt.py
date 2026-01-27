#!/usr/bin/env python3
"""
Build a comprehensive Gemini prompt with all relevant source files
for cursor blinking and text editing architecture.
"""

import os
from pathlib import Path

AZUL_ROOT = Path(__file__).parent.parent

# Files to include in the prompt
SOURCE_FILES = [
    # Core selection types
    "core/src/selection.rs",
    "core/src/callbacks.rs",
    "core/src/events.rs",
    
    # Timer system
    "layout/src/timer.rs",
    
    # Managers
    "layout/src/managers/mod.rs",
    "layout/src/managers/focus_cursor.rs",
    "layout/src/managers/selection.rs",
    "layout/src/managers/text_input.rs",
    "layout/src/managers/changeset.rs",
    "layout/src/managers/scroll_into_view.rs",
    "layout/src/managers/scroll_state.rs",
    
    # Text editing
    "layout/src/text3/edit.rs",
    "layout/src/text3/selection.rs",
    "layout/src/text3/mod.rs",
    
    # Window and event handling
    "layout/src/window_state.rs",
    "layout/src/default_actions.rs",
    
    # Solver/display list (selection rendering)
    "layout/src/solver3/getters.rs",
    "layout/src/solver3/display_list.rs",
]

HEADER = """# AZUL TEXT EDITING SYSTEM ARCHITECTURE REQUEST

## OVERVIEW

I'm building a text editing system for the Azul GUI framework (Rust). I need to implement:

1. **Cursor blinking timer** - The cursor should blink at ~530ms intervals, but:
   - Stop blinking (stay visible) while user is actively typing
   - Resume blinking after ~530ms of no input
   - Only blink when the focused element is contenteditable

2. **Text input/editing** - When a contenteditable element has focus:
   - Keyboard input should insert text at cursor position
   - If there's a range selection, replace it with the input
   - Keep cursor in view (scroll into view as we type)
   - Support paste, a11y input, programmatic input
   - User callbacks can inspect and reject changes (e.g., number-only input)

3. **Cursor position tracking** - A cursor is a selection with 0 size. We need to track:
   - Whether cursor is attached to start or end of selection (for directional selection)
   - This matters for cursor blinking position

## CURRENT STATE

I've already implemented:
- Selection rendering (selection rects, cursor rect)
- `is_text_selectable()` and `is_node_contenteditable()` checks
- Basic `TextSelection` with anchor/focus model
- Timer system for animations

## QUESTIONS FOR GEMINI

1. **Timer Architecture**: How should I integrate cursor blinking with the existing timer system? Should it be a global timer or per-focused-element?

2. **Input Flow**: What's the best architecture for text input flow?
   - OS keyboard event → ???? → text inserted in DOM → relayout → redraw

3. **Changeset System**: How should the changeset system work for user callbacks to inspect/reject changes?

4. **Scroll Into View**: How to keep cursor visible while typing in a scrollable container?

5. **Selection Replacement**: Best way to handle "type to replace selection" behavior?

Please analyze the source code below and provide a detailed architecture recommendation with specific code changes.

---

## SOURCE CODE

"""

def main():
    prompt = HEADER
    
    total_lines = 0
    for rel_path in SOURCE_FILES:
        full_path = AZUL_ROOT / rel_path
        if not full_path.exists():
            print(f"WARNING: {rel_path} not found")
            continue
        
        content = full_path.read_text()
        lines = len(content.splitlines())
        total_lines += lines
        
        prompt += f"\n### {rel_path}\n\n```rust\n{content}\n```\n"
        print(f"Added {rel_path}: {lines} lines")
    
    # Also add window.rs but only the relevant parts (it's 6500 lines)
    window_rs = AZUL_ROOT / "layout/src/window.rs"
    if window_rs.exists():
        content = window_rs.read_text()
        # Include the whole file - we have 1M tokens
        lines = len(content.splitlines())
        total_lines += lines
        prompt += f"\n### layout/src/window.rs\n\n```rust\n{content}\n```\n"
        print(f"Added layout/src/window.rs: {lines} lines")
    
    # Write the prompt
    output_path = AZUL_ROOT / "scripts/gemini_cursor_blinking_prompt.md"
    output_path.write_text(prompt)
    
    print(f"\n=== SUMMARY ===")
    print(f"Total lines: {total_lines}")
    print(f"Prompt size: {len(prompt)} chars")
    print(f"Estimated tokens: ~{len(prompt) // 4}")
    print(f"Written to: {output_path}")

if __name__ == "__main__":
    main()
