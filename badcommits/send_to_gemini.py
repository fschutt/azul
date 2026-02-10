#!/usr/bin/env python3
"""
Assembles and sends a comprehensive analysis prompt to Gemini 2.5 Pro
with full context about the Azul layout engine regressions.

Includes:
- All solver3/*.rs and text3/*.rs source files
- The diff files from badcommits/
- PNG screenshots as base64
- The REPORT.md problem description
- UA CSS and XML parser source
"""

import base64
import json
import os
import sys
import glob
import urllib.request
import urllib.error

API_KEY = open(os.path.join(os.path.dirname(__file__), "..", "GEMINI_API_KEY.txt")).read().strip()
AZUL_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")

MODEL = "gemini-2.5-pro"
ENDPOINT = f"https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent?key={API_KEY}"

def read_file(path):
    """Read a file relative to AZUL_ROOT."""
    full = os.path.join(AZUL_ROOT, path) if not os.path.isabs(path) else path
    with open(full, "r", errors="replace") as f:
        return f.read()

def read_binary(path):
    """Read a binary file and return base64."""
    full = os.path.join(AZUL_ROOT, path) if not os.path.isabs(path) else path
    with open(full, "rb") as f:
        return base64.b64encode(f.read()).decode("ascii")

def collect_source_files():
    """Collect all solver3 and text3 source files."""
    files = {}
    for pattern in ["layout/src/solver3/*.rs", "layout/src/text3/*.rs"]:
        for path in sorted(glob.glob(os.path.join(AZUL_ROOT, pattern))):
            rel = os.path.relpath(path, AZUL_ROOT)
            files[rel] = read_file(path)
    # Also include key files outside solver3/text3
    extra_files = [
        "core/src/ua_css.rs",
        "layout/src/xml/mod.rs",
    ]
    for f in extra_files:
        full = os.path.join(AZUL_ROOT, f)
        if os.path.exists(full):
            files[f] = read_file(full)
    return files

def collect_diffs():
    """Collect all diff files from badcommits/."""
    diffs = {}
    for path in sorted(glob.glob(os.path.join(AZUL_ROOT, "badcommits/*.diff"))):
        rel = os.path.relpath(path, AZUL_ROOT)
        diffs[rel] = read_file(path)
    return diffs

def collect_pngs():
    """Collect all PNG files from badcommits/ as base64."""
    pngs = {}
    for path in sorted(glob.glob(os.path.join(AZUL_ROOT, "badcommits/*.png"))):
        name = os.path.basename(path)
        pngs[name] = read_binary(path)
    return pngs

def build_prompt_text(source_files, diffs, report):
    """Build the text portion of the prompt."""

    text = """# Azul CSS Layout Engine — Regression Analysis Request

You are analyzing a CSS layout engine written in Rust called "Azul". It implements
CSS 2.2 block/inline formatting contexts, margin collapsing, float positioning,
and uses Taffy for Flex/Grid layout.

## Problem Statement

Three commits have introduced visual regressions that make the layout engine produce
incorrect output. We need you to:

1. Analyze the current state of the codebase (all source files provided below)
2. Compare the "before" and "after" screenshots for each regression
3. Identify the root causes based on the diffs and the CSS specification
4. Propose specific, W3C-conformant fixes

## The Three Regressions

### Regression 1: Commit `c33e94b0` broke `block-margin-collapse-complex-001`
- **Previous good commit:** `a017dcc2`
- **Commit message:** "fix(layout): preserve whitespace-only text nodes for CSS white-space handling"
- **Changed files:** `layout/src/solver3/fc.rs`, `layout/src/xml/mod.rs`
- **Symptom:** Extra vertical spacing between block elements due to whitespace text nodes
  creating anonymous inline boxes that prevent margin collapsing

### Regression 2: Commit `f1fcf27d` broke body background color propagation
- **Previous good commit:** `4bacfcac`
- **Commit message:** "Fix margin double-application bug for body with margin: 15vh auto"
- **Changed files:** `core/src/ua_css.rs`, `layout/src/solver3/cache.rs`, `layout/src/solver3/fc.rs`
- **Symptom:** Body background color no longer fills the viewport. Also broke margin collapsing
  by replacing `collapse_margins(parent, child)` with just `child_margin_top`.

### Regression 3: Commit `8e092a2e` broke `block-positioning-complex-001` completely
- **Previous good commit:** `72ab2a26`
- **Commit message:** "fix(layout): prevent double margin subtraction for root nodes"
- **Changed files:** `layout/src/solver3/fc.rs`, `layout/src/solver3/taffy_bridge.rs`
- **Symptom:** Block elements positioned completely wrong. The commit removed Pass 1
  (sizing) from `layout_bfc`, replacing it with just-in-time sizing that doesn't recursively
  lay out grandchildren.

### Common root cause context
All three regressions trace back to commit `1a3e5850` which added subtree layout caching
to fix an O(n²) performance issue with 300,000-node DOMs. This changed the layout execution
model from two-pass (sizing → positioning) to single-pass with memoization.

## Important Downstream Consumer Context

The layout engine is used by `printpdf` (HTML-to-PDF converter) and `git2pdf` (source code
to PDF). These consumers:
- Process DOMs with 10,000+ nodes (git2pdf code files)
- Use `white-space: pre-wrap` for code blocks
- Use Flex layout for title pages
- Use paged layout (FragmentationContext) for PDF pagination
- The O(n²) → O(n) caching improvement is essential for performance

## Screenshots

The screenshots below show the rendering output at different commits.
For `block-positioning-complex-001`:
- `chrome-reference`: The correct rendering (Chrome browser)
- `azul-at-72ab2a26`: Last good commit (matches Chrome)
- `azul-at-8e092a2e`: After regression 3 (broken positioning)
- `azul-at-4bacfcac`: Last good commit for bg color (matches Chrome)
- `azul-at-f1fcf27d`: After regression 2 (same as 4bacfcac visually but bg color issue)

For `block-margin-collapse-complex-001`:
- `chrome-reference`: The correct rendering (Chrome browser)
- `azul-at-a017dcc2`: Last good commit (matches Chrome)
- `azul-at-c33e94b0`: After regression 1 (extra spacing between blocks)

## Report from debugging session

"""
    text += report
    text += "\n\n"

    # Add diffs
    text += "## Diffs of the bad commits\n\n"
    for name, content in sorted(diffs.items()):
        text += f"### {name}\n\n```diff\n{content}\n```\n\n"

    # Add source files
    text += "## Current source code (solver3 + text3 + ua_css + xml parser)\n\n"
    for name, content in sorted(source_files.items()):
        text += f"### {name}\n\n```rust\n{content}\n```\n\n"

    text += """
## Your Task

Please analyze:
1. What is the root cause of each regression?
2. What CSS specification sections are being violated?
3. What is the correct fix for each regression that:
   - Is W3C conformant
   - Preserves the O(n) caching performance
   - Doesn't break `white-space: pre-wrap` behavior
   - Doesn't break Flex/Grid layout
   - Correctly implements CSS 2.1 § 14.2 canvas background propagation
4. Are there any other issues in the current codebase that you can identify?

Be specific about which functions need to change and how. Reference CSS specification
sections for every claim.
"""
    return text

