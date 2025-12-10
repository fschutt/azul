use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

This generator produces a single Azul.pas unit file. It utilizes the 
ctypes unit to ensure types exactly match C ABI sizes (e.g., cint, cfloat).

1.  **Generate**: Run the generator to produce `Azul.pas`.
2.  **Usage**: Ensure `libazul.so` (Linux), `azul.dll` (Windows), or `libazul.dylib` (macOS) 
    is available to the executable.

### Key Pascal Specifics

1.  **`{$PACKRECORDS C}`**: This compiler directive is critical. It tells Free Pascal 
    to align struct fields exactly like GCC/Clang does. Without this, structs passed by value 
    will have garbage data due to misalignment.
2.  **`cdecl`**: Rust `extern "C"` functions use the C calling convention. You must mark Pascal 
    external functions with `cdecl` or you will get stack corruption crashes.
3.  **Pointers (`^TType`)**: We explicitly generate typed pointers (e.g., `PAzWindow = ^TAzWindow`). 
    This provides type safety. `AzApp_run` expects a `PAzApp`, so passing a `PAzWindow` would be a 
    compile-time error, unlike using generic `Pointer`.
4.  **`ctypes` Unit**: We map to `cint`, `cfloat` etc. This guarantees that `Azul.pas` works correctly 
    on both 32-bit and 64-bit systems without manual `Integer` vs `LongInt` adjustments.

*/

/*
// Usage in Pascal:

program AzulApp;

{$mode objfpc}{$H+}

uses
  ctypes,
  Azul; // The generated unit

var
  Config: PAzAppConfig;
  Opts: PAzWindowCreateOptions;
  App: PAzApp;
begin
  // Create Config
  // Note: Rust constructors return Pointers (PAzAppConfig)
  Config := AzAppConfig_new();
  
  // Create Window Options
  // nil is allowed for pointers
  Opts := AzWindowCreateOptions_new(nil);
  
  // Create App
  // Pass pointers
  App := AzApp_new(nil, Config);
  
  // Run
  AzApp_run(App, Opts);
  
  // Cleanup is manual in raw Pascal API unless you wrap it in classes
  // AzApp_delete(App); // Only if run returns
end.

*/

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/// Maps C/Rust types to Pascal types (using ctypes for safety)
fn map_to_pascal_type(ty: &str, is_ptr: bool) -> String {
    // Handle pointers
    if ty.contains('*') || ty.contains('&') {
        if ty.contains("char") {
            return "PChar".to_string();
        }
        if ty.contains("void") {
            return "Pointer".to_string();
        }
        // Extract inner type to make a typed pointer PMyStruct
        let inner = ty.replace("*", "").replace("&", "").replace("const", "").replace("mut", "").trim().to_string();
        if inner.starts_with(PREFIX) {
            return format!("P{}", inner); // PAzWindow
        }
        return "Pointer".to_string(); // Fallback
    }

    match ty {
        "void" | "c_void" => "Pointer".to_string(), // context dependent, usually ignored in return
        "bool" | "GLboolean" => "cbool".to_string(),
        "char" | "i8" => "cint8".to_string(),
        "u8" => "cuint8".to_string(),
        "i16" => "cint16".to_string(),
        "u16" | "AzU16" => "cuint16".to_string(),
        "i32" | "GLint" | "GLsizei" => "cint32".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "cuint32".to_string(),
        "i64" | "GLint64" => "cint64".to_string(),
        "u64" | "GLuint64" => "cuint64".to_string(),
        "f32" | "GLfloat" | "GLclampf" | "AzF32" => "cfloat".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "cdouble".to_string(),
        "usize" | "size_t" | "uintptr_t" => "csize_t".to_string(),
        "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => "cslong".to_string(), // or PtrInt
        "AzString" => "TAzString".to_string(), // Struct by value
        s if s.starts_with(PREFIX) => {
            // Struct passed by value
            format!("T{}", s)
        }
        _ => "Pointer".to_string(),
    }
}

pub fn generate_pascal_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Unit Header
    code.push_str("unit Azul;\n\n");
    code.push_str("{$mode objfpc}{$H+}\n"); // FreePascal Object mode, LongStrings
    code.push_str("{$PACKRECORDS C}\n");     // C-compatible struct alignment
    code.push_str("{$MACRO ON}\n\n");
    
    code.push_str("interface\n\n");
    code.push_str("uses ctypes;\n\n");
    
    code.push_str(&format!("const\n  AzulLib = '{}';\n\n", LIB_NAME));

    // 2. Forward Declarations
    // Pascal requires pointer types to be defined before use if they are referenced.
    code.push_str("type\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            // We define TAzWindow (Struct) and PAzWindow (Pointer to Struct)
            code.push_str(&format!("  P{} = ^T{};\n", full_name, full_name));
            // If it's an opaque struct (no fields), we need to define the T type stub
            if class_data.struct_fields.is_none() && class_data.enum_fields.is_none() {
                 code.push_str(&format!("  T{} = record end;\n", full_name));
            }
        }
    }
    code.push_str("\n");

    // 3. Types and Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            // Structs
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("  T{} = record\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        // Pascal reserved words check might be needed (e.g. 'type', 'end')
                        let safe_field_name = match field_name.as_str() {
                            "type" => "typ",
                            "end" => "end_",
                            "object" => "obj",
                            "string" => "str",
                            s => s,
                        };
                        let pas_type = map_to_pascal_type(&field_data.r#type, false);
                        code.push_str(&format!("    {}: {};\n", safe_field_name, pas_type));
                    }
                }
                code.push_str("  end;\n\n");
            }
            
            // Enums
            // We map enums to constants for ABI safety, 
            // but we alias the type to cuint32 to signify usage.
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("  T{} = cuint32;\n", full_name));
                    code.push_str("  const\n");
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    {}_{} = {};\n", full_name, variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    // 4. Function Declarations
    code.push_str("\n{ --- Function Declarations --- }\n\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut generate_decl = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let ret_ty_raw = if let Some(ret) = &fn_data.returns {
                     map_to_pascal_type(&ret.r#type, false)
                } else {
                    "".to_string()
                };
                
                let is_func = !ret_ty_raw.is_empty();
                let keyword = if is_func { "function" } else { "procedure" };
                
                let mut args = Vec::new();
                if !is_ctor {
                    // Methods take pointer to self
                    args.push(format!("instance: P{}", class_c_name));
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let pas_type = map_to_pascal_type(ty, false);
                        args.push(format!("{}: {}", name, pas_type));
                    }
                }
                
                let args_str = if args.is_empty() { "".to_string() } else { format!("({})", args.join("; ")) };
                let ret_str = if is_func { format!(": {}", ret_ty_raw) } else { "".to_string() };

                code.push_str(&format!("{} {}{}{}; cdecl; external AzulLib;\n", 
                    keyword, c_symbol, args_str, ret_str));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_decl(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_decl(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                code.push_str(&format!("procedure {}_delete(instance: P{}); cdecl; external AzulLib;\n", class_c_name, class_c_name));
            }
        }
    }
    
    code.push_str("\nimplementation\n\n");
    code.push_str("end.\n");
    code
}
