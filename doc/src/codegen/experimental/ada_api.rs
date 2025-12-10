use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/*
    For **Ada**, the standard way to interface with C is using the `Interfaces.C` standard 
    library packages. Ada allows you to specify the **Convention** (Cdecl), **Link Name**, 
    and memory layout directly in the specification (`.ads`), meaning you rarely need a C glue layer.

    This generator produces `azul.ads` (The package specification).

    ### Usage in Ada

    1.  **Generate**: Save as `azul.ads`.
    2.  **Dependencies**: You need `libazul.so`/`.dll` visible to the linker.
    3.  **Build:** `gprbuild -P default.gpr`

    ### Key Ada Nuances

    1.  **Convention C**: `pragma Convention (C, Type)` is the most important part. It ensures 
        records are laid out without Ada's dope vectors or specific padding, matching Rust `#[repr(C)]`.
    2.  **Access Types**: `type T_Access is access all T`. This roughly translates to `T*`. Using `access all` 
        allows it to point to both heap-allocated (aliased) objects and C pointers.
    3.  **Strings**: `Interfaces.C.Strings.chars_ptr` is the C string type. You cannot pass standard Ada `String` 
        directly to C. You must use `New_String("Text")` to allocate a C-string, pass it, and then `Free` it. 
        The generated API expects `chars_ptr`.
    4.  **Enums**: Ada enums are distinct types. You cannot accidentally pass an integer where an enum is expected, 
        offering great type safety over C.
    5.  **Linker Options**: `pragma Linker_Options ("-lazul")` inside the spec file tells the GNAT builder to 
        automatically link the library, which is a nice convenience feature of Ada.
*/

/*

    with Azul; use Azul;
    with Interfaces.C; use Interfaces.C;
    with Interfaces.C.Strings; use Interfaces.C.Strings;
    with System;

    procedure Main is
    Config : AzAppConfig_Access;
    Opts   : AzWindowCreateOptions_Access;
    App    : AzApp_Access;
    begin
    -- Create Config
    -- Returns Access (Pointer) to AzAppConfig
    Config := AzAppConfig_new;
    
    -- Create Options
    -- Pass Null_Address for null pointers if arguments require System.Address
    -- Here we generated _Access types, so simply passing null works for optional pointers
    Opts := AzWindowCreateOptions_new(null);
    
    -- Create App
    App := AzApp_new(null, Config);
    
    -- Run
    AzApp_run(App, Opts);
    
    -- Manual cleanup
    -- AzApp_delete(App);
    end Main;

*/

pub fn default_gpr_file() -> String {
    format!("
project Default is
   for Source_Dirs use (\"src\");
   for Object_Dir use \"obj\";
   for Main use (\"main.adb\");
   
   package Linker is
      -- Link against azul
      for Switches (\"Ada\") use (\"-L.\", \"-lazul\");
   end Linker;
end Default;
    ")
}

/// Maps C/Rust types to Ada C-binding types
fn map_to_ada_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "chars_ptr".to_string(); // Interfaces.C.Strings
        }
        // Check if it's a pointer to a known struct
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        if inner.starts_with(PREFIX) {
            // We define Access types for structs: type AzWindow_Access is access all AzWindow;
            return format!("{}_Access", inner);
        }
        return "System.Address".to_string(); // Void*
    }

    match ty {
        "void" | "c_void" | "GLvoid" => "null".to_string(), // handled in logic
        "bool" | "GLboolean" => "bool".to_string(), // C Bool
        "char" | "u8" | "i8" => "unsigned_char".to_string(), // or char
        "u16" | "i16" => "short".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "int".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" => "long_long".to_string(), // standard C long long is 64bit usually
        "f32" | "GLfloat" | "AzF32" => "C_float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        "usize" | "size_t" => "size_t".to_string(),
        "isize" | "ssize_t" | "intptr_t" => "ptrdiff_t".to_string(), 
        // Struct by value
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "System.Address".to_string(),
    }
}

/// Sanitizes identifiers for Ada (reserved words)
fn sanitize_ada_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "type" | "end" | "begin" | "procedure" | "function" | "package" | 
        "record" | "is" | "in" | "out" | "access" | "constant" | "array" | 
        "range" | "digits" | "delta" | "null" => format!("{}_K", name),
        _ => name.to_string(),
    }
}

