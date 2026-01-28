#!/usr/bin/env python3
"""
Build a comprehensive Gemini prompt for full architectural analysis of contenteditable bugs.
Includes all relevant source files, logs, diffs, and screenshot.
"""

import os
import subprocess
import base64
from datetime import datetime
from pathlib import Path

# Configuration
WORKSPACE_ROOT = Path("/Users/fschutt/Development/azul")
OUTPUT_DIR = WORKSPACE_ROOT / "scripts"
MAX_LINES = 200000
SCREENSHOT_PATH = WORKSPACE_ROOT / "examples" / "c" / "screenshot6.png"

# Track line count
total_lines = 0

def count_lines(content: str) -> int:
    return len(content.split('\n'))

def read_file_safe(path: Path) -> str:
    """Read file with error handling."""
    try:
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            return f.read()
    except Exception as e:
        return f"[ERROR reading file: {e}]"

def get_git_diff() -> str:
    """Get current git diff."""
    try:
        result = subprocess.run(
            ['git', 'diff', 'HEAD'],
            cwd=WORKSPACE_ROOT,
            capture_output=True,
            text=True
        )
        return result.stdout if result.stdout else "[No uncommitted changes]"
    except Exception as e:
        return f"[ERROR getting git diff: {e}]"

def get_recent_commits(n: int = 10) -> str:
    """Get recent commit history with diffs."""
    try:
        result = subprocess.run(
            ['git', 'log', '-p', f'-{n}', '--oneline'],
            cwd=WORKSPACE_ROOT,
            capture_output=True,
            text=True
        )
        return result.stdout if result.stdout else "[No recent commits]"
    except Exception as e:
        return f"[ERROR getting commits: {e}]"

def get_screenshot_base64() -> str:
    """Get screenshot as base64."""
    try:
        with open(SCREENSHOT_PATH, 'rb') as f:
            data = f.read()
            return base64.b64encode(data).decode('utf-8')
    except Exception as e:
        return f"[ERROR reading screenshot: {e}]"

def add_file(sections: list, path: Path, description: str = None):
    """Add a file to sections if it exists and we have budget."""
    global total_lines
    
    if not path.exists():
        return
    
    content = read_file_safe(path)
    lines = count_lines(content)
    
    if total_lines + lines > MAX_LINES:
        sections.append(f"\n[SKIPPED: {path.relative_to(WORKSPACE_ROOT)} - would exceed {MAX_LINES} line limit]\n")
        return
    
    rel_path = path.relative_to(WORKSPACE_ROOT)
    header = f"\n{'='*80}\n## FILE: {rel_path}"
    if description:
        header += f"\n## Description: {description}"
    header += f"\n{'='*80}\n"
    
    sections.append(header)
    sections.append(f"```\n{content}\n```\n")
    total_lines += lines

def add_directory_files(sections: list, dir_path: Path, pattern: str = "*.rs", description: str = None):
    """Add all matching files from a directory."""
    if not dir_path.exists():
        return
    
    for file_path in sorted(dir_path.rglob(pattern)):
        if file_path.is_file():
            add_file(sections, file_path, description)

