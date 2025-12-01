#!/usr/bin/env python3
"""
Fix enum_fields schema in api.json.

The CORRECT schema is:
    "enum_fields": [
        {"Variant1": {}},
        {"Variant2": {"type": "SomeType"}}
    ]

The WRONG schema (created by autofix) is:
    "enum_fields": [
        {"Variant1": {}, "Variant2": {"type": "SomeType"}}
    ]

This script converts the wrong schema back to the correct one,
preserving the order of variants.
"""

import json
import sys
from collections import OrderedDict

def fix_enum_fields(data):
    """Recursively fix enum_fields in the API data."""
    fixed_count = 0
    
    for version_name, version_data in data.items():
        if "api" not in version_data:
            continue
            
        for module_name, module_data in version_data["api"].items():
            if "classes" not in module_data:
                continue
                
            for class_name, class_data in module_data["classes"].items():
                if "enum_fields" not in class_data:
                    continue
                    
                enum_fields = class_data["enum_fields"]
                
                # Check if this needs fixing:
                # Wrong format: single dict with multiple keys
                # Correct format: array of single-key dicts
                if len(enum_fields) == 1 and len(enum_fields[0]) > 1:
                    # This is the wrong format - fix it
                    old_dict = enum_fields[0]
                    new_array = []
                    
                    # Preserve order by iterating through keys
                    for variant_name, variant_data in old_dict.items():
                        new_array.append({variant_name: variant_data})
                    
                    class_data["enum_fields"] = new_array
                    fixed_count += 1
                    print(f"  Fixed: {module_name}.{class_name} ({len(new_array)} variants)")
    
    return fixed_count

def main():
    api_json_path = "api.json"
    
    print(f"Loading {api_json_path}...")
    with open(api_json_path, 'r') as f:
        # Use object_pairs_hook to preserve order
        data = json.load(f, object_pairs_hook=OrderedDict)
    
    print("Fixing enum_fields schema...")
    fixed_count = fix_enum_fields(data)
    
    if fixed_count == 0:
        print("No enum_fields needed fixing.")
        return
    
    print(f"\nFixed {fixed_count} enum_fields entries.")
    
    print(f"Writing corrected {api_json_path}...")
    with open(api_json_path, 'w') as f:
        json.dump(data, f, indent=2)
    
    print("Done!")

if __name__ == "__main__":
    main()
