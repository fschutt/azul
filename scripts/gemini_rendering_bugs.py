#!/usr/bin/env python3
"""
Send rendering bugs analysis to Gemini 2.5 Pro for debugging.

Problems to solve:
1. TEXT INPUT STOPPED WORKING - clicking sets cursor but typing does nothing
2. Border/scrollbar offset - rendered ~10px detached from element  
3. white-space: nowrap is ignored - text wraps anyway
4. Missing glyphs - some characters render as boxes
5. Scrollbar sizing/positioning wrong

Usage: python3 gemini_rendering_bugs.py
"""

import subprocess
import os
import json
import datetime

GEMINI_API_KEY_FILE = "/Users/fschutt/Development/azul/GEMINI_API_KEY.txt"
AZUL_ROOT = "/Users/fschutt/Development/azul"

# Files to include, ordered by relevance - REDUCED SET
FILES_TO_INCLUDE = [
    # Text input flow - CRITICAL (smaller files first)
    ("dll/src/desktop/shell2/macos/events.rs", "macOS event handling - keyDown, insertText"),
    ("layout/src/managers/text_input.rs", "Text input manager - records changesets"),
    ("layout/src/managers/focus_cursor.rs", "Focus and cursor manager"),
    
    # Core window - PARTIAL (will read specific functions)
    ("layout/src/window.rs", "Main window logic - text input, focus"),
]

# Specific functions to extract from large files
FUNCTIONS_TO_EXTRACT = {
    "layout/src/window.rs": [
        "fn record_text_input",
        "fn get_text_before_textinput", 
        "fn process_mouse_click_for_selection",
        "fn handle_text_input",
        "fn process_callback_result_v2",
    ],
}

