use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
This generator produces a single `azul.lua` file.

### The Strategy

1.  **`ffi.cdef[[ ... ]]`**: We generate a massive string containing the C-compatible struct/function definitions (reusing logic similar to the C generator).
2.  **`ffi.metatype`**: We define Lua metatables for the C structs to give them Object-Oriented methods (e.g., `window:setTitle("...")`).
3.  **`ffi.gc`**: We automatically attach the C `_delete` function to the Lua garbage collector for objects created via constructors.

*/

/*

### Usage in Lua

1.  Place `azul.lua` in your project.
2.  Ensure `libazul.so` (or dll/dylib) is in the library path (or current directory).

### Key LuaJIT Specifics

1.  **`ffi.metatype`**: This is the magic that turns C structs into Lua objects.
    *   Example: `local win = AzWindow_new()`. `win` is a `cdata<struct AzWindow *>`.
    *   Calling `win:setTitle("Foo")` works because we associated a metatable index with the struct type `AzWindow`.
2.  **Strings**: Lua strings are immutable. Passing a Lua string to a `const char*` argument in C works automatically.
    *   *Warning*: If the C code stores that pointer for later use (asynchronous), you must copy it in C, because the Lua string might be garbage collected.
3.  **Structs by Value**: LuaJIT FFI supports passing structs by value perfectly. If `AzColor` is a struct, `func(color)` works if `color` is a `cdata<AzColor>`.

*/

/*

local azul = require("azul")

-- Create Config
-- Note: Lua numbers are doubles, but LuaJIT converts to int/float for FFI
local config = azul.AzAppConfig_new()

-- Create Window Options
local opts = azul.AzWindowCreateOptions_new(nil)
-- We can set fields directly on structs because of the C definition
-- opts.title = "Hello Lua" -- (If AzString was mapped to char* correctly in struct def)

-- Create App
local app = azul.AzApp_new(nil, config)

-- Run
azul.AzApp_run(app, opts)

-- When 'app' goes out of scope, Lua's GC triggers __gc, 
-- which calls AzApp_delete via ffi.gc

*/

