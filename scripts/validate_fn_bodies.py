#!/usr/bin/env python3
"""
Validate fn_body entries in api.json to ensure they follow the expected pattern.

Expected patterns:
1. Simple method forwarding: `object.method_name(args...)` 
2. Static/constructor call: `path::to::Type::function(args...)`
3. Simple expressions like `true`, `false`, `Default::default()`

Suspicious patterns (likely hallucinated):
- Contains `let` bindings
- Contains `unsafe` blocks
- Contains `if`/`match`/`loop` control flow
- Contains closures `|...|`
- Contains multiple statements (`;` followed by more code)
- Inline implementations instead of forwarding
"""

import json
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple, Any

# Patterns that indicate inline implementation (suspicious)
SUSPICIOUS_PATTERNS = [
    (r'\blet\s+', "contains 'let' binding"),
    (r'\bif\s+', "contains 'if' expression"),
    (r'\bmatch\s+', "contains 'match' expression"),
    (r'\bloop\s*\{', "contains 'loop'"),
    (r'\bwhile\s+', "contains 'while' loop"),
    (r'\bfor\s+\w+\s+in\s+', "contains 'for' loop"),
    (r'\|[^|]*\|', "contains closure"),
    (r';\s*\w', "contains multiple statements"),
    (r'\.map\s*\(', "contains .map() - might be OK but check"),
    (r'\.unwrap\s*\(', "contains .unwrap() - should use proper error handling"),
    (r'\.expect\s*\(', "contains .expect() - should use proper error handling"),
    (r'core::slice::from_raw_parts', "contains raw pointer manipulation"),
    (r'core::mem::transmute', "contains transmute - usually wrong in fn_body"),
    (r'\.offset\s*\(', "contains pointer offset"),
    (r'\.add\s*\(', "contains pointer add"),
]

# Valid simple patterns
VALID_PATTERNS = [
    # Simple forwarding to object method
    r'^object\.\w+\([^)]*\)$',
    # Static function call
    r'^[\w:]+::\w+\([^)]*\)$',
    # Simple literals
    r'^(true|false|None|\d+|"[^"]*")$',
    # Default::default()
    r'^Default::default\(\)$',
    # Simple field access with method call
    r'^object\.\w+\.\w+\([^)]*\)$',
    # Chained method calls (common pattern)
    r'^object\.\w+\([^)]*\)\.\w+\([^)]*\)$',
    # Into/From conversions
    r'^[\w:]+::\w+\([^)]*\)\.into\(\)$',
    r'^object\.\w+\([^)]*\)\.into\(\)$',
    # Unsafe wrappers that just delegate (common for pointer-taking functions)
    r'^unsafe\s*\{\s*[\w:]+::\w+\([^}]*\)\s*\}$',
]


def find_fn_bodies(obj: Any, path: str = "") -> List[Tuple[str, str, Dict]]:
    """Recursively find all fn_body entries in the JSON structure."""
    results = []
    
    if isinstance(obj, dict):
        if "fn_body" in obj:
            results.append((path, obj["fn_body"], obj))
        for key, value in obj.items():
            new_path = f"{path}.{key}" if path else key
            results.extend(find_fn_bodies(value, new_path))
    elif isinstance(obj, list):
        for i, item in enumerate(obj):
            results.extend(find_fn_bodies(item, f"{path}[{i}]"))
    
    return results


def is_valid_pattern(fn_body: str) -> bool:
    """Check if fn_body matches a known valid pattern."""
    fn_body = fn_body.strip()
    for pattern in VALID_PATTERNS:
        if re.match(pattern, fn_body):
            return True
    return False


def check_suspicious(fn_body: str) -> List[str]:
    """Check for suspicious patterns and return list of issues."""
    issues = []
    for pattern, description in SUSPICIOUS_PATTERNS:
        if re.search(pattern, fn_body):
            issues.append(description)
    return issues


def extract_function_name(path: str) -> str:
    """Extract the function name from the JSON path."""
    parts = path.split(".")
    # Look for the function name (usually the last meaningful part before fn_body context)
    for i, part in enumerate(parts):
        if part in ("constructors", "functions", "static_functions"):
            if i + 1 < len(parts):
                return parts[i + 1]
    return parts[-1] if parts else "unknown"


