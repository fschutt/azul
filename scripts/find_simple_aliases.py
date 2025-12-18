#!/usr/bin/env python3
"""Find all simple type aliases in api.json that should be converted to newtype structs."""

import json
import sys
import os

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    api_path = os.path.join(project_root, 'api.json')
    
    with open(api_path) as f:
        data = json.load(f)

    simple_aliases = []
    primitives = ['usize', 'isize', 'u8', 'u16', 'u32', 'u64', 'i8', 'i16', 'i32', 'i64', 'f32', 'f64', 'bool', 'c_void']
    
    for version, vdata in data.items():
        if not isinstance(vdata, dict):
            continue
        if 'api' not in vdata:
            continue
        api_data = vdata['api']
        for mod, mdata in api_data.items():
            if not isinstance(mdata, dict):
                continue
            if 'classes' not in mdata:
                continue
            for cname, cdata in mdata['classes'].items():
                if 'type_alias' not in cdata:
                    continue
                ta = cdata['type_alias']
                target = ta.get('type', ta.get('target', ''))
                generic_args = ta.get('generic_args', [])
                ref_kind = ta.get('ref_kind', '')
                # Simple alias = no generic args, no ref_kind (not a pointer), target is not a primitive
                if not generic_args and not ref_kind and '<' not in target and target not in primitives:
                    simple_aliases.append((mod, cname, target))
                    print(f"{mod}/{cname} = {target}")
    
    print(f"\nTotal: {len(simple_aliases)} simple type aliases found")
    if simple_aliases:
        print("\nThese should be converted to newtype structs!")
        print("See: scripts/convert_type_aliases.py for automation")
    return simple_aliases

if __name__ == '__main__':
    main()
