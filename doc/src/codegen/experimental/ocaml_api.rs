use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

    For **OCaml**, the industry standard for binding to C libraries is **Ctypes**.

    Ctypes allows you to describe C types and functions using pure OCaml values. 
    It can operate in two modes:

    1.  **Foreign (Dynamic)**: Loads the shared library (`dlopen`) at runtime and 
        binds functions via `libffi`.
    2.  **Stub Generation**: Generates C code to be compiled.

    For ease of distribution and usage (similar to Python/Ruby examples), this generator 
    uses the **Foreign (Dynamic)** mode. This generates a single `azul.ml` file.

    ### Usage in OCaml

    OCaml uses **Dune** and **Opam**.

    #### 1. Setup

    Install dependencies:

    ```bash
    opam install ctypes ctypes-foreign
    ```

    ```
    azul-ml/
    ├── dune-project
    ├── src/
    │   ├── dune
    │   ├── azul.ml      <-- Generated file
    │   └── main.ml
    ├── libazul.so       <-- Native library
    ```

    ### Key OCaml Details

    1.  **Ctypes Dynamic**: The code generated uses `Foreign`. This means `libazul.so` 
        must be discoverable by `dlopen` at runtime (e.g., in `LD_LIBRARY_PATH` or the 
        current directory if configured).
    2.  **Type Safety**:
        *   `ptr void` allows passing any pointer.
        *   `ptr az_app` is specific. `AzApp_run` requires `ptr az_app`. OCaml's type system 
            ensures you don't pass an `AzWindow*` to a function expecting `AzApp*`.
    3.  **Structs**:
        *   `let v = make az_rect` creates a struct on the managed OCaml heap.
        *   `setf v az_rect_width 10.0` sets a field.
        *   Passing `addr v` to C passes the pointer to the struct.
    4.  **Integers**: OCaml's `int` is 63-bit (on 64-bit systems). C's `int` is 32-bit. Ctypes 
        handles conversion, but for struct fields, we use `int32_t` / `uint32_t` to be precise about memory layout.
    5.  **Garbage Collection**:
        
        ```ocaml
        let app_ptr = az_app_new null config
        let () = Gc.finalise az_app_delete app_ptr
        ```
        
        This idiom attaches the C destructor to the OCaml value. When `app_ptr` becomes unreachable, 
        `az_app_delete` is called. The generator provides the `_delete` function to enable this.
*/

/*
    open Azul
    open Ctypes
    open Foreign

    let () =
        (* Create Config *)
        let config = az_app_config_new () in
        
        (* Create Options *)
        (* passing Ctypes.null for C pointers *)
        let opts = az_window_create_options_new null in
        
        (* Create App *)
        (* null is type unit ptr, needs casting if function signature is strict, 
            but generated code uses `ptr void` for void*, which accepts null. *)
        let app = az_app_new null config in
        
        (* Run *)
        az_app_run app opts;
        
        (* OCaml GC finalizers for Ctypes *)
        (* Ctypes doesn't auto-free structs returned by C unless configured to do so.
            Since Azul uses manual destructors, you should wrap the pointer in a custom type 
            with a Gc.finalise hook if you want automatic memory management. *)
            
        az_app_delete app
*/

// `src/dune` file
pub fn get_src_dune_file() -> String {
    format!("
(executable
 (name main)
 (libraries ctypes ctypes.foreign))
    ")
}

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/// Maps C types to OCaml Ctypes definitions
fn map_to_ocaml_ctype(ty: &str) -> String {
    // Handle pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "string".to_string(); // Ctypes maps string <-> char*
        }
        
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        
        // Void* -> ptr void
        if inner == "void" || inner == "c_void" {
            return "ptr void".to_string();
        }
        
        // Pointer to known struct -> ptr az_window
        if inner.starts_with(PREFIX) {
            let snake = to_snake_case(&inner);
            return format!("ptr {}", snake);
        }
        
        return "ptr void".to_string();
    }

    match ty {
        "void" | "c_void" => "void".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "char" | "i8" => "char".to_string(),
        "u8" => "uint8_t".to_string(),
        "u16" | "AzU16" => "uint16_t".to_string(),
        "i16" => "int16_t".to_string(),
        // OCaml 'int' is 31/63 bit. For C compatibility, Ctypes uses explicit widths.
        "i32" | "GLint" | "GLsizei" => "int32_t".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "uint32_t".to_string(),
        "i64" | "GLint64" | "isize" | "ssize_t" | "intptr_t" => "int64_t".to_string(),
        "u64" | "GLuint64" | "usize" | "size_t" | "uintptr_t" => "uint64_t".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        // Struct passed by value
        s if s.starts_with(PREFIX) => to_snake_case(s),
        _ => "ptr void".to_string(),
    }
}

