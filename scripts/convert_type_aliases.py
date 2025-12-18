#!/usr/bin/env python3
"""Convert simple type aliases to newtype structs in api.json.

Usage:
    python3 scripts/convert_type_aliases.py

This script finds simple type aliases (like `type XmlTagName = String`)
and converts them to proper newtype structs with an `inner` field.

Before running this script, you must also update the Rust source code
to use the newtype struct pattern. See the error messages from
`cargo run --release -- codegen all` for exact instructions.
"""

import json
import sys
import os

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    api_path = os.path.join(project_root, 'api.json')
    
    with open(api_path) as f:
        data = json.load(f)

    primitives = ['usize', 'isize', 'u8', 'u16', 'u32', 'u64', 'i8', 'i16', 'i32', 'i64', 'f32', 'f64', 'bool', 'c_void']
    converted = []
    
    for version, vdata in data.items():
        if not isinstance(vdata, dict) or 'api' not in vdata:
            continue
        api_data = vdata['api']
        for mod, mdata in api_data.items():
            if not isinstance(mdata, dict) or 'classes' not in mdata:
                continue
            for cname, cdata in list(mdata['classes'].items()):
                if 'type_alias' not in cdata:
                    continue
                ta = cdata['type_alias']
                target = ta.get('type', ta.get('target', ''))
                generic_args = ta.get('generic_args', [])
                ref_kind = ta.get('ref_kind', '')
                
                # Only convert simple aliases (no generics, no pointers, not primitives)
                if not generic_args and not ref_kind and '<' not in target and target not in primitives:
                    external = cdata.get('external', '')
                    doc = cdata.get('doc', [f'Wrapper around {target}'])
                    
                    # Replace type_alias with struct_fields
                    new_cdata = {
                        'doc': doc,
                        'external': external,
                        'derive': ['Clone', 'Debug', 'Default', 'Eq', 'Hash', 'Ord', 'PartialEq', 'PartialOrd'],
                        'custom_impls': ['Deref', 'From'],
                        'struct_fields': [
                            {
                                'inner': {
                                    'type': target
                                }
                            }
                        ],
                        'repr': 'C'
                    }
                    mdata['classes'][cname] = new_cdata
                    converted.append((mod, cname, target))
                    print(f"Converted {cname} from type_alias to struct")

    if converted:
        with open(api_path, 'w') as f:
            json.dump(data, f, indent=2)
        print(f"\nDone! Converted {len(converted)} type aliases.")
        print("api.json updated.")
        print("\n*** IMPORTANT ***")
        print("You must also update the Rust source code!")
        print("For each converted type, add the newtype struct pattern:")
        for mod, cname, target in converted:
            print(f"\n  // For {cname}:")
            print(f"  #[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]")
            print(f"  #[repr(C)]")
            print(f"  pub struct {cname} {{")
            print(f"      pub inner: {target},")
            print(f"  }}")
            print(f"  impl From<{target}> for {cname} {{ ... }}")
            print(f"  impl core::ops::Deref for {cname} {{ ... }}")
    else:
        print("No simple type aliases found to convert.")

if __name__ == '__main__':
    main()