// Lua has no strict packaging standard like Cargo or Maven - optional rockspec
pub fn get_lua_rocks_spec() -> String {
    format!("
package = \"azul\"
version = \"1.0-1\"
source = { url = \"...\" }
build = {
    type = \"builtin\",
    modules = {
        azul = \"azul.lua\"
    },
    copy_directories = { \"lib\" } -- Copy shared libraries
}
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

pub fn generate_lua_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header & Requirements
    code.push_str("local ffi = require(\"ffi\")\n");
    code.push_str("-- Try to load the library (platform dependent names handled mostly by ffi.load)\n");
    code.push_str(&format!("local lib = ffi.load(\"{}\")\n\n", LIB_NAME));
    
    code.push_str("local M = {}\n\n");

    // -------------------------------------------------------------------------
    // 2. C Declarations (ffi.cdef)
    // -------------------------------------------------------------------------
    code.push_str("ffi.cdef[[\n");
    
    // Primitives
    code.push_str("  typedef uint8_t AzGLboolean;\n");
    code.push_str("  typedef uint32_t AzScanCode;\n");
    code.push_str("  typedef void* AzHwndHandle;\n"); // Generic pointers
    
    // Forward Declarations
    for (_, module) in &version_data.api {
        for (class_name, _) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            code.push_str(&format!("  typedef struct {} {};\n", full_name, full_name));
        }
    }
    code.push_str("\n");

    // Enums & Structs Definitions (C Syntax)
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            // Enums
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("  enum {} {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    {}_{},\n", full_name, variant_name));
                        }
                    }
                    code.push_str("  };\n");
                }
            }

            // Structs
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("  struct {} {{\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        // Use simplified C types for LuaJIT
                        let c_type = map_to_c_def_type(&field_data.r#type);
                        code.push_str(&format!("    {} {};\n", c_type, field_name));
                    }
                }
                code.push_str("  };\n");
            }
        }
    }

    // Functions
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helper to write C signatures
            let mut write_sig = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let ret_type = if let Some(ret) = &fn_data.returns {
                    map_to_c_def_type(&ret.r#type)
                } else {
                    "void".to_string()
                };

                let mut args = Vec::new();
                if !is_ctor {
                    // Methods usually take a pointer to the struct
                    args.push(format!("{}* self", class_c_name));
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(format!("{} {}", map_to_c_def_type(ty), name));
                    }
                }

                code.push_str(&format!("  {} {}({});\n", ret_type, c_symbol, args.join(", ")));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { write_sig(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { write_sig(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                code.push_str(&format!("  void {}_delete({}* instance);\n", class_c_name, class_c_name));
            }
        }
    }

    code.push_str("]]\n\n");

    // -------------------------------------------------------------------------
    // 3. Metatypes and Lua Wrappers
    // -------------------------------------------------------------------------
    
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            // --- A. Enums (Lua Tables) ---
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("M.{} = {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Map to the C enum value (which is exposed via ffi.C)
                            code.push_str(&format!("    {} = lib.{}_{},\n", 
                                variant_name, full_name, variant_name));
                        }
                    }
                    code.push_str("}\n\n");
                }
            }

            // --- B. Classes (Structs/Opaque) ---
            // Even if fields are missing (opaque), we can bind methods to the pointer type?
            // LuaJIT metatype works on struct types. If we have only a pointer, we need to typedef the struct.
            
            let has_methods = class_data.functions.is_some();
            let has_dtor = class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object;
            let dtor_name = format!("{}_delete", full_name);

            if has_methods || has_dtor {
                code.push_str(&format!("local {}_mt = {{}}\n", full_name));
                code.push_str(&format!("{}_mt.__index = {}_mt\n", full_name, full_name));

                // Generate Methods
                if let Some(functions) = &class_data.functions {
                    for (fn_name, _) in functions {
                         let camel_name = snake_case_to_lower_camel(fn_name);
                         let c_symbol = format!("{}_{}", full_name, camel_name);
                         
                         // Generate Lua wrapper method
                         // function AzWindow_mt:setTitle(...) return lib.AzWindow_setTitle(self, ...) end
                         code.push_str(&format!("function {}_mt:{}(...)\n", full_name, camel_name));
                         code.push_str(&format!("    return lib.{}(self, ...)\n", c_symbol));
                         code.push_str("end\n");
                    }
                }

                // Bind metatable to C struct
                // Note: ffi.metatype binds to the struct "AzWindow", not "AzWindow*"
                code.push_str(&format!("ffi.metatype(\"{}\", {}_mt)\n\n", full_name, full_name));
            }

            // --- C. Constructors (Module Level) ---
            if let Some(constructors) = &class_data.constructors {
                for (fn_name, fn_data) in constructors {
                    let camel_name = if fn_name == "new" {
                        format!("{}_new", full_name)
                    } else {
                         format!("{}_{}", full_name, snake_case_to_lower_camel(fn_name))
                    };
                    
                    let c_symbol = format!("{}_{}", full_name, snake_case_to_lower_camel(fn_name));
                    
                    code.push_str(&format!("function M.{}(...)\n", camel_name));
                    code.push_str(&format!("    local ptr = lib.{}(...)\n", c_symbol));
                    
                    // Attach Garbage Collector if it returns a pointer and has a destructor
                    let ret_is_ptr = fn_data.returns.as_ref().map_or(false, |r| r.r#type.contains('*'));
                    
                    if ret_is_ptr && has_dtor {
                        code.push_str("    if ptr ~= nil then\n");
                        code.push_str(&format!("        ffi.gc(ptr, lib.{})\n", dtor_name));
                        code.push_str("    end\n");
                    }
                    
                    code.push_str("    return ptr\n");
                    code.push_str("end\n\n");
                }
            }
        }
    }

    code.push_str("return M\n");
    code
}

/// Simplified C type mapper for ffi.cdef
fn map_to_c_def_type(ty: &str) -> String {
    // If it's a known struct starting with Az, use it as is
    if ty.starts_with(PREFIX) {
        return ty.to_string();
    }
    
    // Arrays [T; N] are valid C syntax
    if ty.starts_with('[') {
        // Rust: [u8; 4] -> C: uint8_t[4]
        // This parser needs to be robust, but simplified here:
        // assuming standard primitives
        if ty.contains("u8") { return "uint8_t[4]".to_string(); } // simplified hack
        return "void*".to_string(); 
    }

    match ty {
        "bool" => "AzGLboolean".to_string(),
        "u8" => "uint8_t".to_string(),
        "i8" => "int8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "i16" => "int16_t".to_string(),
        "u32" => "uint32_t".to_string(),
        "i32" => "int32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "int64_t".to_string(),
        "c_void" | "GLvoid" => "void".to_string(),
        "AzString" => "const char*".to_string(), // Lua strings map to char*
        s => s.to_string(),
    }
}