def read_file_content(filepath):
    """Read file content, return empty string if not found."""
    full_path = os.path.join(AZUL_ROOT, filepath)
    try:
        with open(full_path, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception as e:
        return f"// ERROR reading {filepath}: {e}"

def extract_functions(content, function_names):
    """Extract specific functions from a file."""
    lines = content.split('\n')
    extracted = []
    
    for func_name in function_names:
        # Find function start
        for i, line in enumerate(lines):
            if func_name in line and ('fn ' in line or 'pub fn ' in line):
                # Found function, now find its end
                brace_count = 0
                started = False
                func_lines = []
                
                for j in range(i, len(lines)):
                    func_lines.append(lines[j])
                    brace_count += lines[j].count('{')
                    brace_count -= lines[j].count('}')
                    if brace_count > 0:
                        started = True
                    if started and brace_count == 0:
                        break
                
                extracted.append(f"// Lines {i+1}-{i+len(func_lines)}")
                extracted.extend(func_lines)
                extracted.append("")
                break
    
    return '\n'.join(extracted)

def get_git_diff():
    """Get current git diff."""
    try:
        result = subprocess.run(
            ['git', 'diff', 'HEAD'],
            cwd=AZUL_ROOT,
            capture_output=True,
            text=True
        )
        return result.stdout
    except Exception as e:
        return f"// ERROR getting git diff: {e}"

def build_prompt():
    """Build the full prompt for Gemini."""
    
    bugs_description = """
# CRITICAL BUGS TO FIX

## Bug 1: TEXT INPUT STOPPED WORKING (HIGHEST PRIORITY)
- Clicking on contenteditable element now positions cursor (visible blinking cursor)
- But typing does NOTHING - no text appears
- This worked before the current diff changes
- The diff added:
  - Focus setting on click in process_mouse_click_for_selection
  - dirty_text_nodes check in get_text_before_textinput
  - scroll_selection_into_view after text edit
  - Removed duplicate record_text_input from macOS handle_key_down

Expected flow:
1. Click -> process_mouse_click_for_selection -> sets focus + cursor
2. Type -> macOS insertText: -> handle_text_input -> record_text_input
3. record_text_input checks focus_manager.get_focused_node()
4. If focused, records changeset, returns affected nodes
5. Callback fires, text appears

Current behavior:
- Step 1 works (cursor appears)
- Step 2-5: Nothing happens, no text appears

## Bug 2: Border/Scrollbar Offset (~10px detached)
- The border around elements is rendered ~10px away from the actual element
- The scrollbar at bottom is also offset, not at the window edge
- This suggests incorrect position calculation during display list building
- Probably related to padding/margin not being accounted for in border rect calculation

## Bug 3: white-space: nowrap Ignored
- CSS sets white-space: nowrap on .editor
- But text still wraps to multiple lines
- The text layout ignores the white-space constraint

## Bug 4: Missing Glyphs
- Some characters render as white boxes instead of glyphs
- Font loading or glyph caching issue
- Possibly related to font-family: monospace fallback

## Bug 5: Scrollbar Sizing/Position Wrong
- Scrollbar track size should be (width - 2*button_width), not just width
- Scrollbar should be hidden when overflow: auto and content fits
- Scrollbar is painted at wrong Y position (should be at bottom of scroll container)
"""

    prompt_parts = [bugs_description, "\n\n# CURRENT GIT DIFF\n\n```diff\n"]
    prompt_parts.append(get_git_diff())
    prompt_parts.append("\n```\n\n")
    
    prompt_parts.append("# SOURCE FILES\n\n")
    
    total_lines = 0
    for filepath, description in FILES_TO_INCLUDE:
        full_content = read_file_content(filepath)
        
        # Check if we should extract specific functions
        if filepath in FUNCTIONS_TO_EXTRACT:
            content = extract_functions(full_content, FUNCTIONS_TO_EXTRACT[filepath])
            prompt_parts.append(f"## {filepath} (EXTRACTED FUNCTIONS)\n")
        else:
            content = full_content
            prompt_parts.append(f"## {filepath}\n")
        
        lines = content.count('\n')
        total_lines += lines
        
        prompt_parts.append(f"// {description}\n")
        prompt_parts.append(f"// {lines} lines\n\n")
        prompt_parts.append(f"```rust\n{content}\n```\n\n")
        
        # Stop if we're getting too big
        if total_lines > 30000:
            prompt_parts.append(f"\n// Stopped at {total_lines} lines to stay within limits\n")
            break
    
    prompt_parts.append("""
# TASK

Analyze the code and identify the root cause of each bug. Provide specific fixes.

Focus especially on Bug 1 (text input stopped working) since that's a regression from the current diff.

For each bug, provide:
1. Root cause analysis
2. Specific file and line numbers
3. Exact code fix (diff format preferred)

Start with Bug 1 since it's the most critical regression.
""")
    
    return "".join(prompt_parts)

def send_to_gemini(prompt):
    """Send prompt to Gemini API."""
    
    # Read API key
    with open(GEMINI_API_KEY_FILE, 'r') as f:
        api_key = f.read().strip()
    
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key={api_key}"
    
    payload = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 65536,
        }
    }
    
    print(f"Sending {len(prompt)} chars to Gemini...")
    print(f"Estimated tokens: ~{len(prompt)//4}")
    
    # Save prompt for reference
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    prompt_file = os.path.join(AZUL_ROOT, "scripts", f"gemini_rendering_prompt_{timestamp}.md")
    with open(prompt_file, 'w') as f:
        f.write(prompt)
    print(f"Saved prompt to {prompt_file}")
    
    # Send request
    import urllib.request
    import urllib.error
    
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode('utf-8'),
        headers={'Content-Type': 'application/json'},
        method='POST'
    )
    
    try:
        with urllib.request.urlopen(req, timeout=600) as response:
            result = json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        error_body = e.read().decode('utf-8')
        print(f"HTTP Error {e.code}: {error_body}")
        return None
    except Exception as e:
        print(f"Error: {e}")
        return None
    
    # Extract response text
    try:
        response_text = result['candidates'][0]['content']['parts'][0]['text']
    except (KeyError, IndexError) as e:
        print(f"Error parsing response: {e}")
        print(f"Full response: {json.dumps(result, indent=2)}")
        return None
    
    # Save response
    response_file = os.path.join(AZUL_ROOT, "scripts", f"gemini_rendering_response_{timestamp}.md")
    with open(response_file, 'w') as f:
        f.write(response_text)
    print(f"Saved response to {response_file}")
    
    # Also save as "latest"
    latest_file = os.path.join(AZUL_ROOT, "scripts", "gemini_rendering_response_latest.md")
    with open(latest_file, 'w') as f:
        f.write(response_text)
    print(f"Also saved to {latest_file}")
    
    return response_text

def main():
    print("=" * 60)
    print("Gemini Rendering Bugs Analysis")
    print("=" * 60)
    
    prompt = build_prompt()
    print(f"\nPrompt size: {len(prompt)} chars (~{len(prompt)//4} tokens)")
    
    response = send_to_gemini(prompt)
    
    if response:
        print("\n" + "=" * 60)
        print("GEMINI RESPONSE (first 2000 chars):")
        print("=" * 60)
        print(response[:2000])
        if len(response) > 2000:
            print(f"\n... ({len(response) - 2000} more chars, see full response in file)")
    else:
        print("\nFailed to get response from Gemini")

if __name__ == "__main__":
    main()
