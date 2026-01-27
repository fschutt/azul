#!/usr/bin/env python3
"""
Build a comprehensive Gemini prompt with all relevant source files
for cursor blinking, text editing, and scroll-into-view architecture.

This version includes ~80k lines of context covering:
- Timer system and platform implementations
- All manager files (cursor, focus, selection, scroll, text_input, etc.)
- Text editing (text3)
- Display list generation (cursor/selection rendering)
- Core types (callbacks, events, selection, task/TimerId)
- Platform-specific timer code (Windows, macOS)
- Event processing pipeline
"""

import os
from pathlib import Path

AZUL_ROOT = Path(__file__).parent.parent

# Files to include in the prompt, ordered by importance
SOURCE_FILES = [
    # Architecture overview (IMPORTANT: at the top for context)
    "ARCHITECTURE.md",
    
    # Core types for timers, callbacks, events
    "core/src/task.rs",           # TimerId, Duration, TerminateTimer
    "core/src/callbacks.rs",      # CallbackInfo, Update, CallbackType
    "core/src/events.rs",         # SyntheticEvent types
    "core/src/selection.rs",      # TextSelection, TextCursor, GraphemeClusterId
    "core/src/dom.rs",            # Dom, DomNodeId, NodeId
    "core/src/refany.rs",         # RefAny for callback data
    "core/src/window.rs",         # WindowState, WindowFlags
    "core/src/styled_dom.rs",     # StyledDom
    "core/src/prop_cache.rs",     # CSS property cache
    
    # Timer system
    "layout/src/timer.rs",        # Timer, TimerCallback, TimerCallbackInfo
    
    # ALL managers (the interaction between these is the key question)
    "layout/src/managers/mod.rs",
    "layout/src/managers/cursor.rs",          # CursorManager - ALREADY EXISTS!
    "layout/src/managers/focus_cursor.rs",    # FocusManager
    "layout/src/managers/selection.rs",       # SelectionManager
    "layout/src/managers/text_input.rs",      # TextInputManager
    "layout/src/managers/changeset.rs",       # Changeset for text edits
    "layout/src/managers/scroll_into_view.rs",# ScrollIntoView helper
    "layout/src/managers/scroll_state.rs",    # ScrollManager
    "layout/src/managers/undo_redo.rs",       # UndoRedo for text editing
    "layout/src/managers/gesture.rs",         # GestureManager
    "layout/src/managers/hover.rs",           # HoverManager
    "layout/src/managers/gpu_state.rs",       # GpuStateManager (opacity animations)
    "layout/src/managers/iframe.rs",          # IFrameManager
    "layout/src/managers/a11y.rs",            # Accessibility
    "layout/src/managers/clipboard.rs",       # Clipboard
    "layout/src/managers/drag_drop.rs",       # Drag/Drop
    "layout/src/managers/file_drop.rs",       # File drop
    
    # Text editing engine
    "layout/src/text3/mod.rs",
    "layout/src/text3/edit.rs",       # Text editing operations
    "layout/src/text3/selection.rs",  # Selection in text
    "layout/src/text3/cache.rs",      # Text layout cache
    
    # Window and event handling
    "layout/src/window.rs",           # LayoutWindow - main entry point
    "layout/src/window_state.rs",     # FullWindowState
    "layout/src/default_actions.rs",  # Default event handlers
    "layout/src/event_determination.rs", # Event routing
    "layout/src/hit_test.rs",         # Hit testing for clicks
    "layout/src/callbacks.rs",        # CallbackInfo implementation
    
    # Display list generation (cursor/selection rendering)
    "layout/src/solver3/mod.rs",
    "layout/src/solver3/display_list.rs",  # paint_selection_and_cursor
    "layout/src/solver3/getters.rs",       # is_text_selectable, is_node_contenteditable
    "layout/src/solver3/cache.rs",         # LayoutCache
    "layout/src/solver3/fc.rs",            # Formatting contexts
    
    # Platform timer implementations
    "dll/src/desktop/shell2/windows/mod.rs",  # SetTimer/KillTimer
    "dll/src/desktop/shell2/macos/mod.rs",    # NSTimer
    
    # Event processing
    "dll/src/desktop/shell2/common/event_v2.rs",
    "dll/src/desktop/shell2/common/callback_processing.rs",
    
    # App entry point
    "dll/src/desktop/app.rs",
]

