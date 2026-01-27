#!/usr/bin/env python3
"""
Debug script to compare text_area.c (broken) vs scrollbar_drag.c (working)
and send analysis to Gemini.

The issue: text_area.c has incorrect content_size calculation (92px instead of ~400px),
while scrollbar_drag.c works correctly with 30 items.

Key difference:
- text_area.c: Single text node with \n characters (white-space: pre)
- scrollbar_drag.c: Multiple child div nodes

Also includes selection.c bug: selection rectangles at wrong visual offset.
"""

import subprocess
import json
import time
import os
import sys
import base64
import requests
from pathlib import Path

API_URL = "http://localhost:8765/"
GEMINI_API_KEY_FILE = Path(__file__).parent.parent / "GEMINI_API_KEY.txt"
OUTPUT_DIR = Path(__file__).parent.parent / "target" / "textarea_debug"

def read_api_key():
    if GEMINI_API_KEY_FILE.exists():
        return GEMINI_API_KEY_FILE.read_text().strip()
    return os.environ.get("GEMINI_API_KEY", "")

def debug_api_call(op, **params):
    """Call the Debug API"""
    payload = {"op": op, **params}
    try:
        resp = requests.post(API_URL, json=payload, timeout=10)
        return resp.json()
    except Exception as e:
        return {"error": str(e)}

def wait_for_server(timeout=10):
    """Wait for debug server to be ready"""
    start = time.time()
    while time.time() - start < timeout:
        try:
            resp = requests.get(API_URL, timeout=1)
            if resp.status_code == 200:
                return True
        except:
            pass
        time.sleep(0.5)
    return False

def start_test_binary(binary_path, name):
    """Start a test binary with AZUL_DEBUG=8765"""
    env = os.environ.copy()
    env["AZUL_DEBUG"] = "8765"
    
    print(f"  Starting {name}...")
    proc = subprocess.Popen(
        [binary_path],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(Path(binary_path).parent)
    )
    time.sleep(3)  # Wait for window to open
    
    if not wait_for_server():
        print(f"  ERROR: Debug server not responding for {name}")
        proc.kill()
        return None
    
    return proc

def collect_debug_info(name):
    """Collect comprehensive debug info from a running test"""
    info = {"name": name}
    
    # Get display list with scroll frame info
    display_list = debug_api_call("get_display_list")
    info["display_list"] = display_list
    
    # Extract scroll frame info
    if "data" in display_list and "value" in display_list["data"]:
        value = display_list["data"]["value"]
        info["total_items"] = value.get("total_items", 0)
        
        # Get scroll frame from clip_analysis
        clip_analysis = value.get("clip_analysis", {})
        operations = clip_analysis.get("operations", [])
        scroll_frames = [op for op in operations if "Scroll" in op.get("op", "")]
        info["scroll_frames"] = scroll_frames
    
    # Get DOM structure
    info["dom"] = debug_api_call("get_dom")
    
    # Get layout tree
    info["layout_tree"] = debug_api_call("get_layout_tree")
    
    # Get scrollable nodes
    info["scrollable_nodes"] = debug_api_call("get_scrollable_nodes")
    
    # Get scroll states
    info["scroll_states"] = debug_api_call("get_scroll_states")
    
    # Get selection state (for selection.c)
    info["selection_state"] = debug_api_call("get_selection_state")
    
    # Take screenshot
    screenshot = debug_api_call("take_screenshot")
    if "data" in screenshot and "screenshot" in screenshot.get("data", {}):
        info["screenshot_base64"] = screenshot["data"]["screenshot"]
    
    return info

def close_window():
    """Close the test window"""
    debug_api_call("close")
    time.sleep(1)

def get_recent_commits():
    """Get the 5 most recent commits"""
    try:
        result = subprocess.run(
            ["git", "log", "--oneline", "-5"],
            capture_output=True, text=True, cwd=str(Path(__file__).parent.parent)
        )
        return result.stdout.strip()
    except:
        return "Could not get commits"

def get_commit_details(n=3):
    """Get detailed commit info"""
    try:
        result = subprocess.run(
            ["git", "log", f"-{n}", "--format=%H%n%s%n%b%n---"],
            capture_output=True, text=True, cwd=str(Path(__file__).parent.parent)
        )
        return result.stdout.strip()
    except:
        return ""

