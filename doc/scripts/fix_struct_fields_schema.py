#!/usr/bin/env python3
"""
Fix struct_fields schema in api.json

The old build.py expects struct_fields in this format:
  "struct_fields": [
    { "field1": { "type": "..." } },
    { "field2": { "type": "..." } }
  ]

But some entries may have the wrong format:
  "struct_fields": [
    { "field1": {...}, "field2": {...} }  // all fields in one dict!
  ]

This script converts the wrong format to the correct one.
"""

import json
import sys
from collections import OrderedDict

def fix_struct_fields(api_data):
    """Recursively fix struct_fields in the API data."""
    fixed_count = 0
    
    for version_key, version_data in api_data.items():
        if isinstance(version_data, dict):
            # Check if this is the 'api' key or a module directly
            if 'api' in version_data:
                modules = version_data['api']
            elif 'classes' in version_data:
                modules = {version_key: version_data}
            else:
                modules = version_data
            
            for module_name, module in modules.items():
                if not isinstance(module, dict) or 'classes' not in module:
                    continue
                
                for class_name, class_data in module['classes'].items():
                    if 'struct_fields' not in class_data:
                        continue
                    
                    sf = class_data['struct_fields']
                    if not isinstance(sf, list):
                        print(f"Warning: {class_name}.struct_fields is not a list")
                        continue
                    
                    # Check if we need to fix it
                    needs_fix = False
                    for item in sf:
                        if isinstance(item, dict) and len(item) > 1:
                            needs_fix = True
                            break
                    
                    if needs_fix:
                        # Convert to correct format
                        new_sf = []
                        for item in sf:
                            if isinstance(item, dict):
                                # Split multi-key dict into separate dicts
                                for field_name, field_value in item.items():
                                    new_sf.append({field_name: field_value})
                            else:
                                new_sf.append(item)
                        
                        class_data['struct_fields'] = new_sf
                        fixed_count += 1
                        print(f"Fixed: {class_name} ({len(new_sf)} fields)")
    
    return fixed_count

def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else '/Users/fschutt/Development/azul/api.json'
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        api_data = json.load(f, object_pairs_hook=OrderedDict)
    
    print("Fixing struct_fields schema...")
    fixed_count = fix_struct_fields(api_data)
    
    print(f"\nFixed {fixed_count} struct_fields entries")
    
    if fixed_count > 0:
        print(f"Writing {input_file}...")
        with open(input_file, 'w') as f:
            json.dump(api_data, f, indent=2)
        print("Done!")
    else:
        print("No changes needed.")

if __name__ == '__main__':
    main()
