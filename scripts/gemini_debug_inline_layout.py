#!/usr/bin/env python3
"""
Enhanced Debug Script for Inline Layout and White-Space Handling

This script:
1. Compiles and runs text_area.c and scrollbar_drag.c
2. Collects detailed debug info about:
   - InlineContent collection
   - LogicalItem creation  
   - Line breaking behavior
   - white-space: pre handling
   - \n character processing
3. Analyzes source code for newline handling
4. Sends comprehensive analysis to Gemini for W3C conformance check
"""

import subprocess
import json
import time
import os
import sys
import signal
import requests
from pathlib import Path

# Paths
ROOT = Path(__file__).parent.parent.absolute()
BUILD_DIR = ROOT / "target" / "release"
E2E_DIR = ROOT / "tests" / "e2e"
OUTPUT_DIR = ROOT / "target" / "inline_debug"
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

# Read Gemini API key
try:
    API_KEY = (ROOT / "GEMINI_API_KEY.txt").read_text().strip()
except:
    print("ERROR: GEMINI_API_KEY.txt not found")
    sys.exit(1)

def run_command(cmd, cwd=None, timeout=60):
    """Run a command and return output"""
    print(f"  Running: {' '.join(cmd[:3])}...")
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=cwd)
        if result.returncode != 0:
            print(f"    FAILED: {result.stderr[:200]}")
        return result.returncode == 0, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        print(f"    TIMEOUT after {timeout}s")
        return False, "", "timeout"

def compile_test(name):
    """Compile a test binary"""
    c_file = E2E_DIR / f"{name}.c"
    binary = E2E_DIR / name
    
    cmd = [
        "cc", str(c_file),
        f"-I{ROOT}/target/codegen/v2/",
        f"-L{BUILD_DIR}",
        "-lazul",
        "-o", str(binary),
        f"-Wl,-rpath,{BUILD_DIR}"
    ]
    
    success, stdout, stderr = run_command(cmd, cwd=E2E_DIR)
    return success, binary

