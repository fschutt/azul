#!/usr/bin/env python3
"""
Fix type naming issues in api.json:

1. Remove duplicate entries where both 'X' and 'AzX' exist (keep AzX)
2. Rename bare 'String' references to 'AzString' 
3. Remove entries that are Rust-internal types not suitable for FFI

The convention is:
- All types in api.json should NOT have the Az prefix (it's added during codegen)
- Exception: AzString is the canonical name (not String) because Rust's String is not FFI-safe
"""

import json
import sys
from collections import OrderedDict

# Types that should be removed because they're Rust-internal or duplicates
TYPES_TO_REMOVE = {
    # Rust String is not FFI-safe, use AzString instead
    'String',
    # Duplicate - AzDebugMessageVecDestructorType exists
    'DebugMessageVecDestructorType',
}

# Types that need to be renamed in references (old -> new)
TYPE_RENAMES = {
    'String': 'AzString',
}

# Additional types that shouldn't exist in api.json (primitive-like)
INVALID_TYPES = {
    'T',  # Generic type parameter
    'U8', 'U16', 'U32', 'U64',  # Should be u8, u16, etc.
    'I8', 'I16', 'I32', 'I64',  # Should be i8, i16, etc.
    'F32', 'F64',  # Should be f32, f64
    'Usize', 'Isize',  # Should be usize, isize
}

def fix_type_reference(type_str):
    """Fix a type reference string."""
    if type_str is None:
        return None
    
    result = type_str
    for old, new in TYPE_RENAMES.items():
        # Replace whole words only
        import re
        result = re.sub(r'\b' + old + r'\b', new, result)
    
    return result

def process_field_data(field_data):
    """Process a field data dict, fixing type references."""
    if isinstance(field_data, dict):
        if 'type' in field_data:
            field_data['type'] = fix_type_reference(field_data['type'])
    return field_data

def process_class(class_data):
    """Process a class, fixing all type references."""
    # Fix struct_fields
    if 'struct_fields' in class_data:
        for field_map in class_data['struct_fields']:
            for field_name, field_data in field_map.items():
                process_field_data(field_data)
    
    # Fix enum_fields
    if 'enum_fields' in class_data:
        for variant_map in class_data['enum_fields']:
            for variant_name, variant_data in variant_map.items():
                if isinstance(variant_data, dict) and 'type' in variant_data:
                    variant_data['type'] = fix_type_reference(variant_data['type'])
    
    # Fix callback_typedef
    if 'callback_typedef' in class_data:
        ct = class_data['callback_typedef']
        if ct is not None and isinstance(ct, dict):
            if 'fn_args' in ct and ct['fn_args'] is not None:
                for arg in ct['fn_args']:
                    if isinstance(arg, dict) and 'type' in arg:
                        arg['type'] = fix_type_reference(arg['type'])
            if 'returns' in ct and ct['returns'] is not None and isinstance(ct['returns'], dict) and 'type' in ct['returns']:
                ct['returns']['type'] = fix_type_reference(ct['returns']['type'])
    
    # Fix constructors
    if 'constructors' in class_data:
        for fn_name, fn_data in class_data['constructors'].items():
            if 'fn_args' in fn_data:
                for arg in fn_data['fn_args']:
                    for arg_name, arg_type in list(arg.items()):
                        arg[arg_name] = fix_type_reference(arg_type)
            if 'returns' in fn_data and 'type' in fn_data['returns']:
                fn_data['returns']['type'] = fix_type_reference(fn_data['returns']['type'])
    
    # Fix functions
    if 'functions' in class_data:
        for fn_name, fn_data in class_data['functions'].items():
            if 'fn_args' in fn_data:
                for arg in fn_data['fn_args']:
                    for arg_name, arg_type in list(arg.items()):
                        if arg_name != 'self':
                            arg[arg_name] = fix_type_reference(arg_type)
            if 'returns' in fn_data and 'type' in fn_data['returns']:
                fn_data['returns']['type'] = fix_type_reference(fn_data['returns']['type'])
    
    return class_data

def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else '/Users/fschutt/Development/azul/api.json'
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        api_data = json.load(f, object_pairs_hook=OrderedDict)
    
    removed_count = 0
    fixed_refs_count = 0
    
    for version_key, version_data in api_data.items():
        if not isinstance(version_data, dict) or 'api' not in version_data:
            continue
        
        api = version_data['api']
        
        # First pass: remove invalid types
        for module_name in list(api.keys()):
            module = api[module_name]
            if 'classes' not in module:
                continue
            
            classes = module['classes']
            for class_name in list(classes.keys()):
                if class_name in TYPES_TO_REMOVE or class_name in INVALID_TYPES:
                    del classes[class_name]
                    print(f"Removed: {module_name}.{class_name}")
                    removed_count += 1
        
        # Second pass: fix type references
        for module_name, module in api.items():
            if 'classes' not in module:
                continue
            
            for class_name, class_data in module['classes'].items():
                process_class(class_data)
    
    print(f"\nRemoved {removed_count} invalid types")
    
    print(f"Writing {input_file}...")
    with open(input_file, 'w') as f:
        json.dump(api_data, f, indent=2)
    print("Done!")

if __name__ == '__main__':
    main()
