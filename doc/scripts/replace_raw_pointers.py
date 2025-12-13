#!/usr/bin/env python3
"""
Replace all raw pointer types (*const Foo, *mut Foo) with *const c_void or *mut c_void
in api.json. Only c_void can take raw pointers in the C ABI.
"""

import json
import sys
import re

def replace_raw_pointer_types(obj, path=""):
    """Recursively replace all raw pointer types with c_void"""
    if isinstance(obj, dict):
        for key, value in obj.items():
            if key == "type" and isinstance(value, str):
                # Match *const SomeType or *mut SomeType (but not *const c_void or *mut c_void)
                if value.startswith("*const ") and not value.startswith("*const c_void"):
                    old_value = value
                    obj[key] = "*const c_void"
                    print(f"  {path}.{key}: {old_value} → *const c_void")
                elif value.startswith("*mut ") and not value.startswith("*mut c_void"):
                    old_value = value
                    obj[key] = "*mut c_void"
                    print(f"  {path}.{key}: {old_value} → *mut c_void")
            else:
                replace_raw_pointer_types(value, f"{path}.{key}")
    elif isinstance(obj, list):
        for i, item in enumerate(obj):
            replace_raw_pointer_types(item, f"{path}[{i}]")

def main():
    api_file = "api.json"
    
    print(f"📖 Reading {api_file}...")
    with open(api_file, 'r', encoding='utf-8') as f:
        api_data = json.load(f)
    
    print(f"\n[ INFO ] Replacing raw pointer types...")
    replace_raw_pointer_types(api_data)
    
    print(f"\n[ INFO ] Saving {api_file}...")
    with open(api_file, 'w', encoding='utf-8') as f:
        json.dump(api_data, f, indent=2, ensure_ascii=False)
    
    print(f"\n[ OK ] Done!")

if __name__ == "__main__":
    main()