/// Helper: PascalCase to snake_case for OCaml variables
fn to_snake_case(s: &str) -> String {
    let mut res = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 { res.push('_'); }
            res.push(c.to_lowercase().next().unwrap());
        } else {
            res.push(c);
        }
    }
    res
}

pub fn generate_ocaml_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("(* Auto-generated OCaml bindings for Azul *)\n");
    code.push_str("open Ctypes\n");
    code.push_str("open Foreign\n\n");
    
    // Load Library
    code.push_str("let () = Dl.dlopen ~filename:\"");
    // Platform specific extension logic usually handled by build system, 
    // but here we hardcode a name for Dl to find.
    // Users usually symlink libazul.so to this name.
    code.push_str(LIB_NAME); 
    code.push_str("\" ~flags:[Dl.RTLD_LAZY; Dl.RTLD_GLOBAL] |> ignore\n\n");

    // 2. Struct Definitions (Phase 1: Declaration)
    // Ctypes requires creating the structure structure first, then defining fields.
    code.push_str("(* --- Type Declarations --- *)\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            let ocaml_name = to_snake_case(&full_name);
            
            // Generate struct stub
            if class_data.struct_fields.is_some() || class_data.is_boxed_object {
                code.push_str(&format!("type {}\n", ocaml_name));
                code.push_str(&format!("let {} : {} structure typ = structure \"{}\"\n", 
                    ocaml_name, ocaml_name, full_name));
            }
        }
    }
    code.push_str("\n");

    // 3. Struct Definitions (Phase 2: Fields & Seal)
    code.push_str("(* --- Field Definitions --- *)\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            let ocaml_name = to_snake_case(&full_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let c_type = map_to_ocaml_ctype(&field_data.r#type);
                        code.push_str(&format!("let {}_{} = field {} \"{}\" ({})\n", 
                            ocaml_name, field_name, ocaml_name, field_name, c_type));
                    }
                }
                code.push_str(&format!("let () = seal {}\n\n", ocaml_name));
            } else if class_data.is_boxed_object {
                // Opaque struct (seal without fields)
                code.push_str(&format!("let () = seal {}\n\n", ocaml_name));
            }
        }
    }

    // 4. Enums (Constants)
    code.push_str("(* --- Enums --- *)\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("module {} = struct\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Use Unsigned.UInt32 or int? C enums are usually ints.
                            // Ctypes usually maps them to int.
                            code.push_str(&format!("  let {} = {}\n", to_snake_case(variant_name), idx));
                            idx += 1;
                        }
                    }
                    code.push_str("end\n");
                }
            }
        }
    }
    code.push_str("\n");

    // 5. Function Bindings
    code.push_str("(* --- Functions --- *)\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut generate_fn = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                let ocaml_fn_name = to_snake_case(&c_symbol);

                let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_ocaml_ctype(&r.r#type));
                
                // Build signature DSL: type @-> type @-> returning type
                let mut args = Vec::new();
                if !is_ctor {
                    // Self pointer
                    let self_type = to_snake_case(&class_c_name);
                    args.push(format!("ptr {}", self_type));
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(map_to_ocaml_ctype(ty));
                    }
                }
                
                let sig_str = if args.is_empty() {
                    format!("returning {}", ret_type)
                } else {
                    format!("{} @-> returning {}", args.join(" @-> "), ret_type)
                };

                code.push_str(&format!("let {} = foreign \"{}\" ({})\n", ocaml_fn_name, c_symbol, sig_str));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_fn(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_fn(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 let self_type = to_snake_case(&class_c_name);
                 let dtor_name = format!("{}_delete", to_snake_case(&class_c_name));
                 let c_symbol = format!("{}_delete", class_c_name);
                 code.push_str(&format!("let {} = foreign \"{}\" (ptr {} @-> returning void)\n", 
                    dtor_name, c_symbol, self_type));
            }
        }
    }
    
    code
}