def start_test_binary(binary):
    """Start a test binary with debug API enabled"""
    env = os.environ.copy()
    env["AZUL_DEBUG"] = "8765"
    
    proc = subprocess.Popen(
        [str(binary)],
        cwd=binary.parent,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    
    # Wait for debug API to be ready
    time.sleep(2.0)
    return proc

def debug_api_call(command, params=None):
    """Call the debug API"""
    try:
        payload = {"command": command}
        if params:
            payload["params"] = params
        
        resp = requests.post(
            "http://127.0.0.1:8765/debug",
            json=payload,
            timeout=5
        )
        return resp.json() if resp.status_code == 200 else None
    except:
        return None

def collect_debug_info():
    """Collect all available debug info"""
    info = {}
    
    commands = [
        "get_display_list",
        "get_dom",
        "get_layout_tree", 
        "get_scroll_states",
        "get_selection_state",
        "get_styled_dom",
    ]
    
    for cmd in commands:
        result = debug_api_call(cmd)
        if result:
            info[cmd] = result
    
    return info

def read_source_files():
    """Read relevant source files for analysis"""
    files_to_read = [
        # Main inline layout code
        ("layout/src/text3/cache.rs", "create_logical_items"),
        ("layout/src/text3/cache.rs", "InlineContent"),
        ("layout/src/text3/cache.rs", "InlineBreak"),
        ("layout/src/text3/cache.rs", "LogicalItem"),
        ("layout/src/text3/cache.rs", "perform_fragment_layout"),
        
        # Content collection
        ("layout/src/solver3/fc.rs", "collect_and_measure_inline_content"),
        ("layout/src/solver3/fc.rs", "InlineContent"),
        ("layout/src/solver3/fc.rs", "white_space"),
        
        # Scroll content size calculation  
        ("layout/src/solver3/display_list.rs", "get_scroll_content_size"),
        ("layout/src/solver3/display_list.rs", "inline_layout_result"),
        
        # Sizing
        ("layout/src/solver3/sizing.rs", "collect_inline_content"),
    ]
    
    source_excerpts = {}
    
    for file_path, search_term in files_to_read:
        full_path = ROOT / file_path
        if not full_path.exists():
            continue
            
        try:
            content = full_path.read_text()
            lines = content.split('\n')
            
            # Find relevant sections around the search term
            relevant_lines = []
            for i, line in enumerate(lines):
                if search_term.lower() in line.lower():
                    # Get 20 lines before and 40 lines after
                    start = max(0, i - 20)
                    end = min(len(lines), i + 40)
                    snippet = '\n'.join(f"{j+1}: {lines[j]}" for j in range(start, end))
                    relevant_lines.append(f"\n### Match at line {i+1}:\n```rust\n{snippet}\n```")
            
            if relevant_lines:
                source_excerpts[f"{file_path} ({search_term})"] = '\n'.join(relevant_lines[:3])  # Max 3 matches
        except Exception as e:
            print(f"  Failed to read {file_path}: {e}")
    
    return source_excerpts

def analyze_newline_handling():
    """Specifically analyze how \n characters are handled"""
    analysis = {}
    
    # Check create_logical_items for \n handling
    cache_rs = ROOT / "layout/src/text3/cache.rs"
    if cache_rs.exists():
        content = cache_rs.read_text()
        
        # Check for newline splitting
        if "split('\\n')" in content or "split(\"\\n\")" in content:
            analysis["newline_split"] = "FOUND: Code splits text on newline"
        else:
            analysis["newline_split"] = "NOT FOUND: No explicit newline splitting in cache.rs"
        
        # Check for InlineBreak creation from \n
        if "InlineBreak" in content and ("\\n" in content or "'\\n'" in content):
            analysis["inline_break_from_newline"] = "POSSIBLE: InlineBreak and \\n both referenced"
        else:
            analysis["inline_break_from_newline"] = "UNCLEAR: InlineBreak exists but unclear if created from \\n"
        
        # Check for white-space: pre handling
        if "TextWrap::NoWrap" in content:
            analysis["nowrap_handling"] = "FOUND: TextWrap::NoWrap is handled"
        else:
            analysis["nowrap_handling"] = "NOT FOUND: No TextWrap::NoWrap handling"
    
    # Check fc.rs for content collection
    fc_rs = ROOT / "layout/src/solver3/fc.rs"
    if fc_rs.exists():
        content = fc_rs.read_text()
        
        if "LineBreak" in content:
            analysis["fc_linebreak"] = "FOUND: fc.rs references LineBreak"
        else:
            analysis["fc_linebreak"] = "NOT FOUND: No LineBreak in fc.rs"
        
        if "StyleWhiteSpace::Pre" in content:
            analysis["fc_whitespace_pre"] = "FOUND: fc.rs handles StyleWhiteSpace::Pre"
            # Find the mapping
            idx = content.find("StyleWhiteSpace::Pre")
            if idx > 0:
                snippet = content[max(0, idx-100):min(len(content), idx+200)]
                analysis["fc_whitespace_pre_code"] = snippet
    
    return analysis

def find_w3c_white_space_spec():
    """Get W3C CSS Text spec summary for white-space: pre"""
    return """
## W3C CSS Text Module Level 3 - white-space Property
https://www.w3.org/TR/css-text-3/#white-space-property

### white-space: pre

The 'pre' value:
- PRESERVES newline characters as forced line breaks
- PRESERVES sequences of white space  
- Does NOT wrap lines at unforced break opportunities

Key requirement: "Newlines in the source will be honored as forced line breaks."

This means when `white-space: pre` is set:
1. Each `\\n` character MUST create a new line box
2. The inline layout MUST NOT collapse multiple `\\n` into one
3. Content height MUST include ALL lines, not just the first

### Line Breaking Requirements
Per CSS Text Level 3, Section 5.1:
- Line breaks are ONLY allowed at "soft wrap opportunities" unless forced
- `\\n` in `white-space: pre` is a FORCED line break
- A forced line break terminates the current line box and starts a new one
"""

def build_prompt(test_name, debug_info, source_excerpts, newline_analysis):
    """Build comprehensive prompt for Gemini"""
    
    prompt = f"""# Bug Analysis Request: Inline Layout with white-space: pre

## Problem Summary

Testing the `{test_name}` binary reveals:
- Text with `\\n` characters and `white-space: pre` doesn't create multiple lines
- `content_size.height` is calculated for ONLY the first line (~92px instead of ~400px for 15 lines)
- Scrollbars don't appear because content appears to fit

Meanwhile, `scrollbar_drag.c` with multiple `<div>` children works correctly.

## Debug Data from {test_name}

### Display List Summary
```json
{json.dumps(debug_info.get('get_display_list', {}), indent=2)[:2000]}
```

### Layout Tree Summary  
```json
{json.dumps(debug_info.get('get_layout_tree', {}), indent=2)[:2000]}
```

### Scroll States
```json
{json.dumps(debug_info.get('get_scroll_states', {}), indent=2)}
```

## Newline Handling Analysis

```json
{json.dumps(newline_analysis, indent=2)}
```

{find_w3c_white_space_spec()}

## Source Code Excerpts

"""
    
    for file_key, excerpt in source_excerpts.items():
        prompt += f"\n### {file_key}\n{excerpt[:3000]}\n"
    
    prompt += """

## Questions for Analysis

1. **Where is the bug?** 
   - Is `\\n` being converted to `InlineContent::LineBreak` anywhere?
   - Is `create_logical_items` splitting text on `\\n` characters?
   
2. **W3C Conformance Check:**
   - Does our `white-space: pre` implementation meet the CSS Text Level 3 spec?
   - Are we correctly treating `\\n` as a forced line break?

3. **Root Cause:**
   - Why does `glyph_count: 40` (first line only) indicate layout stopped at first `\\n`?
   - Where in the pipeline is text being truncated?

4. **Fix Location:**
   - Exactly which function needs to be modified?
   - Should `\\n` splitting happen in `collect_and_measure_inline_content` or `create_logical_items`?

5. **Selection Rectangle Bug (Bug 3):**
   - After fixing Bug 1, selection rectangles may still be offset
   - Where is the coordinate space transformation missing for selection?

Please provide:
1. The exact code location of the bug
2. A specific fix (Rust code)
3. Whether this is W3C conformant after the fix
"""
    
    return prompt

def call_gemini(prompt, output_file):
    """Call Gemini API and save response"""
    print(f"\n[3/4] Calling Gemini 2.5 Pro API...")
    print(f"  Prompt length: {len(prompt)} chars")
    
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key={API_KEY}"
    
    payload = {
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 12000,
        }
    }
    
    try:
        response = requests.post(url, json=payload, timeout=120)
        
        if response.status_code != 200:
            print(f"  ERROR: {response.status_code}")
            print(response.text[:500])
            return None
        
        result = response.json()
        text = result.get("candidates", [{}])[0].get("content", {}).get("parts", [{}])[0].get("text", "")
        
        # Save response
        output_file.write_text(text)
        print(f"  Response saved to: {output_file}")
        
        return text
        
    except Exception as e:
        print(f"  ERROR: {e}")
        return None

