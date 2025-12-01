#!/usr/bin/env python3
"""
Script to remove primitive types from api.json.

Primitive types like f32, isize, u8, etc. should not have entries in api.json
as they are Rust built-in types and don't need the Az prefix.
"""

import json

PRIMITIVE_TYPES = {
    'bool', 'f32', 'f64', 'i128', 'i16', 'i32', 'i64', 'i8', 'isize',
    'u128', 'u16', 'u32', 'u64', 'u8', 'usize', 'c_void', 'c_char',
    'c_schar', 'c_uchar', 'char', 'str'
}

def main():
    with open('api.json', 'r') as f:
        data = json.load(f)
    
    removed_count = 0
    
    for version_name, version_data in data.items():
        if 'api' not in version_data:
            continue
        
        for module_name, module_data in version_data.get('api', {}).items():
            if 'classes' not in module_data:
                continue
            
            classes = module_data['classes']
            classes_to_remove = []
            
            for class_name in classes:
                if class_name in PRIMITIVE_TYPES:
                    classes_to_remove.append(class_name)
            
            for class_name in classes_to_remove:
                del classes[class_name]
                removed_count += 1
                print(f"Removed primitive type: {module_name}.{class_name}")
    
    if removed_count > 0:
        with open('api.json', 'w') as f:
            json.dump(data, f, indent=2)
        print(f"\nRemoved {removed_count} primitive type entries from api.json")
    else:
        print("No primitive type entries found to remove")

if __name__ == '__main__':
    main()