def read_source_file(path):
    """Read a source file"""
    try:
        return Path(path).read_text()
    except:
        return f"Could not read {path}"

def send_to_gemini(prompt):
    """Send prompt to Gemini API"""
    api_key = read_api_key()
    if not api_key:
        print("ERROR: No Gemini API key found")
        return None
    
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key={api_key}"
    
    payload = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 16384
        }
    }
    
    print(f"  Sending {len(prompt)} chars to Gemini...")
    try:
        resp = requests.post(url, json=payload, timeout=300)
        if resp.status_code == 200:
            data = resp.json()
            if "candidates" in data and data["candidates"]:
                return data["candidates"][0]["content"]["parts"][0]["text"]
        else:
            print(f"  Gemini API error: {resp.status_code}")
            print(f"  {resp.text[:500]}")
    except Exception as e:
        print(f"  Gemini request failed: {e}")
    
    return None

def main():
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    
    print("=" * 60)
    print("Textarea vs Scrollbar Debug Analysis")
    print("=" * 60)
    
    base_dir = Path(__file__).parent.parent
    
    # Compile test binaries
    print("\n[1] Compiling test binaries...")
    
    binaries = {
        "text_area": base_dir / "tests/e2e/text_area_test",
        "scrollbar_drag": base_dir / "tests/e2e/scrollbar_drag",
        "selection": base_dir / "tests/e2e/selection"
    }
    
    for name, binary in binaries.items():
        src = base_dir / f"tests/e2e/{name.replace('_test', '')}.c"
        if not src.exists():
            src = base_dir / f"tests/e2e/{name}.c"
        
        if not binary.exists() or src.stat().st_mtime > binary.stat().st_mtime if binary.exists() else True:
            print(f"  Compiling {name}...")
            cmd = [
                "clang", str(src), "-o", str(binary),
                "-I", str(base_dir / "target/codegen"),
                "-L", str(base_dir / "target/release"),
                "-lazul", "-Wl,-rpath," + str(base_dir / "target/release")
            ]
            result = subprocess.run(cmd, capture_output=True, text=True)
            if result.returncode != 0:
                print(f"    ERROR: {result.stderr[:200]}")
    
    # Collect debug info from each test
    print("\n[2] Collecting debug info from tests...")
    
    all_info = {}
    
    # Kill any existing test processes
    subprocess.run(["pkill", "-f", "text_area|scrollbar|selection"], capture_output=True)
    time.sleep(1)
    
    for name, binary in binaries.items():
        if not binary.exists():
            print(f"  Skipping {name} - binary not found")
            continue
            
        print(f"\n  === {name} ===")
        proc = start_test_binary(str(binary), name)
        if proc:
            info = collect_debug_info(name)
            all_info[name] = info
            
            # Save screenshot
            if "screenshot_base64" in info:
                screenshot_path = OUTPUT_DIR / f"{name}_screenshot.png"
                with open(screenshot_path, "wb") as f:
                    f.write(base64.b64decode(info["screenshot_base64"]))
                print(f"  Saved screenshot to {screenshot_path}")
            
            close_window()
            proc.kill()
            time.sleep(1)
    
    # Save debug info
    debug_info_path = OUTPUT_DIR / "debug_info.json"
    with open(debug_info_path, "w") as f:
        # Remove screenshots from JSON (too large)
        info_copy = {}
        for name, info in all_info.items():
            info_copy[name] = {k: v for k, v in info.items() if k != "screenshot_base64"}
        json.dump(info_copy, f, indent=2)
    print(f"\n  Saved debug info to {debug_info_path}")
    
    # Build Gemini prompt
    print("\n[3] Building Gemini prompt...")
    
    recent_commits = get_recent_commits()
    commit_details = get_commit_details(3)
    
    # Read relevant source files
    source_files = {
        "text_area.c": base_dir / "tests/e2e/text_area.c",
        "scrollbar_drag.c": base_dir / "tests/e2e/scrollbar_drag.c",
        "selection.c": base_dir / "tests/e2e/selection.c",
        "get_scroll_content_size": None,  # We'll extract this
    }
    
    # Read the get_scroll_content_size function
    display_list_rs = base_dir / "layout/src/solver3/display_list.rs"
    try:
        content = display_list_rs.read_text()
        # Find the function
        start = content.find("fn get_scroll_content_size")
        if start != -1:
            end = content.find("\n}\n", start)
            source_files["get_scroll_content_size (display_list.rs)"] = content[start:end+2]
    except:
        pass
    
    prompt = f"""# Azul GUI Framework - Text Layout Debug Analysis

## Problem Summary

We have THREE bugs related to text layout and scroll frame sizing:

### Bug 1: text_area.c - Incorrect content_size calculation

The text area shows "Line 1: This is the first line of text." correctly at the top (Y-offset bug was fixed), 
BUT the scrollbar is missing because content_size is calculated as only ~92px instead of ~400px for 15 lines of text.

Key observation:
- content_size.height = 92.79843 (from PushScrollFrame in display list)
- viewport height = 230.0 (bounds)
- Expected: content_size should be ~400px (15 lines × ~26px line height with 36px font)

The text is:
```
Line 1: This is the first line of text.
Line 2: Second line here.
Line 3: Third line of text.
...
Line 15: Final line of the text area.
```

### Bug 2: scrollbar_drag.c - WORKS CORRECTLY

This test creates 30 items as separate DOM nodes (<div> children), and scrolling works perfectly.
The scrollbar appears and content_size is calculated correctly.

### Bug 3: selection.c - Selection rectangles at wrong Y-offset

When selecting text across paragraphs, the selection rectangles (blue highlight) and cursor line 
appear at the WRONG visual position. The selection logic seems correct (it selects the right text),
but the VISUAL rendering of selection rectangles is offset.

## Recent Git Commits

```
{recent_commits}
```

## Commit Details

```
{commit_details}
```

## Source Code

### text_area.c (BROKEN - uses single text node with \\n)
```c
{read_source_file(source_files["text_area.c"])}
```

### scrollbar_drag.c (WORKS - uses multiple child div nodes)
```c
{read_source_file(source_files["scrollbar_drag.c"])}
```

### selection.c (selection offset bug)
```c
{read_source_file(source_files["selection.c"])}
```

### get_scroll_content_size function (layout/src/solver3/display_list.rs)
```rust
{source_files.get("get_scroll_content_size (display_list.rs)", "Not found")}
```

## Debug Data

### text_area (BROKEN)
```json
{json.dumps(all_info.get("text_area", {}), indent=2)[:15000]}
```

### scrollbar_drag (WORKS)
```json
{json.dumps(all_info.get("scrollbar_drag", {}), indent=2)[:15000]}
```

### selection (offset bug)
```json
{json.dumps(all_info.get("selection", {}), indent=2)[:10000]}
```

## Analysis Questions

1. **text_area content_size bug**: Why does get_scroll_content_size() return ~92px instead of ~400px for a text node with 15 lines?
   - Is the inline_layout_result being calculated correctly for the text node?
   - Is `white-space: pre` being respected for height calculation?
   - Is there a difference in how text nodes with \\n vs block children are measured?

2. **Difference between text_area and scrollbar_drag**:
   - text_area uses a SINGLE text node with \\n newlines and `white-space: pre`
   - scrollbar_drag uses MULTIPLE child div nodes
   - Why does the multi-div approach work but the text node approach fails?

3. **selection.c visual offset bug**:
   - The selection rectangles appear at the wrong Y-position
   - This is likely related to the same coordinate space transformation issue we fixed
   - Are selection rectangles being transformed correctly from Window to ScrollFrame space?

4. **Recommended fixes**:
   - What specific code changes would fix the content_size calculation for text nodes with \\n?
   - Where in the codebase should we look to fix the selection rectangle offset?

Please provide specific code locations and fix suggestions.
"""

    # Save prompt
    prompt_path = OUTPUT_DIR / "gemini_prompt.txt"
    with open(prompt_path, "w") as f:
        f.write(prompt)
    print(f"  Saved prompt ({len(prompt)} chars) to {prompt_path}")
    
    # Send to Gemini
    print("\n[4] Sending to Gemini...")
    response = send_to_gemini(prompt)
    
    if response:
        response_path = OUTPUT_DIR / "gemini_response.md"
        with open(response_path, "w") as f:
            f.write(response)
        print(f"  Saved response to {response_path}")
        print("\n" + "=" * 60)
        print("GEMINI ANALYSIS")
        print("=" * 60)
        print(response[:3000])
        if len(response) > 3000:
            print(f"\n... (truncated, see {response_path} for full response)")
    else:
        print("  No response from Gemini")
    
    print("\n[Done]")

if __name__ == "__main__":
    main()