pub fn generate_ada_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("with Interfaces.C; use Interfaces.C;\n");
    code.push_str("with Interfaces.C.Strings; use Interfaces.C.Strings;\n");
    code.push_str("with System;\n\n");
    
    code.push_str("package Azul is\n");
    code.push_str("   pragma Preelaborate;\n");
    code.push_str(&format!("   pragma Linker_Options (\"-l{}\");\n\n", LIB_NAME));

    // 2. Forward Declarations
    // Ada needs incomplete types to define access types (pointers) before the full record definition
    code.push_str("   -- Forward Declarations\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if class_data.struct_fields.is_some() || class_data.is_boxed_object {
                code.push_str(&format!("   type {};\n", full_name));
                code.push_str(&format!("   type {}_Access is access all {};\n", full_name, full_name));
                code.push_str(&format!("   pragma Convention (C, {});\n", full_name));
                code.push_str(&format!("   pragma Convention (C, {}_Access);\n", full_name));
            }
        }
    }
    code.push_str("\n");

    // 3. Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("   type {} is \n     (", full_name));
                    let mut variants = Vec::new();
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            variants.push(format!("{}_{}", full_name, variant_name));
                        }
                    }
                    code.push_str(&variants.join(",\n      "));
                    code.push_str(");\n");
                    code.push_str(&format!("   pragma Convention (C, {});\n\n", full_name));
                }
            }
        }
    }

    // 4. Struct Definitions
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("   type {} is record\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let ada_name = sanitize_ada_name(field_name);
                        let ada_type = map_to_ada_type(&field_data.r#type);
                        code.push_str(&format!("      {} : {};\n", ada_name, ada_type));
                    }
                }
                code.push_str("   end record;\n\n");
            } else if class_data.is_boxed_object && class_data.enum_fields.is_none() {
                // Opaque handle
                code.push_str(&format!("   type {} is record\n", full_name));
                code.push_str("      Dummy : System.Address;\n");
                code.push_str("   end record;\n\n");
            }
        }
    }

    // 5. Subprograms (Functions)
    code.push_str("   -- Functions\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_subprog = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                // Ada doesn't like underscores at the start or double underscores, but standard C mapping usually fine
                let ada_func_name = c_symbol.clone(); 

                let ret_type = fn_data.returns.as_ref().map(|r| map_to_ada_type(&r.r#type));
                let is_func = ret_type.is_some() && ret_type.as_ref().unwrap() != "null";

                if is_func {
                    code.push_str(&format!("   function {} ", ada_func_name));
                } else {
                    code.push_str(&format!("   procedure {} ", ada_func_name));
                }

                let mut args = Vec::new();
                if !is_ctor {
                    // Self pointer. In C it's Type*. In Ada we use Type_Access.
                    // Important: For safety with C APIs, passing an access type (pointer) usually implies 'in'.
                    args.push(format!("Instance : in {}_Access", class_c_name));
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let safe_name = sanitize_ada_name(name);
                        let ada_ty = map_to_ada_type(ty);
                        
                        // Heuristic: Structs passed by value need 'in'. Pointers (Access types) are also 'in'.
                        // To be safe, most C parameters are 'in' unless they are out-pointers.
                        args.push(format!("{} : in {}", safe_name, ada_ty));
                    }
                }

                if !args.is_empty() {
                    code.push_str("(\n      ");
                    code.push_str(&args.join(";\n      "));
                    code.push_str(")");
                }

                if is_func {
                    code.push_str(&format!(" return {};\n", ret_type.unwrap()));
                } else {
                    code.push_str(";\n");
                }
                
                code.push_str(&format!("   pragma Import (C, {}, \"{}\");\n\n", ada_func_name, c_symbol));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_subprog(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_subprog(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                let dtor_name = format!("{}_delete", class_c_name);
                code.push_str(&format!("   procedure {} (Instance : in {}_Access);\n", dtor_name, class_c_name));
                code.push_str(&format!("   pragma Import (C, {}, \"{}\");\n\n", dtor_name, dtor_name));
            }
        }
    }

    code.push_str("end Azul;\n");
    code
}
