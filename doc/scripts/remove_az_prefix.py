#!/usr/bin/env python3
"""
Remove Az prefix from type names in api.json.

The Az prefix is added during codegen, so types in api.json should NOT have it.
For example:
- AzString -> String (but we have AzString as the canonical name, which is special)
- AzDebugMessageVec -> DebugMessageVec
- AzAppPtr -> AppPtr

Exception: AzString is kept as-is because it's the canonical FFI type name.
"""

import json
import sys
import re
from collections import OrderedDict

# Types that should keep their Az prefix (special cases)
KEEP_AZ_PREFIX = {
    'AzString',  # AzString is the canonical FFI name (not String)
}

def remove_az_prefix(name):
    """Remove Az prefix from a type name, unless it's a special case."""
    if name in KEEP_AZ_PREFIX:
        return name
    if name.startswith('Az') and len(name) > 2 and name[2].isupper():
        return name[2:]
    return name

def fix_type_reference(type_str):
    """Fix a type reference string by removing Az prefixes."""
    if type_str is None:
        return None
    
    # Match type names (capitalized words)
    def replace_match(m):
        word = m.group(0)
        if word.startswith('Az') and len(word) > 2 and word[2].isupper():
            if word not in KEEP_AZ_PREFIX:
                return word[2:]
        return word
    
    result = re.sub(r'\bAz[A-Z][a-zA-Z0-9]*', replace_match, type_str)
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
    
    # Fix external path
    if 'external' in class_data:
        # Don't touch the external path - it's the Rust path
        pass
    
    return class_data

def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else '/Users/fschutt/Development/azul/api.json'
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        api_data = json.load(f, object_pairs_hook=OrderedDict)
    
    renamed_count = 0
    
    for version_key, version_data in api_data.items():
        if not isinstance(version_data, dict) or 'api' not in version_data:
            continue
        
        api = version_data['api']
        
        # First pass: rename class keys that have Az prefix
        for module_name in list(api.keys()):
            module = api[module_name]
            if 'classes' not in module:
                continue
            
            classes = module['classes']
            # Collect renames
            renames = []
            for class_name in list(classes.keys()):
                new_name = remove_az_prefix(class_name)
                if new_name != class_name:
                    renames.append((class_name, new_name))
            
            # Apply renames (need to be careful with OrderedDict)
            for old_name, new_name in renames:
                if new_name in classes:
                    print(f"WARNING: Cannot rename {module_name}.{old_name} -> {new_name} (already exists)")
                    # Merge or delete duplicate
                    del classes[old_name]
                else:
                    classes[new_name] = classes.pop(old_name)
                    print(f"Renamed: {module_name}.{old_name} -> {new_name}")
                    renamed_count += 1
        
        # Second pass: fix type references in all classes
        for module_name, module in api.items():
            if 'classes' not in module:
                continue
            
            for class_name, class_data in module['classes'].items():
                process_class(class_data)
    
    print(f"\nRenamed {renamed_count} types")
    
    print(f"Writing {input_file}...")
    with open(input_file, 'w') as f:
        json.dump(api_data, f, indent=2)
    print("Done!")

if __name__ == '__main__':
    main()