HEADER = """# AZUL TEXT EDITING SYSTEM ARCHITECTURE REQUEST

## OVERVIEW

I'm building a text editing system for the Azul GUI framework (Rust). I need architectural guidance on:

1. **Cursor Blinking Timer** - The cursor should blink at ~530ms intervals, but:
   - Stop blinking (stay visible) while user is actively typing
   - Resume blinking after ~530ms of no input
   - Only blink when the focused element is contenteditable
   - Use "reserved" TimerIds that can be dynamically started/stopped

2. **Timer Architecture with Reserved IDs**:
   - In Rust, timers have IDs with a "KillTimer" / "StartTimer" API
   - Windows uses `SetTimer(hwnd, timer_id, interval_ms, ...)` / `KillTimer(hwnd, timer_id)`
   - macOS uses `NSTimer` with `scheduledTimerWithTimeInterval:...`
   - I want a list of "reserved" timer IDs for system timers (cursor blink, scroll momentum, etc.)
   - These can be dynamically started/stopped via private API through CallbackInfo

3. **Text Input/Editing Flow**:
   - On focus received + element is contenteditable → register cursor blink timer
   - On focus out → kill cursor blink timer (via TerminateTimer return or explicit kill)
   - Keyboard input should insert text at cursor position
   - If there's a range selection, replace it with the input
   - User callbacks can inspect and reject changes (e.g., number-only input)

4. **Scroll Selection End / Cursor Into View** (CRITICAL):
   - When typing in a scrollable container, cursor must stay visible
   - When extending selection (Shift+Arrow), the selection END (not anchor) should scroll into view
   - How should the managers interact?
     - FocusManager knows which node has focus
     - CursorManager knows cursor position within text
     - SelectionManager knows selection range
     - ScrollManager controls scroll positions
     - TextInputManager handles keyboard input
   - What's the flow? E.g., user types 'a':
     1. KeyboardEvent → TextInputManager.record_input()?
     2. → CursorManager.get_cursor_position()
     3. → Insert text, update cursor
     4. → ScrollIntoView.scroll_cursor_into_view()?
     5. → ScrollManager.scroll_to()?
   - Who owns this coordination?

5. **CallbackChange for Cursor Opacity**:
   - Timer callback should update cursor visibility (opacity 0/1)
   - This could be done via a new "CallbackChange" operation
   - Or by modifying a private API exposed through CallbackInfo
   - What's the cleanest architecture?

## IMPORTANT: EXISTING CODE

**The `CursorManager` already exists at `layout/src/managers/cursor.rs`!**
Please review it and suggest modifications rather than creating a new one.

## QUESTIONS FOR GEMINI

1. **Reserved Timer IDs**: What's a good scheme for reserved system timer IDs?
   - Proposal: IDs 0x0000-0x00FF reserved for system (cursor blink = 0x0001, scroll momentum = 0x0002, etc.)
   - User timers start at 0x0100

2. **Manager Coordination**: Who coordinates the managers?
   - Should there be a "TextEditingCoordinator" that wraps FocusManager + CursorManager + SelectionManager + TextInputManager?
   - Or should coordination happen in `layout/src/window.rs`?
   - Or in `default_actions.rs`?

3. **Scroll Into View Flow**: What's the exact flow for scrolling cursor/selection into view?
   - When should it happen? (after every keystroke? only on cursor movement?)
   - Who triggers it? (TextInputManager? CursorManager? window.rs?)
   - How does it interact with ongoing scroll animations?

4. **Cursor Blink State**: Where should cursor visibility state live?
   - In CursorManager (add `is_visible: bool` field)?
   - In a separate `CursorBlinkState` struct?
   - In LayoutWindow directly?

5. **Focus Change Callbacks**: How to hook into focus changes?
   - Current: `On::FocusReceived` / `On::FocusLost` user callbacks
   - Proposal: Internal focus change handler in window.rs that:
     - On focus to contenteditable: start cursor blink timer (ID 0x0001)
     - On focus away: stop timer, clear cursor
   - Return `TerminateTimer` from timer callback when focus is lost?

Please analyze the source code below and provide:
1. Specific code changes to implement this architecture
2. The exact flow diagram for text input → scroll into view
3. How the platform timer APIs (SetTimer/KillTimer, NSTimer) map to this

---

## SOURCE CODE

"""

def main():
    prompt = HEADER
    
    total_lines = 0
    files_included = 0
    
    for rel_path in SOURCE_FILES:
        full_path = AZUL_ROOT / rel_path
        if not full_path.exists():
            print(f"WARNING: File not found: {rel_path}")
            continue
        
        try:
            content = full_path.read_text(encoding='utf-8')
            lines = content.count('\n') + 1
            total_lines += lines
            files_included += 1
            
            # Add file header
            prompt += f"\n### FILE: {rel_path}\n"
            prompt += f"```{'rust' if rel_path.endswith('.rs') else 'markdown'}\n"
            prompt += content
            if not content.endswith('\n'):
                prompt += '\n'
            prompt += "```\n"
            
            print(f"Added {rel_path}: {lines} lines")
        except Exception as e:
            print(f"ERROR reading {rel_path}: {e}")
    
    # Write the prompt to a file
    output_path = AZUL_ROOT / "scripts" / "gemini_cursor_blinking_prompt_v2.md"
    output_path.write_text(prompt, encoding='utf-8')
    
    print(f"\n{'='*60}")
    print(f"Total files: {files_included}")
    print(f"Total lines: {total_lines}")
    print(f"Prompt size: {len(prompt)} chars")
    print(f"Estimated tokens: ~{len(prompt) // 4}")
    print(f"Written to: {output_path}")
    print(f"{'='*60}")
    
    return prompt

if __name__ == "__main__":
    main()
