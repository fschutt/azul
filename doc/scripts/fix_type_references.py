#!/usr/bin/env python3
"""
Fix type references in api.json:

1. Replace generic type parameter 'T' with actual types (based on context)
2. Replace primitive-like types (ColorU, F32, U32, etc.) with correct names
3. Ensure all type references use either:
   - Primitive types (bool, f32, u32, etc.) without prefix
   - Or FFI types (AzColorU, AzNodeId, etc.) with correct names
"""

import json
import sys
import re
from collections import OrderedDict

# Type corrections: old_name -> new_name
# NOTE: In api.json, types should NOT have Az prefix (it's added by codegen)
# So we only fix primitives and capitalization issues
TYPE_CORRECTIONS = {
    # Primitive types - should be lowercase
    'F32': 'f32',
    'F64': 'f64',
    'U8': 'u8',
    'U16': 'u16',
    'U32': 'u32',
    'U64': 'u64',
    'I8': 'i8',
    'I16': 'i16',
    'I32': 'i32',
    'I64': 'i64',
    'Usize': 'usize',
    'Isize': 'isize',
    'Bool': 'bool',
    # Fix Option naming
    'Optionusize': 'OptionUsize',
    'Optionu32': 'OptionU32',
    'Optioni32': 'OptionI32',
    # NOTE: Don't add Az prefix - codegen does that
    # ColorU -> ColorU (keep as is)
    # NodeId -> NodeId (keep as is)
}

# Context-specific T replacements
# For PhysicalPosition<T> and PhysicalSize<T>, we need to determine what T should be
# Looking at the Rust source, PhysicalPositionI32 and PhysicalSizeU32 exist
T_REPLACEMENTS = {
    'PhysicalPosition': 'i32',  # PhysicalPositionI32 uses i32
    'PhysicalSize': 'u32',      # PhysicalSizeU32 uses u32
}

def fix_type_reference(type_str, context_class=None):
    """Fix a type reference string."""
    if type_str is None:
        return None
    
    # Handle generic T
    if type_str == 'T':
        if context_class and context_class in T_REPLACEMENTS:
            return T_REPLACEMENTS[context_class]
        # Default fallback - should not happen in well-formed data
        print(f"WARNING: Found bare 'T' type without context, leaving as-is")
        return type_str
    
    # Apply corrections
    if type_str in TYPE_CORRECTIONS:
        return TYPE_CORRECTIONS[type_str]
    
    return type_str

def process_field_data(field_data, context_class=None):
    """Process a field data dict, fixing type references."""
    if isinstance(field_data, dict):
        if 'type' in field_data:
            field_data['type'] = fix_type_reference(field_data['type'], context_class)
    return field_data

def process_class(class_name, class_data):
    """Process a class, fixing all type references."""
    # Fix struct_fields
    if 'struct_fields' in class_data:
        for field_map in class_data['struct_fields']:
            for field_name, field_data in field_map.items():
                process_field_data(field_data, context_class=class_name)
    
    # Fix enum_fields
    if 'enum_fields' in class_data:
        for variant_map in class_data['enum_fields']:
            for variant_name, variant_data in variant_map.items():
                if isinstance(variant_data, dict) and 'type' in variant_data:
                    variant_data['type'] = fix_type_reference(variant_data['type'], context_class=class_name)
    
    # Fix callback_typedef
    if 'callback_typedef' in class_data:
        ct = class_data['callback_typedef']
        if ct is not None and isinstance(ct, dict):
            if 'fn_args' in ct and ct['fn_args'] is not None:
                for arg in ct['fn_args']:
                    if isinstance(arg, dict) and 'type' in arg:
                        arg['type'] = fix_type_reference(arg['type'], context_class=class_name)
            if 'returns' in ct and ct['returns'] is not None and isinstance(ct['returns'], dict) and 'type' in ct['returns']:
                ct['returns']['type'] = fix_type_reference(ct['returns']['type'], context_class=class_name)
    
    # Fix constructors
    if 'constructors' in class_data:
        for fn_name, fn_data in class_data['constructors'].items():
            if 'fn_args' in fn_data:
                for arg in fn_data['fn_args']:
                    for arg_name, arg_type in list(arg.items()):
                        arg[arg_name] = fix_type_reference(arg_type, context_class=class_name)
            if 'returns' in fn_data and 'type' in fn_data['returns']:
                fn_data['returns']['type'] = fix_type_reference(fn_data['returns']['type'], context_class=class_name)
    
    # Fix functions
    if 'functions' in class_data:
        for fn_name, fn_data in class_data['functions'].items():
            if 'fn_args' in fn_data:
                for arg in fn_data['fn_args']:
                    for arg_name, arg_type in list(arg.items()):
                        if arg_name != 'self':
                            arg[arg_name] = fix_type_reference(arg_type, context_class=class_name)
            if 'returns' in fn_data and 'type' in fn_data['returns']:
                fn_data['returns']['type'] = fix_type_reference(fn_data['returns']['type'], context_class=class_name)
    
    return class_data

def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else '/Users/fschutt/Development/azul/api.json'
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        api_data = json.load(f, object_pairs_hook=OrderedDict)
    
    fixed_count = 0
    
    for version_key, version_data in api_data.items():
        if not isinstance(version_data, dict) or 'api' not in version_data:
            continue
        
        api = version_data['api']
        
        for module_name, module in api.items():
            if 'classes' not in module:
                continue
            
            for class_name, class_data in module['classes'].items():
                process_class(class_name, class_data)
    
    print(f"Writing {input_file}...")
    with open(input_file, 'w') as f:
        json.dump(api_data, f, indent=2)
    print("Done!")

if __name__ == '__main__':
    main()