def build_prompt():
    global total_lines
    sections = []
    
    # Header
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    
    sections.append(f"""# Azul ContentEditable Full Architectural Analysis
# Generated: {datetime.now().isoformat()}
# Purpose: Debug multiple contenteditable and layout bugs

## BUGS TO FIX (Priority Order)

### MAJOR BUGS:

1. **Text Input Not Working**
   - Clicking on contenteditable positions cursor (green cursor visible)
   - Typing 'f' multiple times produces: `[record_text_input] ERROR: No focused node!`
   - The contenteditable div should be focusable
   - Need to understand how focus works when `<div tabindex=0>` is parent of contenteditable
   - Focus should be set on click, not just cursor position

2. **Line Wrapping When It Shouldn't**
   - ContentEditable wraps lines but shouldn't in this case
   - Should horizontally overflow and be horizontally scrollable
   - Related to `white-space` CSS property handling

3. **Scroll Into View**
   - When typing, cursor should scroll into view
   - Cannot test until text input works

### MINOR BUGS:

4. **Scrollbar Visibility (auto mode)**
   - Scrollbar shows at 100% even though nothing to scroll
   - `overflow: auto` should hide scrollbar when content fits
   - Also affects size reservation (auto shouldn't reserve space if not scrollable)

5. **Scrollbar Track Width**
   - Track width calculated wrong
   - Should be "100% - button sizes on each side"

6. **Scrollbar Position**
   - Scrollbar in wrong position/size
   - Should use "inner" box (directly below text), not outer box

7. **Border Position**
   - Light grey 1px border around content is mispositioned
   - Border and content with padding should NEVER be visually detached

8. **Font Resolution**
   - "d" character shows as box (missing glyph)
   - Wrong font being used
   - Need to debug rust-fontconfig and text3 font resolution

---

## CHAT SUMMARY (Context from debugging session)

### Investigation Timeline:
1. User reported clicking positions cursor but typing doesn't work
2. Found `insertText:replacementRange:` never called - added `interpretKeyEvents` to `keyDown`
3. Still getting "blip" sound - fixed `selectedRange()` to return `{{location: 0, length: 0}}` not `NSNotFound`
4. No blip but still no text - NSTextInputClient not triggering with objc2
5. Workaround: Direct text input in `handle_key_down` for printable characters
6. Still "No focused node" error - clicking sets cursor but not focus
7. Found: `is_contenteditable` check only checked IFC root node
8. Fix attempt: Walk up DOM tree to find `contenteditable` attribute on ancestor
9. STILL NOT WORKING - "No focused node" error persists

### Key Files Modified:
- `dll/src/desktop/shell2/macos/mod.rs` - keyDown, selectedRange, insert_text
- `dll/src/desktop/shell2/macos/events.rs` - handle_key_down direct text input
- `layout/src/window.rs` - contenteditable ancestor detection
- `layout/src/text3/edit.rs` - CursorAffinity handling

### Architecture Understanding:
- IFC root node is the TEXT node, not the DIV with contenteditable
- `contenteditable` attribute is on parent DIV
- Focus manager and cursor manager are separate
- macOS uses NSTextInputClient protocol via objc2 bindings

---

## SCREENSHOT (Base64 PNG)

The screenshot shows:
- "Click here and type..." text with green cursor after "an"
- Scrollbar visible at 100% (shouldn't be visible)
- Light grey border visually detached from content
- Font rendering issue with "d" character

```
data:image/png;base64,{get_screenshot_base64()}
```

---

## CURRENT GIT DIFF (Uncommitted Changes)

```diff
{get_git_diff()}
```

---

## RECENT COMMITS (Last 10 with diffs)

```
{get_recent_commits(10)}
```

---

## OUTPUT LOGS

""")
    
    # Add output logs
    out_txt = WORKSPACE_ROOT / "examples" / "c" / "out.txt"
    if out_txt.exists():
        content = read_file_safe(out_txt)
        # Limit log size
        lines = content.split('\n')
        if len(lines) > 500:
            content = '\n'.join(lines[-500:])
            content = f"[... truncated, showing last 500 lines ...]\n{content}"
        sections.append(f"### examples/c/out.txt\n```\n{content}\n```\n")
        total_lines += count_lines(content)
    
    sections.append("\n---\n\n## SOURCE FILES\n")
    
    # Priority 1: Text3 module (text editing core)
    sections.append("\n### TEXT3 MODULE (Text Editing Core)\n")
    text3_dir = WORKSPACE_ROOT / "layout" / "src" / "text3"
    add_directory_files(sections, text3_dir, "*.rs", "Text editing, cursor, selection")
    
    # Priority 2: macOS platform code
    sections.append("\n### MACOS PLATFORM CODE\n")
    macos_dir = WORKSPACE_ROOT / "dll" / "src" / "desktop" / "shell2" / "macos"
    add_directory_files(sections, macos_dir, "*.rs", "macOS window, events, NSTextInputClient")
    
    # Priority 3: Platform window abstraction
    sections.append("\n### PLATFORM WINDOW ABSTRACTION\n")
    add_file(sections, WORKSPACE_ROOT / "dll" / "src" / "desktop" / "platform_window_v2.rs", "Platform window abstraction")
    
    # Priority 4: Event handling
    sections.append("\n### EVENT HANDLING\n")
    add_file(sections, WORKSPACE_ROOT / "dll" / "src" / "desktop" / "event_v2.rs", "Event loop and dispatch")
    
    # Priority 5: Layout window (focus, cursor management)
    sections.append("\n### LAYOUT WINDOW\n")
    add_file(sections, WORKSPACE_ROOT / "layout" / "src" / "window.rs", "Window with focus/cursor management")
    
    # Priority 6: Layout managers
    sections.append("\n### LAYOUT MANAGERS\n")
    layout_managers = [
        "block_layout.rs",
        "text.rs",
        "inline_layout.rs",
        "flexbox.rs",
        "scrollbar.rs",
        "positioned.rs",
        "overflow.rs",
    ]
    for manager in layout_managers:
        add_file(sections, WORKSPACE_ROOT / "layout" / "src" / manager, f"Layout manager: {manager}")
    
    # Priority 7: Core layout lib
    add_file(sections, WORKSPACE_ROOT / "layout" / "src" / "lib.rs", "Layout lib root")
    
    # Priority 8: DOM and styled DOM
    sections.append("\n### DOM STRUCTURES\n")
    add_file(sections, WORKSPACE_ROOT / "core" / "src" / "dom.rs", "DOM structures")
    add_file(sections, WORKSPACE_ROOT / "core" / "src" / "styled_dom.rs", "Styled DOM")
    
    # Priority 9: Callbacks and focus
    sections.append("\n### CALLBACKS AND FOCUS\n")
    add_file(sections, WORKSPACE_ROOT / "core" / "src" / "callbacks.rs", "Callback system")
    
    # Priority 10: CSS parsing
    sections.append("\n### CSS\n")
    add_file(sections, WORKSPACE_ROOT / "css" / "src" / "lib.rs", "CSS parsing")
    add_file(sections, WORKSPACE_ROOT / "css" / "src" / "css_parser.rs", "CSS parser")
    
    # Priority 11: Test example
    sections.append("\n### TEST EXAMPLE\n")
    add_file(sections, WORKSPACE_ROOT / "examples" / "c" / "test_simple.cpp", "Test example")
    
    # Priority 12: Shell2 core
    sections.append("\n### SHELL2 CORE\n")
    add_file(sections, WORKSPACE_ROOT / "dll" / "src" / "desktop" / "shell2" / "mod.rs", "Shell2 module root")
    add_file(sections, WORKSPACE_ROOT / "dll" / "src" / "desktop" / "shell2" / "window.rs", "Shell2 window")
    
    # Priority 13: DLL main and lib
    sections.append("\n### DLL ENTRY\n")
    add_file(sections, WORKSPACE_ROOT / "dll" / "src" / "lib.rs", "DLL lib")
    add_file(sections, WORKSPACE_ROOT / "dll" / "main.rs", "DLL main")
    
    # Priority 14: Core lib
    sections.append("\n### CORE LIB\n")
    add_file(sections, WORKSPACE_ROOT / "core" / "src" / "lib.rs", "Core lib root")
    
    # Priority 15: Animation (might affect cursor blinking)
    add_file(sections, WORKSPACE_ROOT / "core" / "src" / "animation.rs", "Animation system")
    
    # Priority 16: Font handling
    sections.append("\n### FONT HANDLING\n")
    font_files = [
        WORKSPACE_ROOT / "layout" / "src" / "text3" / "font.rs",
        WORKSPACE_ROOT / "core" / "src" / "font.rs",
    ]
    for f in font_files:
        add_file(sections, f)
    
    # Priority 17: Additional layout files
    sections.append("\n### ADDITIONAL LAYOUT FILES\n")
    layout_files = [
        "display_list.rs",
        "hit_test.rs",
        "solver.rs",
        "sizing.rs",
    ]
    for lf in layout_files:
        path = WORKSPACE_ROOT / "layout" / "src" / lf
        if path.exists():
            add_file(sections, path)
    
    # Footer with line count
    sections.append(f"""

---

## ANALYSIS REQUEST

Please analyze the above code and provide:

1. **Root Cause Analysis** for each bug listed above
2. **Specific Code Fixes** with exact file paths and line numbers
3. **Architecture Recommendations** if the current design has fundamental issues
4. **Testing Strategy** to verify fixes

Focus especially on:
- Why "No focused node" error occurs when clicking on contenteditable text
- How focus should propagate from parent `<div tabindex=0>` to contenteditable child
- The relationship between IFC root nodes and contenteditable containers
- Why line wrapping happens when it shouldn't
- Scrollbar visibility and positioning logic

Total lines in this prompt: ~{total_lines}
""")
    
    # Write output
    output_path = OUTPUT_DIR / f"gemini_contenteditable_prompt_{timestamp}.md"
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(''.join(sections))
    
    print(f"Generated prompt: {output_path}")
    print(f"Total lines: {total_lines}")
    print(f"File size: {output_path.stat().st_size / 1024 / 1024:.2f} MB")
    
    # Also create a latest symlink/copy
    latest_path = OUTPUT_DIR / "gemini_contenteditable_prompt_latest.md"
    with open(latest_path, 'w', encoding='utf-8') as f:
        f.write(''.join(sections))
    print(f"Also saved to: {latest_path}")

if __name__ == "__main__":
    build_prompt()
