use crate::{
    api::{ApiData, ClassData, FunctionData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";

/*

azul-go/
├── go.mod
├── azul.go       <-- Generated file
├── azul.h        <-- Copy generated C header here
├── libazul.so    <-- Shared library (Linux)
├── azul.dll      <-- Shared library (Windows)
└── libazul.dylib <-- Shared library (Mac)
*/

/*

### Handling Dynamic Libraries with `cgo`

`cgo` needs to find the library at **compile time** and **runtime**.

1.  **Compile Time**: The `#cgo LDFLAGS: -L. -lazul` line in the generated 
    code tells Go to look in the current directory (`.`) for `libazul.so`.
2.  **Runtime**: The user must have `libazul` in their system library path 
    (`LD_LIBRARY_PATH` on Linux, `PATH` on Windows) OR usually, you tell the 
    user to install the Rust dll to `/usr/local/lib`.

### Key Go Specifics

1.  **Garbage Collection**: The `runtime.SetFinalizer` in the generator is critical. 
    It bridges Go's GC with Rust's `Drop`. Without it, you leak memory.
2.  **CGO Performance**: Calling C from Go has a small overhead (stack switching). 
    It's fine for GUI calls (which are relatively infrequent compared to tight loops), but avoid calling C functions inside a tight pixel loop in Go.
3.  **Cross Compilation**: CGO makes cross-compilation (e.g., building for Windows 
    from Linux) harder. You need a C cross-compiler (like `MinGW`). This is unavoidable 
    for FFI bindings.

*/

/*
package main

import (
	"github.com/yourusername/azul-go"
	"fmt"
)

func main() {
    // Create struct options
    opts := azul.AzWindowCreateOptions_New()
    
    // Create config
    cfg := azul.AzAppConfig_New()
    
    // Create App
    // Note: nil passed as unsafe.Pointer if mapped that way
    app := azul.AzApp_New(nil, cfg)
    
    azul.AzApp_Run(app, opts)
    
    // Go GC handles cleanup via SetFinalizer generated in wrappers
}
*/

pub fn get_go_mod_file() -> String {
    format!("
module github.com/yourusername/azul-go

go 1.18
    ").trim().to_string()
}

/// Maps C types to Public Go types (e.g. "int32", "string", "*AzWindow")
fn map_to_go_public_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return "string".to_string();
        }
        // Extract base type name (e.g. "AzWindow* -> *AzWindow")
        let clean = ty.replace("*", "").replace("const", "").trim().to_string();
        // Check if primitive pointer or struct pointer
        match clean.as_str() {
            "void" | "c_void" | "GLvoid" => "unsafe.Pointer".to_string(),
             _ => format!("*{}", clean), // Return pointer to wrapper struct
        }
    }

    match ty {
        "void" | "c_void" => "".to_string(), // Handle separately for returns
        "bool" | "GLboolean" => "bool".to_string(),
        "char" | "i8" => "int8".to_string(),
        "u8" => "uint8".to_string(),
        "i16" => "int16".to_string(),
        "u16" | "AzU16" => "uint16".to_string(),
        "i32" | "GLint" | "GLsizei" => "int32".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "uint32".to_string(),
        "i64" | "GLint64" => "int64".to_string(),
        "u64" | "GLuint64" => "uint64".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float32".to_string(),
        "f64" | "GLdouble" => "float64".to_string(),
        "usize" | "size_t" | "uintptr_t" => "uint64".to_string(),
        "isize" | "ssize_t" | "intptr_t" => "int64".to_string(),
        s if s.starts_with(PREFIX) => s.to_string(), // Value structs passed as structs
        _ => "unsafe.Pointer".to_string(),
    }
}