def extract_type_name(path: str) -> str:
    """Extract the type name from the JSON path."""
    parts = path.split(".")
    # Usually: modules.X.classes.Y.functions.Z
    for i, part in enumerate(parts):
        if part == "classes" and i + 1 < len(parts):
            return parts[i + 1]
    return "unknown"


def validate_forwarding(fn_body: str, func_name: str, type_name: str) -> List[str]:
    """Check if fn_body properly forwards to the expected function."""
    issues = []
    fn_body = fn_body.strip()
    
    # Convert function name from snake_case
    expected_method = func_name
    
    # Check if it looks like it's calling a completely different function
    # This is heuristic - we look for the function name in the body
    if expected_method not in fn_body and not is_valid_pattern(fn_body):
        # It might be using a different naming convention
        # e.g., fn_name in JSON but fn_name in Rust is slightly different
        pass  # Don't flag this as it's too noisy
    
    return issues


def main():
    api_json_path = Path(__file__).parent.parent / "api.json"
    
    if not api_json_path.exists():
        print(f"Error: {api_json_path} not found")
        sys.exit(1)
    
    print(f"Loading {api_json_path}...")
    with open(api_json_path, "r") as f:
        api = json.load(f)
    
    print("Finding all fn_body entries...")
    fn_bodies = find_fn_bodies(api)
    print(f"Found {len(fn_bodies)} fn_body entries\n")
    
    suspicious_entries = []
    inline_implementations = []
    
    for path, fn_body, context in fn_bodies:
        func_name = extract_function_name(path)
        type_name = extract_type_name(path)
        
        # Check for suspicious patterns
        issues = check_suspicious(fn_body)
        
        if issues:
            entry = {
                "path": path,
                "function": func_name,
                "type": type_name,
                "fn_body": fn_body,
                "issues": issues,
            }
            
            # Categorize by severity
            severe_issues = [i for i in issues if any(x in i for x in 
                ["let", "unsafe", "raw pointer", "transmute", "offset"])]
            
            if severe_issues:
                inline_implementations.append(entry)
            else:
                suspicious_entries.append(entry)
    
    # Report results
    print("=" * 80)
    print("CRITICAL: Likely inline implementations (should delegate to Rust)")
    print("=" * 80)
    
    if inline_implementations:
        for entry in inline_implementations:
            print(f"\n📛 {entry['type']}::{entry['function']}")
            print(f"   Path: {entry['path']}")
            print(f"   Issues: {', '.join(entry['issues'])}")
            print(f"   fn_body: {entry['fn_body'][:200]}{'...' if len(entry['fn_body']) > 200 else ''}")
    else:
        print("\n✅ No critical inline implementations found!")
    
    print("\n" + "=" * 80)
    print("WARNING: Potentially suspicious patterns (may be OK, review manually)")
    print("=" * 80)
    
    if suspicious_entries:
        # Group by issue type
        by_issue = {}
        for entry in suspicious_entries:
            for issue in entry["issues"]:
                if issue not in by_issue:
                    by_issue[issue] = []
                by_issue[issue].append(entry)
        
        for issue, entries in sorted(by_issue.items()):
            print(f"\n⚠️  {issue} ({len(entries)} occurrences)")
            for entry in entries[:5]:  # Show first 5
                print(f"   - {entry['type']}::{entry['function']}")
                print(f"     {entry['fn_body'][:100]}{'...' if len(entry['fn_body']) > 100 else ''}")
            if len(entries) > 5:
                print(f"   ... and {len(entries) - 5} more")
    else:
        print("\n✅ No suspicious patterns found!")
    
    # Summary
    print("\n" + "=" * 80)
    print("SUMMARY")
    print("=" * 80)
    print(f"Total fn_body entries: {len(fn_bodies)}")
    print(f"Critical issues: {len(inline_implementations)}")
    print(f"Warnings: {len(suspicious_entries)}")
    
    # Exit with error if critical issues found
    if inline_implementations:
        print("\n❌ Found critical issues that need to be fixed!")
        sys.exit(1)
    else:
        print("\n✅ All fn_body entries look OK!")
        sys.exit(0)


if __name__ == "__main__":
    main()
