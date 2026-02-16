#!/usr/bin/env python3
"""
Send the performance optimization prompt to Gemini API.

Usage:
    python3 scripts/send_perf_to_gemini.py

Reads: scripts/gemini_perf_prompt.md
Writes: scripts/gemini_perf_response.md
"""

import json
import os
import sys
import urllib.request
import urllib.error

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def main():
    # Read API key
    key_path = os.path.join(BASE, "GEMINI_API_KEY.txt")
    if not os.path.exists(key_path):
        print("ERROR: GEMINI_API_KEY.txt not found", file=sys.stderr)
        sys.exit(1)
    
    with open(key_path, 'r') as f:
        api_key = f.read().strip()
    
    # Read prompt
    prompt_path = os.path.join(BASE, "scripts", "gemini_perf_prompt2.md")
    if not os.path.exists(prompt_path):
        print("ERROR: gemini_perf_prompt.md not found", file=sys.stderr)
        sys.exit(1)
    
    with open(prompt_path, 'r') as f:
        prompt_text = f.read()
    
    prompt_lines = prompt_text.count('\n')
    prompt_chars = len(prompt_text)
    print(f"Prompt: {prompt_lines:,} lines, {prompt_chars:,} chars (~{prompt_chars // 4:,} tokens)")
    
    # Use Gemini 3 Pro Preview with thinking
    model = "gemini-3-pro-preview"
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
    
    payload = {
        "contents": [{
            "role": "user",
            "parts": [{
                "text": prompt_text
            }]
        }],
        "generationConfig": {
            "thinkingConfig": {
                "thinkingLevel": "HIGH"
            }
        }
    }
    
    data = json.dumps(payload).encode('utf-8')
    
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
        },
        method="POST"
    )
    
    print(f"Sending to {model}...")
    print(f"This may take 2-5 minutes for a 100K line prompt...")
    
    try:
        with urllib.request.urlopen(req, timeout=600) as resp:
            response_data = json.loads(resp.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        error_body = e.read().decode('utf-8')
        print(f"HTTP Error {e.code}: {error_body[:2000]}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
    
    # Extract response text
    try:
        candidates = response_data.get("candidates", [])
        if not candidates:
            print("ERROR: No candidates in response", file=sys.stderr)
            print(json.dumps(response_data, indent=2)[:2000], file=sys.stderr)
            sys.exit(1)
        
        parts = candidates[0].get("content", {}).get("parts", [])
        response_text = "\n".join(p.get("text", "") for p in parts)
        
        # Check for finish reason
        finish_reason = candidates[0].get("finishReason", "unknown")
        print(f"Finish reason: {finish_reason}")
        
    except (KeyError, IndexError) as e:
        print(f"ERROR: Unexpected response format: {e}", file=sys.stderr)
        print(json.dumps(response_data, indent=2)[:2000], file=sys.stderr)
        sys.exit(1)
    
    # Write response
    output_path = os.path.join(BASE, "scripts", "gemini_perf_response2.md")
    with open(output_path, 'w') as f:
        f.write(response_text)
    
    response_lines = response_text.count('\n')
    print(f"\nResponse: {response_lines:,} lines")
    print(f"Written to: {output_path}")
    
    # Print usage metadata if available
    usage = response_data.get("usageMetadata", {})
    if usage:
        print(f"\nToken usage:")
        print(f"  Prompt tokens: {usage.get('promptTokenCount', 'N/A'):,}")
        print(f"  Response tokens: {usage.get('candidatesTokenCount', 'N/A'):,}")
        print(f"  Total tokens: {usage.get('totalTokenCount', 'N/A'):,}")

if __name__ == "__main__":
    main()
