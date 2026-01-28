#!/usr/bin/env python3
"""
Gemini API script to debug contenteditable text input bugs.

Sends relevant source files, git diffs, bug analysis, and test code to Gemini
for comprehensive debugging analysis.

Usage:
    cd scripts
    python gemini_contenteditable_debug.py
"""

import os
import subprocess
import json
import datetime
import google.generativeai as genai

# Configuration
WORKSPACE_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API_KEY_FILE = os.path.join(WORKSPACE_ROOT, "GEMINI_API_KEY.txt")
OUTPUT_DIR = WORKSPACE_ROOT + "/scripts"

# Files to include in the prompt
RELEVANT_FILES = [
    # Event processing
    "dll/src/desktop/shell2/common/event_v2.rs",
    "dll/src/desktop/shell2/common/debug_server.rs",
    
    # Callbacks and layout window
    "layout/src/callbacks.rs",
    "layout/src/window.rs",
    
    # Text input system
    "core/src/widgets/text_input.rs",
    
    # Text processing
    "layout/src/text3/cache.rs",
    "layout/src/text3/edit.rs",
    "layout/src/text3/selection.rs",
    
    # Cursor handling
    "layout/src/cursor.rs",
    "layout/src/focus_cursor.rs",
    
    # Layout solver
    "layout/src/solver3/mod.rs",
    "layout/src/solver3/fc.rs",
    
    # Test file
    "tests/e2e/contenteditable.c",
]

# Bug analysis document
BUG_ANALYSIS_FILE = "scripts/CONTENTEDITABLE_BUGS_ANALYSIS.md"

