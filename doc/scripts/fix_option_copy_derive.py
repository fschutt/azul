#!/usr/bin/env python3
"""
Script to fix Copy derive on Option types where the inner type is not Copy.

Option types should only have derive: ["Copy"] if their inner type is also Copy.
This script removes the Copy derive from Option enums where the inner type
doesn't have Copy derive.
"""

import json

# Primitive types that are always Copy
PRIMITIVE_COPY_TYPES = {
    'bool', 'f32', 'f64', 'i8', 'i16', 'i32', 'i64', 'i128',
    'u8', 'u16', 'u32', 'u64', 'u128', 'usize', 'isize', 'char',
    # Capitalized versions
    'Bool', 'F32', 'F64', 'I8', 'I16', 'I32', 'I64', 'I128',
    'U8', 'U16', 'U32', 'U64', 'U128', 'Usize', 'Isize', 'Char',
}

# Known Copy types
KNOWN_COPY_TYPES = {
    'NodeId', 'DomNodeId', 'ThreadId', 'TimerId', 'ColorU',
    'LayoutRect', 'LayoutPoint', 'LogicalPosition', 'LogicalSize',
    'PhysicalPosition', 'PhysicalSize', 'WindowTheme', 'LayoutZIndex',
    'LayoutFloat', 'LayoutBoxSizing', 'LayoutWidth', 'LayoutHeight',
    'PixelValue', 'AngleValue', 'NormalizedLinearColorStop', 'NormalizedRadialColorStop',
}

def is_type_copy(type_name, classes_by_name):
    """Check if a type has Copy derive."""
    if type_name in PRIMITIVE_COPY_TYPES:
        return True
    if type_name in KNOWN_COPY_TYPES:
        return True
    
    # Look up in the classes
    if type_name in classes_by_name:
        class_data = classes_by_name[type_name]
        derive = class_data.get('derive', [])
        if derive is None:
            derive = []
        return 'Copy' in derive
    
    return False

def main():
    with open('api.json', 'r') as f:
        data = json.load(f)
    
    fixed_count = 0
    
    for version_name, version_data in data.items():
        if 'api' not in version_data:
            continue
        
        # First, build a map of all class names to their data
        classes_by_name = {}
        for module_name, module_data in version_data.get('api', {}).items():
            for class_name, class_data in module_data.get('classes', {}).items():
                classes_by_name[class_name] = class_data
        
        # Now check all Option types
        for module_name, module_data in version_data.get('api', {}).items():
            for class_name, class_data in module_data.get('classes', {}).items():
                # Check if it's an Option type with Copy derive
                derive = class_data.get('derive', [])
                if derive is None:
                    derive = []
                
                if 'Copy' not in derive:
                    continue
                
                # Check if it has enum_fields with None/Some pattern
                enum_fields = class_data.get('enum_fields', [])
                if not enum_fields:
                    continue
                
                # Find the Some variant and get its inner type
                inner_type = None
                for variant in enum_fields:
                    if not isinstance(variant, dict):
                        continue
                    for variant_name, variant_data in variant.items():
                        if variant_name == 'Some' and isinstance(variant_data, dict):
                            inner_type = variant_data.get('type')
                            break
                
                if not inner_type:
                    continue
                
                # Check if inner type is Copy
                if not is_type_copy(inner_type, classes_by_name):
                    # Remove Copy from derive
                    new_derive = [d for d in derive if d != 'Copy']
                    if new_derive:
                        class_data['derive'] = new_derive
                    else:
                        # Remove derive key entirely if empty
                        del class_data['derive']
                    
                    print(f"Fixed {module_name}.{class_name}: removed Copy (inner type {inner_type} is not Copy)")
                    fixed_count += 1
    
    if fixed_count > 0:
        with open('api.json', 'w') as f:
            json.dump(data, f, indent=2)
        print(f"\nFixed {fixed_count} Option types with incorrect Copy derive")
    else:
        print("No Option types needed fixing")

if __name__ == '__main__':
    main()