def main():
    print("=" * 60)
    print("Enhanced Inline Layout Debug Script")
    print("=" * 60)
    
    # Step 1: Build the library
    print("\n[0/4] Building azul-dll...")
    success, _, _ = run_command([
        "cargo", "build", "--release", "-p", "azul-dll", "--features", "build-dll"
    ], cwd=ROOT, timeout=300)
    
    if not success:
        print("  Build failed!")
        return
    
    # Step 1: Compile tests
    print("\n[1/4] Compiling test binaries...")
    
    success, text_area_bin = compile_test("text_area")
    if not success:
        print("  Failed to compile text_area.c")
        return
    
    # Step 2: Run text_area and collect debug info
    print("\n[2/4] Running text_area and collecting debug info...")
    
    proc = start_test_binary(text_area_bin)
    debug_info = collect_debug_info()
    
    # Take screenshot
    screenshot_result = debug_api_call("take_screenshot")
    if screenshot_result and "image_base64" in screenshot_result:
        import base64
        img_data = base64.b64decode(screenshot_result["image_base64"])
        (OUTPUT_DIR / "text_area_screenshot.png").write_bytes(img_data)
        print("  Screenshot saved")
    
    # Kill the process
    proc.terminate()
    try:
        proc.wait(timeout=3)
    except:
        proc.kill()
    
    # Save debug info
    (OUTPUT_DIR / "debug_info.json").write_text(json.dumps(debug_info, indent=2))
    
    # Analyze source code
    print("\n  Analyzing source code...")
    source_excerpts = read_source_files()
    newline_analysis = analyze_newline_handling()
    
    # Build and send prompt
    prompt = build_prompt("text_area", debug_info, source_excerpts, newline_analysis)
    
    # Save prompt
    (OUTPUT_DIR / "gemini_prompt.txt").write_text(prompt)
    print(f"  Prompt saved ({len(prompt)} chars)")
    
    # Call Gemini
    response_file = OUTPUT_DIR / "gemini_analysis.md"
    response = call_gemini(prompt, response_file)
    
    if response:
        print("\n[4/4] Analysis complete!")
        print(f"\nResults saved to: {OUTPUT_DIR}")
        print("\n--- Gemini Analysis Preview ---")
        print(response[:2000])
        print("...")
    else:
        print("\n[4/4] Gemini API call failed")

if __name__ == "__main__":
    main()