def read_file_content(filepath):
    """Read file content, return None if file doesn't exist or is too large."""
    full_path = os.path.join(WORKSPACE_ROOT, filepath)
    if not os.path.exists(full_path):
        print(f"  Warning: File not found: {filepath}")
        return None
    
    # Skip files larger than 100KB to avoid token limits
    size = os.path.getsize(full_path)
    if size > 100 * 1024:
        print(f"  Warning: File too large ({size} bytes), truncating: {filepath}")
        with open(full_path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read(100 * 1024)
        return content + f"\n\n... [FILE TRUNCATED - original size: {size} bytes] ..."
    
    try:
        with open(full_path, 'r', encoding='utf-8', errors='replace') as f:
            return f.read()
    except Exception as e:
        print(f"  Error reading {filepath}: {e}")
        return None

def get_git_diffs(num_commits=10):
    """Get git diffs for the last N commits."""
    try:
        result = subprocess.run(
            ['git', 'log', f'-{num_commits}', '--patch', '--stat'],
            cwd=WORKSPACE_ROOT,
            capture_output=True,
            text=True,
            timeout=30
        )
        return result.stdout
    except Exception as e:
        print(f"Error getting git diffs: {e}")
        return ""

def get_git_log_oneline(num_commits=15):
    """Get git log summary."""
    try:
        result = subprocess.run(
            ['git', 'log', '--oneline', f'-{num_commits}'],
            cwd=WORKSPACE_ROOT,
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout
    except Exception as e:
        print(f"Error getting git log: {e}")
        return ""

def build_prompt():
    """Build the complete prompt for Gemini."""
    
    prompt_parts = []
    
    # Header
    prompt_parts.append("""# Azul GUI Framework - ContentEditable Text Input Bug Analysis

## Context

I'm developing Azul, a desktop GUI framework in Rust with C bindings. I've implemented 
a contenteditable text input system but it has several critical bugs. I need your help
analyzing the code and identifying the root causes.

## The Bugs

After implementing the text input system, the following bugs were observed:

1. **Cursor not appearing on click**: Clicking on a contenteditable element focuses it
   (blue outline appears) but the text cursor doesn't start blinking.

2. **Double input bug**: Pressing 'j' inserts 'jj' (duplicated character).

3. **Wrong text input affected**: When typing in the first input, the SECOND input
   gets modified instead of the first one.

4. **Mouse move triggers horrible resize**: Moving the mouse causes the first text
   input to resize incorrectly and text explodes across multiple lines.

5. **Line breaking bug**: Single-line input breaks onto many lines, ignoring
   `white-space: nowrap` CSS property.

6. **No scroll into view**: Text doesn't scroll into view when typing.

## Debug Output Analysis

The debug output shows:
- `old_text` is ALWAYS "Hello World - Click here and type!" (34 chars) - it never updates
- This suggests the text input's internal state is not being updated between keystrokes
- The text should be accumulating, but it's resetting each time

## Architecture Overview

The text input flow should be:
1. OS key event → debug_server.rs or platform event handler
2. `create_text_input()` creates a `CallbackChange::CreateTextInput`
3. `apply_callback_changes()` in window.rs processes it
4. `process_text_input()` is called, which calls `process_text_input_on_focused()`
5. Result is stored in `text_input_triggered` field
6. `process_callback_result_v2()` in event_v2.rs invokes user callbacks
7. User callback receives `PendingTextEdit` via `CallbackInfo::getTextChangeset()`

""")
    
    # Git log summary
    prompt_parts.append("## Recent Git Commits\n\n```\n")
    prompt_parts.append(get_git_log_oneline())
    prompt_parts.append("```\n\n")
    
    # Source files
    prompt_parts.append("## Relevant Source Files\n\n")
    
    for filepath in RELEVANT_FILES:
        content = read_file_content(filepath)
        if content:
            prompt_parts.append(f"### {filepath}\n\n```rust\n{content}\n```\n\n")
            print(f"  Added: {filepath} ({len(content)} bytes)")
    
    # Bug analysis document
    bug_analysis = read_file_content(BUG_ANALYSIS_FILE)
    if bug_analysis:
        prompt_parts.append(f"## Bug Analysis Document\n\n{bug_analysis}\n\n")
        print(f"  Added bug analysis document")
    
    # Git diffs (truncated to avoid token limits)
    prompt_parts.append("## Recent Git Diffs (Last 10 Commits)\n\n")
    git_diffs = get_git_diffs(10)
    if len(git_diffs) > 50000:  # Truncate if too long
        git_diffs = git_diffs[:50000] + "\n\n... [DIFFS TRUNCATED] ..."
    prompt_parts.append(f"```diff\n{git_diffs}\n```\n\n")
    print(f"  Added git diffs ({len(git_diffs)} bytes)")
    
    # Analysis request
    prompt_parts.append("""## Analysis Request

Please analyze the code and help me identify:

1. **Root Cause of Double Input**: Why is each keypress inserting the character twice?
   Look at how `text_input_triggered` flows through the system and where callbacks
   might be invoked twice.

2. **Root Cause of Wrong Input Affected**: Why does typing in input 1 modify input 2?
   Look at how `dom_node_id` is tracked and whether the focus system is correct.

3. **Root Cause of State Not Updating**: Why is `old_text` always the original text?
   Look at how `TextInputState` is stored and updated between frames.

4. **Root Cause of Cursor Not Appearing**: Why doesn't the cursor blink after click?
   Look at how focus events trigger cursor initialization.

5. **Root Cause of Layout Issues**: Why does the single-line input wrap to multiple lines?
   Look at how `white-space: nowrap` is handled in the layout solver.

For each bug, please:
- Identify the specific file(s) and function(s) involved
- Explain the flow of data/control that leads to the bug
- Suggest a concrete fix with code changes

Focus especially on the interaction between:
- `process_text_input()` in window.rs
- `text_input_triggered` propagation
- `invoke_callbacks_v2()` and `process_callback_result_v2()` in event_v2.rs
- How `CallbackChange::CreateTextInput` is processed
""")
    
    return "".join(prompt_parts)

def send_to_gemini(prompt):
    """Send prompt to Gemini and get response."""
    
    # Read API key
    if not os.path.exists(API_KEY_FILE):
        print(f"Error: API key file not found: {API_KEY_FILE}")
        return None
    
    with open(API_KEY_FILE, 'r') as f:
        api_key = f.read().strip()
    
    if not api_key:
        print("Error: API key is empty")
        return None
    
    # Configure Gemini
    genai.configure(api_key=api_key)
    
    # Use Gemini 2.5 Pro for deep analysis
    model = genai.GenerativeModel('gemini-2.5-pro')
    
    print(f"\nSending prompt to Gemini ({len(prompt)} chars)...")
    print("This may take a few minutes for complex analysis...")
    
    try:
        response = model.generate_content(
            prompt,
            generation_config=genai.types.GenerationConfig(
                max_output_tokens=32000,
                temperature=0.2,  # Lower temperature for more focused analysis
            )
        )
        return response.text
    except Exception as e:
        print(f"Error calling Gemini API: {e}")
        return None

def main():
    print("=" * 60)
    print("Azul ContentEditable Debug - Gemini Analysis")
    print("=" * 60)
    print()
    
    # Build prompt
    print("Building prompt...")
    prompt = build_prompt()
    
    # Save prompt for reference
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    prompt_file = os.path.join(OUTPUT_DIR, f"gemini_contenteditable_prompt_{timestamp}.md")
    with open(prompt_file, 'w', encoding='utf-8') as f:
        f.write(prompt)
    print(f"\nPrompt saved to: {prompt_file}")
    print(f"Prompt size: {len(prompt)} characters")
    
    # Send to Gemini
    response = send_to_gemini(prompt)
    
    if response:
        # Save response
        response_file = os.path.join(OUTPUT_DIR, f"gemini_contenteditable_response_{timestamp}.md")
        with open(response_file, 'w', encoding='utf-8') as f:
            f.write(response)
        print(f"\nResponse saved to: {response_file}")
        
        # Also save as "latest"
        latest_file = os.path.join(OUTPUT_DIR, "gemini_contenteditable_response_latest.md")
        with open(latest_file, 'w', encoding='utf-8') as f:
            f.write(response)
        print(f"Response also saved to: {latest_file}")
        
        # Print summary
        print("\n" + "=" * 60)
        print("Analysis Complete!")
        print("=" * 60)
        print(f"\nResponse length: {len(response)} characters")
        print("\nFirst 500 characters of response:")
        print("-" * 40)
        print(response[:500] + "..." if len(response) > 500 else response)
    else:
        print("\nFailed to get response from Gemini")
        return 1
    
    return 0

if __name__ == "__main__":
    exit(main())
