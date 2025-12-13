#!/usr/bin/env python3
"""
Simplify CSS in XHTML files by removing complex rendering features.
Removes: border-radius, gradients, rgba() with alpha, box-shadow, gap property
Replaces: gap with margin, rgba() with solid colors
"""

import re
import os
import glob

def simplify_css(content):
    """Remove or simplify complex CSS properties"""
    
    # Remove border-radius
    content = re.sub(r'\s*border-radius:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    # Remove box-shadow
    content = re.sub(r'\s*box-shadow:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    # Remove gradients (linear-gradient, radial-gradient)
    def replace_gradient(match):
        # Extract first color from gradient
        colors = re.findall(r'#[0-9a-fA-F]{3,6}', match.group(0))
        if colors:
            return f'background: {colors[0]};'
        return 'background: #808080;'  # fallback gray
    
    content = re.sub(
        r'background:\s*(?:linear|radial)-gradient\([^)]+\)[^;]*;',
        replace_gradient,
        content,
        flags=re.MULTILINE
    )
    
    # Replace rgba() with solid colors
    def replace_rgba(match):
        rgba_str = match.group(0)
        # Extract rgb values
        rgb_match = re.search(r'rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*[\d.]+)?\)', rgba_str)
        if rgb_match:
            r, g, b = rgb_match.groups()
            # Convert to hex
            hex_color = '#{:02x}{:02x}{:02x}'.format(int(r), int(g), int(b))
            return rgba_str.replace(rgb_match.group(0), hex_color)
        return rgba_str
    
    content = re.sub(
        r'(?:background|color|border-color):\s*rgba?\([^)]+\)[^;]*;',
        replace_rgba,
        content,
        flags=re.MULTILINE
    )
    
    # Replace gap with margin
    # gap: 10px; -> margin-right: 10px; margin-bottom: 10px;
    def replace_gap(match):
        gap_value = re.search(r'gap:\s*([^;]+);', match.group(0))
        if gap_value:
            val = gap_value.group(1).strip()
            return f'/* gap: {val}; */ margin-right: {val}; margin-bottom: {val};'
        return match.group(0)
    
    content = re.sub(
        r'\s*gap:\s*[^;]+;',
        replace_gap,
        content,
        flags=re.MULTILINE
    )
    
    # Remove box-sizing if present
    content = re.sub(r'\s*box-sizing:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    # Remove opacity if present
    content = re.sub(r'\s*opacity:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    # Remove transform if present
    content = re.sub(r'\s*transform:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    # Remove transition if present
    content = re.sub(r'\s*transition:\s*[^;]+;', '', content, flags=re.MULTILINE)
    
    return content

def process_file(filepath):
    """Process a single XHTML file"""
    print(f"Processing {os.path.basename(filepath)}...")
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    original_length = len(content)
    simplified = simplify_css(content)
    new_length = len(simplified)
    
    if original_length != new_length:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(simplified)
        print(f"  ✓ Simplified ({original_length} → {new_length} bytes)")
    else:
        print(f"  - No changes needed")

def main():
    # Get all .xht files in current directory
    files = glob.glob('*.xht')
    
    if not files:
        print("No .xht files found in current directory")
        return
    
    print(f"Found {len(files)} XHTML files to process\n")
    
    for filepath in sorted(files):
        process_file(filepath)
    
    print(f"\n[ OK ] Processed {len(files)} files")

if __name__ == '__main__':
    main()
