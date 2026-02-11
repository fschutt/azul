#!/usr/bin/env python3
"""
Reads the debug dump produced by collect_hello_world_debug.sh, assembles a
large Gemini prompt with all solver3 source files + related files + git diff,
and optionally sends it to Gemini.

Usage:
  # Step 1: collect debug data (bash)
  bash scripts/collect_hello_world_debug.sh

  # Step 2: assemble prompt (dry-run)
  python3 scripts/hello_world_debug_gemini.py

  # Step 3: send to Gemini
  python3 scripts/hello_world_debug_gemini.py --send
"""

import argparse
import json
import os
import sys
import subprocess
import urllib.request
import urllib.error
from typing import Any, Dict, List, Tuple

AZUL_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
GEMINI_API_URL = "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent"


def read_text(path: str) -> str:
    with open(path, "r", errors="replace") as f:
        return f.read()


def json_post(url: str, payload: Dict[str, Any], timeout: int = 30) -> Dict[str, Any]:
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def load_debug_dump(dump_path: str) -> Dict[str, Any]:
    """Load the combined JSON dump produced by collect_hello_world_debug.sh."""
    print(f"Loading debug dump from: {dump_path}")
    with open(dump_path, "r") as f:
        data = json.load(f)
    print(f"  → {len(data)} keys: {', '.join(sorted(data.keys()))}")
    return data


def collect_solver3_files() -> List[Tuple[str, str]]:
    result: List[Tuple[str, str]] = []
    root = os.path.join(AZUL_ROOT, "layout", "src", "solver3")
    for dirpath, _, filenames in os.walk(root):
        for name in sorted(filenames):
            if name.endswith(".rs"):
                full = os.path.join(dirpath, name)
                rel = os.path.relpath(full, AZUL_ROOT)
                result.append((rel, read_text(full)))
    return result


def collect_related_files() -> List[Tuple[str, str]]:
    files = [
        "examples/c/hello-world.c",
        "layout/src/text3/cache.rs",
        "layout/src/widgets/titlebar.rs",
        "dll/src/desktop/shell2/common/layout_v2.rs",
        "dll/src/desktop/shell2/common/event_v2.rs",
        "core/src/window.rs",
        "dll/src/desktop/shell2/macos/mod.rs",
        "dll/src/desktop/shell2/windows/wcreate.rs",
    ]
    result = []
    for f in files:
        full = os.path.join(AZUL_ROOT, f)
        if os.path.exists(full):
            result.append((f, read_text(full)))
    return result


def collect_git_diff() -> str:
    import subprocess

    try:
        out = subprocess.check_output([
            "git",
            "-C",
            AZUL_ROOT,
            "diff",
        ])
        return out.decode("utf-8", errors="replace")
    except Exception:
        return ""


def build_problem_statement() -> str:
    return (
        "## Observed Problems in hello-world\n\n"
        "- Counter and button should be vertically stacked, but appear horizontally misaligned.\n"
        "- Button is not properly clickable anymore.\n"
        "- Title is cut off and appears clipped by content size.\n"
        "- Title and content are ~8px too low. If the titlebar is not drawn, the default padding\n"
        "  likely needs to shift: titlebar visually outside <body>, but inside <html>, and padding\n"
        "  should be on <body> not <html>.\n"
        "- layout/titlebar.rs should include CSD maximize/minimize/close button configuration on Wayland,\n"
        "  with callbacks and SystemStyle theming.\n"
    )


def build_prompt(debug_info: Dict[str, Any], source_files: List[Tuple[str, str]], diff_text: str) -> str:
    prompt = "# Azul hello-world layout debugging\n\n"
    prompt += "You are debugging a layout regression in the Azul hello-world C example.\n"
    prompt += build_problem_statement()
    fence = "```"
    prompt += "\n## Debug API dump (JSON)\n\n" + fence + "json\n"
    prompt += json.dumps(debug_info, indent=2)
    prompt += "\n" + fence + "\n\n"

    if diff_text.strip():
        prompt += "## Working tree diff\n\n" + fence + "diff\n"
        prompt += diff_text
        prompt += "\n" + fence + "\n\n"

    prompt += "## Source code (solver3 + related files)\n\n"
    for path, content in source_files:
        prompt += f"### {path}\n\n" + fence + f"rust\n{content}\n" + fence + "\n\n"

    prompt += "## Task\n\n"
    prompt += (
        "1. Identify why the title is clipped and offset, and why content is 8px too low.\n"
        "2. Identify why the button click no longer triggers or hit-tests correctly.\n"
        "3. Propose concrete fixes (files + functions) and any required CSS/DOM changes.\n"
        "4. Implement Wayland CSD button configuration in layout/titlebar.rs, with callbacks and SystemStyle theming.\n"
        "5. Provide a minimal patch plan and list of files to edit.\n"
    )
    return prompt


