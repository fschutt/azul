#!/usr/bin/env python3
"""
Remove duplicated enum_fields/struct_fields from type_alias entries.

When a type has both:
- type_alias (pointing to a generic type like CssPropertyValue<T>)
- enum_fields or struct_fields (duplicating the structure)

We remove the enum_fields/struct_fields, keeping only the type_alias.
The generator should be smart enough to look up the structure from the target type.
"""

import json
import sys
from collections import OrderedDict

def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else '/Users/fschutt/Development/azul/api.json'
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        api_data = json.load(f, object_pairs_hook=OrderedDict)
    
    removed_count = 0
    
    for version_key, version_data in api_data.items():
        if not isinstance(version_data, dict) or 'api' not in version_data:
            continue
        
        api = version_data['api']
        
        for module_name, module in api.items():
            if 'classes' not in module:
                continue
            
            for class_name, class_data in module['classes'].items():
                # Only process entries with type_alias
                if 'type_alias' not in class_data:
                    continue
                
                # Remove duplicated fields
                removed_fields = []
                if 'enum_fields' in class_data:
                    del class_data['enum_fields']
                    removed_fields.append('enum_fields')
                if 'struct_fields' in class_data:
                    del class_data['struct_fields']
                    removed_fields.append('struct_fields')
                
                if removed_fields:
                    print(f"  {module_name}.{class_name}: removed {', '.join(removed_fields)}")
                    removed_count += 1
    
    print(f"\nRemoved duplicated fields from {removed_count} type_alias entries")
    
    print(f"Writing {input_file}...")
    with open(input_file, 'w') as f:
        json.dump(api_data, f, indent=2)
    print("Done!")

if __name__ == '__main__':
    main()