def build_request(prompt_text, pngs):
    """Build the Gemini API request with text and inline images."""
    parts = []

    # Add text prompt
    parts.append({"text": prompt_text})

    # Add images
    for name, b64data in sorted(pngs.items()):
        parts.append({"text": f"\n[Screenshot: {name}]\n"})
        parts.append({
            "inline_data": {
                "mime_type": "image/png",
                "data": b64data
            }
        })

    request_body = {
        "contents": [{"parts": parts}],
        "generationConfig": {
            "temperature": 1.0,
            "maxOutputTokens": 65536,
            "thinkingConfig": {
                "thinkingBudget": 32768
            }
        }
    }
    return request_body

def save_prompt_for_review(prompt_text, pngs, output_path):
    """Save the prompt text (without base64 images) for human review."""
    with open(output_path, "w") as f:
        f.write(prompt_text)
        f.write("\n\n---\n\n")
        f.write(f"## Attached Images ({len(pngs)} PNG files)\n\n")
        for name in sorted(pngs.keys()):
            f.write(f"- {name} ({len(pngs[name])} bytes base64)\n")
    print(f"Prompt text saved to: {output_path}")

def send_to_gemini(request_body):
    """Send the request to Gemini API and return the response."""
    data = json.dumps(request_body).encode("utf-8")

    print(f"Sending request to Gemini ({len(data)/1024/1024:.1f} MB)...")

    req = urllib.request.Request(
        ENDPOINT,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST"
    )

    try:
        with urllib.request.urlopen(req, timeout=600) as resp:
            response_data = json.loads(resp.read().decode("utf-8"))
            return response_data
    except urllib.error.HTTPError as e:
        error_body = e.read().decode("utf-8")
        print(f"HTTP Error {e.code}: {error_body}", file=sys.stderr)
        sys.exit(1)

def extract_response_text(response):
    """Extract the text response from Gemini's response."""
    try:
        candidates = response.get("candidates", [])
        if not candidates:
            return "No candidates in response"
        parts = candidates[0].get("content", {}).get("parts", [])
        texts = [p.get("text", "") for p in parts if "text" in p]
        return "\n".join(texts)
    except (KeyError, IndexError) as e:
        return f"Error extracting response: {e}\n\nRaw: {json.dumps(response, indent=2)}"

def main():
    print("Collecting source files...")
    source_files = collect_source_files()
    print(f"  → {len(source_files)} source files")

    print("Collecting diffs...")
    diffs = collect_diffs()
    print(f"  → {len(diffs)} diff files")

    print("Collecting PNG screenshots...")
    pngs = collect_pngs()
    print(f"  → {len(pngs)} PNG files")

    print("Reading REPORT.md...")
    report = read_file("badcommits/REPORT.md")

    print("Building prompt...")
    prompt_text = build_prompt_text(source_files, diffs, report)
    print(f"  → Prompt text: {len(prompt_text)/1024:.0f} KB")

    # Save prompt for review
    prompt_review_path = os.path.join(AZUL_ROOT, "badcommits", "gemini_prompt_review.md")
    save_prompt_for_review(prompt_text, pngs, prompt_review_path)

    # Build full request with images
    request_body = build_request(prompt_text, pngs)

    if "--dry-run" in sys.argv:
        print("Dry run mode — not sending to Gemini.")
        print(f"Total request size: {len(json.dumps(request_body))/1024/1024:.1f} MB")
        return

    # Send to Gemini
    response = send_to_gemini(request_body)

    # Extract and save response
    response_text = extract_response_text(response)

    response_path = os.path.join(AZUL_ROOT, "badcommits", "gemini_response.md")
    with open(response_path, "w") as f:
        f.write("# Gemini Analysis Response\n\n")
        f.write(response_text)

    print(f"\nResponse saved to: {response_path}")
    print(f"Response length: {len(response_text)} chars")

    # Also print to stdout
    print("\n" + "=" * 80)
    print(response_text)

if __name__ == "__main__":
    main()