def build_gemini_request(prompt_text: str, screenshots: List[Tuple[str, str]]) -> Dict[str, Any]:
    parts: List[Dict[str, Any]] = [{"text": prompt_text}]
    for label, b64 in screenshots:
        parts.append({"text": f"\n[Image: {label}]\n"})
        parts.append({
            "inline_data": {
                "mime_type": "image/png",
                "data": b64,
            }
        })
    return {
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 32768,
            "thinkingConfig": {"thinkingLevel": "HIGH"},
        },
    }


def extract_screenshot(debug_info: Dict[str, Any], key: str, label: str) -> Tuple[str, str]:
    try:
        # Try path: key -> data -> value -> data (base64 string)
        b64 = debug_info[key]["data"]["value"]["data"]
        return (label, b64)
    except (KeyError, TypeError):
        pass
    try:
        # Fallback: key -> data -> screenshot
        b64 = debug_info[key]["data"]["screenshot"]
        return (label, b64)
    except (KeyError, TypeError):
        return (label, "")


def main() -> int:
    parser = argparse.ArgumentParser(description="Assemble Gemini prompt from hello-world debug dump")
    parser.add_argument("--dump", default=os.path.join(AZUL_ROOT, "doc", "target", "hello-world-debug", "hello_world_debug_dump.json"),
                        help="Path to the combined JSON dump from collect_hello_world_debug.sh")
    parser.add_argument("--out-dir", default=os.path.join(AZUL_ROOT, "doc", "target", "hello-world-debug"))
    parser.add_argument("--send", action="store_true", help="Actually send to Gemini API")
    args = parser.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)

    # Load the pre-collected debug dump
    if not os.path.exists(args.dump):
        print(f"ERROR: Debug dump not found at {args.dump}", file=sys.stderr)
        print("Run  bash scripts/collect_hello_world_debug.sh  first.", file=sys.stderr)
        return 1

    debug_info = load_debug_dump(args.dump)

    # Collect source files
    source_files = collect_solver3_files() + collect_related_files()
    print(f"Collected {len(source_files)} source files")

    # Collect git diff (excluding api.json)
    diff_text = collect_git_diff()
    print(f"Git diff: {len(diff_text)} chars")

    # Build prompt
    prompt_text = build_prompt(debug_info, source_files, diff_text)
    prompt_path = os.path.join(args.out_dir, "hello_world_gemini_prompt.md")
    with open(prompt_path, "w") as f:
        f.write(prompt_text)
    print(f"Prompt saved to: {prompt_path}  ({len(prompt_text)} chars, ~{len(prompt_text)//4} tokens)")

    # Extract screenshots from dump for Gemini multimodal
    screenshots: List[Tuple[str, str]] = []
    for key, label in [
        ("take_screenshot", "azul-software-screenshot"),
        ("take_native_screenshot", "azul-native-screenshot"),
    ]:
        lbl, b64 = extract_screenshot(debug_info, key, label)
        if b64:
            screenshots.append((lbl, b64))
    print(f"Screenshots: {len(screenshots)}")

    # Build Gemini request JSON
    request_body = build_gemini_request(prompt_text, screenshots)
    request_path = os.path.join(args.out_dir, "hello_world_gemini_request.json")
    with open(request_path, "w") as f:
        json.dump(request_body, f)
    req_size = os.path.getsize(request_path)
    print(f"Request JSON saved to: {request_path}  ({req_size/1024/1024:.1f} MB)")

    if not args.send:
        print("\nDry run complete. Use --send to call Gemini API.")
        return 0

    # ── Send to Gemini ───────────────────────────────────────────────────
    api_key_path = os.path.join(AZUL_ROOT, "GEMINI_API_KEY.txt")
    api_key = read_text(api_key_path).strip()
    if not api_key:
        print("GEMINI_API_KEY.txt is empty", file=sys.stderr)
        return 1

    print(f"\n🤖 Sending to Gemini ({req_size/1024/1024:.1f} MB) ...")
    endpoint = f"{GEMINI_API_URL}?key={api_key}"
    try:
        response = json_post(endpoint, request_body, timeout=600)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        print(f"Gemini HTTP error: {exc.code}\n{body}", file=sys.stderr)
        return 1

    response_path = os.path.join(args.out_dir, "hello_world_gemini_response.json")
    with open(response_path, "w") as f:
        json.dump(response, f, indent=2)

    # Extract text
    text = ""
    for c in response.get("candidates", []):
        for p in c.get("content", {}).get("parts", []):
            if "text" in p:
                text += p["text"]
    response_md_path = os.path.join(args.out_dir, "hello_world_gemini_response.md")
    with open(response_md_path, "w") as f:
        f.write(text)

    print(f"Response saved to: {response_md_path}  ({len(text)} chars)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
