#!/usr/bin/env python3
"""
Fix corrupt type_alias entries in api.json for callback types.

When a class has a valid callback_typedef, it should NOT have a type_alias.
The corrupt type_alias entries look like:
  "type_alias": { "target": "c_void) -> OptionThreadReceiveMsg", ... }
instead of the full extern "C" fn(...) signature.

This script removes type_alias from any class that has a callback_typedef.
"""

import json
import sys
from pathlib import Path


def fix_callback_type_aliases(api_path: Path) -> int:
    """Remove corrupt type_alias entries from classes with callback_typedef."""
    
    with open(api_path, 'r') as f:
        api_data = json.load(f)
    
    fixes_made = 0
    
    for version_name, version_data in api_data.items():
        if 'api' not in version_data:
            continue
            
        for module_name, module_data in version_data['api'].items():
            if 'classes' not in module_data:
                continue
                
            for class_name, class_data in module_data['classes'].items():
                # If class has callback_typedef, remove type_alias
                if 'callback_typedef' in class_data and 'type_alias' in class_data:
                    del class_data['type_alias']
                    print(f"  Fixed: {module_name}.{class_name} - removed corrupt type_alias")
                    fixes_made += 1
                
                # Also check for obviously corrupt type_alias (missing "extern")
                if 'type_alias' in class_data:
                    target = class_data['type_alias'].get('target', '')
                    # Corrupt signatures start with something like "c_void)" or "c_void ,"
                    if target.startswith('c_void)') or target.startswith('c_void ,') or \
                       target.startswith('c_void,') or ') ->' in target[:20]:
                        del class_data['type_alias']
                        print(f"  Fixed: {module_name}.{class_name} - removed corrupt type_alias: {target[:50]}...")
                        fixes_made += 1
    
    if fixes_made > 0:
        with open(api_path, 'w') as f:
            json.dump(api_data, f, indent=2)
        print(f"\n[OK] Fixed {fixes_made} corrupt type_alias entries")
    else:
        print("[OK] No corrupt type_alias entries found")
    
    return fixes_made


def main():
    # Find api.json relative to script location
    script_dir = Path(__file__).parent
    api_path = script_dir.parent.parent / 'api.json'
    
    if not api_path.exists():
        print(f"Error: api.json not found at {api_path}")
        sys.exit(1)
    
    print(f"[FIX] Scanning {api_path} for corrupt type_alias entries...")
    fixes = fix_callback_type_aliases(api_path)
    sys.exit(0 if fixes >= 0 else 1)


if __name__ == '__main__':
    main()