pub fn generate_go_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Preamble & CGO Directives
    code.push_str("package azul\n\n");
    code.push_str("/*\n");
    code.push_str("#cgo LDFLAGS: -L. -lazul\n"); // Looks for libazul.so/.dll in current dir by default
    code.push_str("#include <stdlib.h>\n");      // For free()
    code.push_str("#include \"azul.h\"\n");      // Your generated C header
    code.push_str("*/\n");
    code.push_str("import \"C\"\n");
    code.push_str("import \"unsafe\"\n");
    code.push_str("import \"runtime\"\n\n");

    // 2. Enums (Const Blocks)
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("type {} uint32\n", full_name));
                    code.push_str("const (\n");
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    {}_{} {} = C.{}_{}\n", 
                                full_name, variant_name, full_name, full_name, variant_name));
                        }
                    }
                    code.push_str(")\n\n");
                }
            }
        }
    }

    // 3. Struct Wrappers
    // In Go, we wrap the C pointer in a struct to attach methods and finalizers
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            let has_fields = class_data.struct_fields.is_some();
            
            code.push_str(&format!("type {} struct {{\n", full_name));
            // We use a pointer to the C type. Cgo names it C.struct_AzName or just C.AzName
            // depending on the typedef.
            code.push_str(&format!("    ptr *C.struct_{}\n", full_name));
            code.push_str("}\n\n");
        }
    }

    // 4. Functions & Methods
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helpers for generation
            let mut generate_fn = |fn_raw_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let camel_name = snake_case_to_lower_camel(fn_raw_name);
                
                // Go Method Name: AzDom_New, AzDom_AppendChild
                // Note: We use "Public" case (Capitalized) for export
                let go_fn_name = if is_ctor {
                    // Constructor: static function in package, usually NewAzDom or AzDom_New
                    format!("{}_{}", class_c_name, camel_name) 
                } else {
                    // Method: func (self *AzDom) AppendChild(...)
                    // But to keep simple FFI generation, let's stick to PascalCase method names
                    // e.g., func (self *AzDom) AppendChild
                    // We need to capitalize the first letter of camel_name
                    let mut chars = camel_name.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                };

                // Build Arguments
                let mut go_args = Vec::new();
                let mut c_call_args = Vec::new();
                let mut pre_call_code = String::new();

                if !is_ctor {
                    // Receiver for methods
                    c_call_args.push("self.ptr".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        
                        let go_ty = map_to_go_public_type(ty);
                        go_args.push(format!("{} {}", name, go_ty));

                        // Convert Go type to C type for the call
                        if go_ty == "string" {
                            // CString allocation
                            pre_call_code.push_str(&format!("    c_{0} := C.CString({0})\n", name));
                            pre_call_code.push_str(&format!("    defer C.free(unsafe.Pointer(c_{0}))\n", name));
                            c_call_args.push(format!("c_{}", name));
                        } else if go_ty.starts_with("*") {
                            // Wrapper struct -> extract .ptr
                            c_call_args.push(format!("{}.ptr", name));
                        } else if go_ty == "bool" {
                            c_call_args.push(format!("C.bool({})", name));
                        } else {
                             // Cast primitive
                             let c_cast = match ty.as_str() {
                                 "u32" | "AzU32" => "C.uint",
                                 "i32" | "AzGLint" => "C.int",
                                 "f32" | "AzF32" => "C.float",
                                 "usize" => "C.size_t",
                                 _ => "C.int", // simplified fallback
                             };
                             c_call_args.push(format!("{}({})", c_cast, name));
                        }
                    }
                }

                // Return Type Logic
                let ret_ty_raw = fn_data.returns.as_ref().map(|r| r.r#type.as_str()).unwrap_or("void");
                let go_ret_ty = map_to_go_public_type(ret_ty_raw);
                let has_return = go_ret_ty != "";

                // Function Signature
                if is_ctor {
                    code.push_str(&format!("func {}({}) {} {{\n", go_fn_name, go_args.join(", "), go_ret_ty));
                } else {
                    code.push_str(&format!("func (self *{}) {}({}) {} {{\n", class_c_name, go_fn_name, go_args.join(", "), go_ret_ty));
                }

                // Body
                code.push_str(&pre_call_code);
                
                let c_func = if is_ctor {
                    format!("{}_{}", class_c_name, crate::utils::string::snake_case_to_lower_camel(fn_raw_name))
                } else {
                     format!("{}_{}", class_c_name, crate::utils::string::snake_case_to_lower_camel(fn_raw_name))
                };

                if has_return {
                    code.push_str(&format!("    ret := C.{}({})\n", c_func, c_call_args.join(", ")));
                    
                    // Convert C return to Go return
                    if go_ret_ty == "string" {
                         code.push_str("    return C.GoString(ret)\n");
                    } else if go_ret_ty.starts_with("*") {
                        // Returning a Struct Pointer (Wrapper)
                        let struct_name = go_ret_ty.trim_start_matches('*');
                        
                        code.push_str(&format!("    if ret == nil {{ return nil }}\n"));
                        code.push_str(&format!("    wrapper := &{}{{ ptr: ret }}\n", struct_name));

                        // Attach Finalizer if ownership is implied (e.g. constructors)
                        // Note: A real generator needs `api.json` to mark "transfer_ownership" explicitly.
                        // Here we heuristically attach finalizers to constructors or owned objects.
                        let has_delete = class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object;
                        if (is_ctor || ret_ty_raw.contains("owned")) && has_delete {
                             code.push_str(&format!("    runtime.SetFinalizer(wrapper, func(w *{}) {{ C.{}_delete(w.ptr) }})\n", 
                                struct_name, struct_name));
                        }
                        code.push_str("    return wrapper\n");
                    } else {
                        // Primitive cast
                        code.push_str(&format!("    return {}(ret)\n", go_ret_ty));
                    }
                } else {
                    code.push_str(&format!("    C.{}({})\n", c_func, c_call_args.join(", ")));
                }

                code.push_str("}\n\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_fn(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_fn(name, data, false); }
            }
        }
    }

    code
}
