use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

    Generates bindings for **FreeBASIC** (`.bi` file).

    FreeBASIC is the industry-standard modern BASIC compiler that supports:

    1.  **C Interop**: `Extern "C"` blocks.
    2.  **Pointers**: `Any Ptr`, `Type Ptr`.
    3.  **Modern Systems**: Windows, Linux, DOS (32-bit/64-bit).

    NOTE: Running actual DEC BASIC-11 on a PDP-11 is impossible with Rust, but FreeBASIC 
    code looks very similar (`DIM`, `CALL`, `INTEGER`).

    1.  **Save** output as `azul.bi`.
    2.  **Compile** your program: `fbc myapp.bas` (ensure `libazul.so` / `azul.dll` is visible).
*/

/*
    ' myapp.bas
    #include "azul.bi"

    Dim config As AzAppConfig Ptr
    Dim opts As AzWindowCreateOptions Ptr
    Dim app As AzApp Ptr

    ' Create Config (Returns Ptr)
    config = AzAppConfig_new()

    ' Create Options (0 = NULL)
    opts = AzWindowCreateOptions_new(0)

    ' Create App
    app = AzApp_new(0, config)

    ' Run
    AzApp_run(app, opts)

    ' Cleanup
    AzApp_delete(app)

*/

const PREFIX: &str = "Az";

/// Maps C/Rust types to FreeBASIC types
fn map_to_fb_type(ty: &str) -> String {
    // Handle Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "ZString Ptr".to_string(); // C-style null-terminated string
        }
        
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        
        if inner == "void" || inner == "c_void" {
            return "Any Ptr".to_string();
        }
        
        if inner.starts_with(PREFIX) {
            return format!("{} Ptr", inner);
        }
        
        return "Any Ptr".to_string();
    }

    match ty {
        "void" | "c_void" => "Any".to_string(), // Context dependent
        // FreeBASIC types
        "bool" | "GLboolean" => "UByte".to_string(), // C bool is usually 1 byte
        "char" | "u8" | "i8" => "UByte".to_string(),
        "u16" | "i16" | "AzU16" => "UShort".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "UInteger".to_string(),
        "i32" | "GLint" | "GLsizei" => "Integer".to_string(), // In FB, Integer is native width (32-bit on x86)
        
        // 64-bit types
        "u64" | "GLuint64" | "usize" | "size_t" => "ULongInt".to_string(),
        "i64" | "GLint64" | "isize" | "ssize_t" | "intptr_t" => "LongInt".to_string(),
        
        "f32" | "GLfloat" | "AzF32" => "Single".to_string(),
        "f64" | "GLdouble" => "Double".to_string(),
        
        // Structs passed by value
        s if s.starts_with(PREFIX) => s.to_string(),
        
        _ => "Any Ptr".to_string(),
    }
}

pub fn generate_freebasic_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("' Auto-generated bindings for Azul GUI (FreeBASIC)\n");
    code.push_str("' To use: #include \"azul.bi\"\n");
    // This tells the linker to look for libazul.so / azul.dll
    code.push_str("#inclib \"azul\"\n\n");
    
    code.push_str("Extern \"C\"\n\n");

    // 2. Enums
    code.push_str("' --- Enums ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("Enum {}\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    {}_{} = {}\n", full_name, variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("End Enum\n\n");
                }
            }
        }
    }

    // 3. Structs (Types)
    code.push_str("' --- Types ---\n");
    
    // First pass: Forward declarations / Opaque handles
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            // If it's an opaque object (no struct fields exposed), we usually handle it as Any Ptr.
            // But to allow strict typing (AzWindow Ptr), we can define an empty Type.
            if class_data.struct_fields.is_none() && class_data.enum_fields.is_none() {
                 code.push_str(&format!("Type {}\n    __opaque As UByte\nEnd Type\n", full_name));
            }
        }
    }
    code.push_str("\n");

    // Second pass: Full Struct Definitions
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("Type {}\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let fb_type = map_to_fb_type(&field_data.r#type);
                        code.push_str(&format!("    {} As {}\n", field_name, fb_type));
                    }
                }
                code.push_str("End Type\n\n");
            }
        }
    }

    // 4. Function Declarations
    code.push_str("' --- Functions ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_decl = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                
                // Determine Sub (void) or Function
                let ret_raw = if let Some(ret) = &fn_data.returns {
                     map_to_fb_type(&ret.r#type)
                } else {
                    "Any".to_string() // "Any" in return usually implies Sub if check below fails
                };
                
                let is_sub = match fn_data.returns.as_ref() {
                    Some(r) => r.r#type == "void" || r.r#type == "c_void",
                    None => true,
                };
                
                let decl_key = if is_sub { "Declare Sub" } else { "Declare Function" };
                
                // Arguments
                let mut args = Vec::new();
                if !is_ctor {
                    // Self pointer
                    args.push(format!("ByVal instance As {} Ptr", class_c_name));
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let fb_ty = map_to_fb_type(ty);
                        
                        // C ABI usually expects primitives and pointers ByVal.
                        // Structs by value are also ByVal.
                        // FreeBASIC 'ByRef' passes a pointer to the data, which matches C pointer semantics if the type is not a Ptr.
                        // But since we mapped everything to explicit types or Ptrs, ByVal is safer for C interop.
                        args.push(format!("ByVal {} As {}", name, fb_ty));
                    }
                }
                
                let arg_str = args.join(", ");
                let ret_str = if is_sub { "".to_string() } else { format!(" As {}", ret_raw) };

                // Alias is required because FB converts names to UPPERCASE by default
                code.push_str(&format!("{} {} Alias \"{}\" ({}){}\n", 
                    decl_key, c_symbol, c_symbol, arg_str, ret_str));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_decl(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_decl(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 code.push_str(&format!("Declare Sub {}_delete Alias \"{}_delete\" (ByVal instance As {} Ptr)\n", 
                    class_c_name, class_c_name, class_c_name));
            }
        }
        code.push_str("\n");
    }

    code.push_str("End Extern\n");
    code
}
