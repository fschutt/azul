#!/usr/bin/env python3
"""
Send the cursor blinking prompt to Gemini 2.5 Pro and save the response.
Uses REST API with requests library.
"""

import json
import requests
from pathlib import Path

AZUL_ROOT = Path(__file__).parent.parent

def main():
    # Read API key
    api_key_path = AZUL_ROOT / "GEMINI_API_KEY.txt"
    api_key = api_key_path.read_text().strip()
    
    # Read prompt
    prompt_path = AZUL_ROOT / "scripts" / "gemini_cursor_blinking_prompt_v2.md"
    prompt = prompt_path.read_text(encoding='utf-8')
    
    print(f"Prompt size: {len(prompt)} chars (~{len(prompt)//4} tokens)")
    print("Sending to Gemini 2.5 Pro (this may take a few minutes)...")
    
    # Use REST API
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key={api_key}"
    
    payload = {
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.3,
            "maxOutputTokens": 32000,
        }
    }
    
    response = requests.post(
        url,
        json=payload,
        headers={"Content-Type": "application/json"},
        timeout=600  # 10 minutes timeout for large prompts
    )
    
    if response.status_code != 200:
        print(f"ERROR: API returned status {response.status_code}")
        print(response.text)
        return
    
    result = response.json()
    
    # Extract response text
    try:
        response_text = result["candidates"][0]["content"]["parts"][0]["text"]
    except (KeyError, IndexError) as e:
        print(f"ERROR parsing response: {e}")
        print(json.dumps(result, indent=2)[:2000])
        return
    
    # Save response
    output_path = AZUL_ROOT / "scripts" / "gemini_cursor_blinking_response_v2.md"
    output_path.write_text(response_text, encoding='utf-8')
    
    print(f"\nResponse saved to: {output_path}")
    print(f"Response length: {len(response_text)} chars")
    print("\n" + "="*60)
    print("RESPONSE PREVIEW (first 3000 chars):")
    print("="*60)
    print(response_text[:3000])
    if len(response_text) > 3000:
        print(f"\n... [{len(response_text) - 3000} more chars]")

if __name__ == "__main__":
    main()
