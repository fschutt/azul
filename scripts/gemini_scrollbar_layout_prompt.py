#!/usr/bin/env python3
"""
Build a focused Gemini prompt for scrollbar and layout constraint analysis.
Includes specific files for Bugs 4-7 (scrollbar) and Bug 2 (line wrapping).
"""

import os
from datetime import datetime
from pathlib import Path

# Configuration
WORKSPACE_ROOT = Path("/Users/fschutt/Development/azul")
OUTPUT_DIR = WORKSPACE_ROOT / "scripts"

def read_file_safe(path: Path) -> str:
    """Read file with error handling."""
    try:
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            return f.read()
    except Exception as e:
        return f"[ERROR reading file: {e}]"

def add_file(sections: list, path: Path, description: str):
    """Add a file to sections."""
    if not path.exists():
        sections.append(f"\n[FILE NOT FOUND: {path}]\n")
        return
    
    content = read_file_safe(path)
    rel_path = path.relative_to(WORKSPACE_ROOT)
    
    header = f"\n{'='*80}\n"
    header += f"## FILE: {rel_path}\n"
    header += f"## Description: {description}\n"
    header += f"{'='*80}\n"
    
    sections.append(header)
    sections.append(f"```rust\n{content}\n```\n")

def build_prompt():
    sections = []
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    
    sections.append(f"""# Azul Scrollbar & Layout Constraint Analysis
# Generated: {datetime.now().isoformat()}
# Purpose: Debug scrollbar visibility/geometry (Bugs 4-7) and line wrapping (Bug 2)

## BUGS BEING ANALYZED

### Bug 2: Line Wrapping When It Shouldn't
- ContentEditable wraps lines but shouldn't
- Should horizontally overflow and be horizontally scrollable
- Suspected cause: `UnifiedConstraints::default()` sets `available_width` to `Definite(0.0)`

### Bug 4: Scrollbar Visibility (auto mode)
- Scrollbar shows at 100% even though nothing to scroll
- `overflow: auto` should hide scrollbar when content fits
- Also affects size reservation (auto shouldn't reserve space if not scrollable)

### Bug 5: Scrollbar Track Width
- Track width calculated wrong
- Should be "100% - button sizes on each side"

### Bug 6: Scrollbar Position
- Scrollbar in wrong position/size
- Should use "inner" box (directly below text), not outer box

### Bug 7: Border Position
- Light grey 1px border around content is mispositioned
- Border and content with padding should NEVER be visually detached

---

## REQUESTED FILES FOR ANALYSIS

""")

    # File 1: scroll_state.rs
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "managers" / "scroll_state.rs",
        "Scroll state management - visibility logic, overflow: auto handling"
    )

    # File 2: scrollbar.rs in solver3
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "solver3" / "scrollbar.rs",
        "Scrollbar geometry calculations - track and thumb sizing"
    )
    
    # Alternative location if not in solver3
    scrollbar_alt = WORKSPACE_ROOT / "layout" / "src" / "scrollbar.rs"
    if scrollbar_alt.exists():
        add_file(
            sections,
            scrollbar_alt,
            "Scrollbar (alternative location)"
        )

    # File 3: display_list.rs
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "solver3" / "display_list.rs",
        "Display list generation - scrollbar rendering, z-ordering, clipping"
    )
    
    # Alternative location
    display_list_alt = WORKSPACE_ROOT / "layout" / "src" / "display_list.rs"
    if display_list_alt.exists():
        add_file(
            sections,
            display_list_alt,
            "Display list (alternative location)"
        )

    # File 4: fc.rs (Formatting Context)
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "solver3" / "fc.rs",
        "Formatting context - UnifiedConstraints instantiation"
    )
    
    # Also include text3/cache.rs for UnifiedConstraints default
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "text3" / "cache.rs",
        "Text cache - UnifiedConstraints default implementation"
    )

    # Include the main layout lib for context
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "lib.rs",
        "Layout lib root - module structure"
    )

    # Include overflow.rs if it exists
    overflow_rs = WORKSPACE_ROOT / "layout" / "src" / "overflow.rs"
    if overflow_rs.exists():
        add_file(
            sections,
            overflow_rs,
            "Overflow handling"
        )

    # Include block_layout.rs for layout context
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "block_layout.rs",
        "Block layout - constraint propagation"
    )

    # Include inline_layout.rs
    add_file(
        sections,
        WORKSPACE_ROOT / "layout" / "src" / "inline_layout.rs",
        "Inline layout - text layout constraints"
    )

    sections.append("""

---

## ANALYSIS REQUEST

Please analyze the above code and provide:

1. **Root Cause for Bug 2 (Line Wrapping):**
   - Where exactly is `UnifiedConstraints` instantiated with wrong `available_width`?
   - What should the fix be?

2. **Root Cause for Bugs 4-7 (Scrollbar Issues):**
   - Where is the `overflow: auto` visibility logic?
   - Where is the scrollbar geometry calculated?
   - Why is the border visually detached?

3. **Specific Code Fixes** with exact file paths and line numbers

4. **Verification Steps** to test each fix
""")

    # Write output
    output_path = OUTPUT_DIR / f"gemini_scrollbar_prompt_{timestamp}.md"
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(''.join(sections))
    
    # Count lines
    total_lines = sum(s.count('\n') for s in sections)
    file_size = output_path.stat().st_size
    
    print(f"Generated prompt: {output_path}")
    print(f"Total lines: {total_lines}")
    print(f"File size: {file_size / 1024:.2f} KB")
    
    # Also create a latest symlink/copy
    latest_path = OUTPUT_DIR / "gemini_scrollbar_prompt_latest.md"
    with open(latest_path, 'w', encoding='utf-8') as f:
        f.write(''.join(sections))
    print(f"Also saved to: {latest_path}")

if __name__ == "__main__":
    build_prompt()
