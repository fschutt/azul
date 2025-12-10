import os
import subprocess
import google.generativeai as genai
from PIL import Image

# 1. SETUP
# Replace with your actual key, or set export GEMINI_API_KEY='...' in your shell
API_KEY = os.getenv("GEMINI_API_KEY") or "YOUR_ACTUAL_API_KEY_HERE"
genai.configure(api_key=API_KEY)

# Use Flash for speed/cost, or 'gemini-1.5-pro' for higher reasoning
model = genai.GenerativeModel('gemini-1.5-flash')

def verify_screen():
    print("📸 Taking screenshot...")
    # Capture screen to a temp file (macOS specific)
    screenshot_path = "temp_verification_screen.png"
    subprocess.run(["screencapture", "-x", screenshot_path])

    try:
        print("🤖 Sending to Gemini...")
        img = Image.open(screenshot_path)
        
        # The Prompt
        prompt = (
            "Analyze this screenshot. I am debugging a GUI application I am building. "
            "Look for a custom window that contains actual UI elements (buttons, text, content) "
            "and is NOT just a blank, black, or white empty rectangle.\n\n"
            "1. If you see a window with content, start your response with 'SUCCESS'.\n"
            "2. If the screen is empty, or the window is blank/broken, start with 'FAILURE'.\n"
            "3. After the status, provide a brief description of the windows you see on screen."
        )

        response = model.generate_content([prompt, img])
        
        print("-" * 30)
        print(response.text)
        print("-" * 30)

    except Exception as e:
        print(f"Error: {e}")
    finally:
        # Cleanup
        if os.path.exists(screenshot_path):
            os.remove(screenshot_path)

if __name__ == "__main__":
    verify_screen()