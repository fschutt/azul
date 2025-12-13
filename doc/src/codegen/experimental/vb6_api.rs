use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";
const DLL_NAME: &str = "azul.dll";

/*

This generates `Azul.bas`, which you simply drag-and-drop into the VB6 
IDE Project Explorer.

**Crucial Constraints for VB6:**
1.  **32-bit Only**: VB6 produces and runs 32-bit binaries. You **must** 
    compile your Rust library as 32-bit (`i686-pc-windows-msvc`). If you 
    use the 64-bit DLL, VB6 will raise "Bad DLL Calling Convention" or "File not found".
2.  **Unicode**: VB6 `String` is BSTR (UTF-16). C uses `char*` (UTF-8). By default, 
    VB6 converts Strings to ANSI (current system codepage) when calling DLLs. This usually 
    works for English text, but proper UTF-8 handling requires byte arrays. This generator 
    uses standard `String` for simplicity.
3.  **Pass-By-Value Structs**: VB6 **cannot** pass user-defined types (structs) ByVal to a 
    DLL. It can only pass them ByRef (as a pointer). If your C API expects a Struct by Value 
    (e.g. `AzColorU`), this generated code might crash unless you specifically wrap those C 
    functions to accept pointers.
*/

/*

' Form1.frm

Private Sub Form_Load()
    Dim config As Long
    Dim opts As Long
    Dim app As Long
    
    ' Create Config (Returns Pointer as Long)
    config = AzAppConfig_new()
    
    ' Create Options
    ' Note: If passing a callback is required, use AddressOf MyCallback
    opts = AzWindowCreateOptions_new(0)
    
    ' Create App
    app = AzApp_new(0, config)
    
    ' Run
    AzApp_run app, opts
    
    ' Cleanup (VB6 has no GC hooks for Modules, must call manually or in Form_Unload)
    ' AzApp_delete app ' (If run returns, which it usually doesnt)
End Sub

*/

/// Maps C types to VB6 types
/// Note: Pointers are mapped to Long (32-bit integer)
fn map_to_vb6_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            // VB6 auto-converts String to ANSI Z-String
            return "String".to_string(); 
        }
        // Void* or Struct* -> Long (32-bit pointer)
        return "Long".to_string();
    }

    match ty {
        "void" | "c_void" => "".to_string(), // Sub
        // Warning: C bool is 1 byte, VB6 Boolean is 2 bytes. 
        // We use Byte to prevent stack misalignment.
        // 1 = True, 0 = False.
        "bool" | "GLboolean" => "Byte".to_string(), 
        "char" | "u8" | "i8" => "Byte".to_string(),
        "u16" | "i16" | "AzU16" => "Integer".to_string(),
        // VB6 Long is 32-bit
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" | "usize" | "size_t" => "Long".to_string(),
        "f32" | "GLfloat" | "AzF32" => "Single".to_string(),
        "f64" | "GLdouble" => "Double".to_string(),
        // VB6 has no native 64-bit integer. Currency is 64-bit fixed point.
        // It consumes 8 bytes on stack, so it "works" for marshaling u64 by value, 
        // but math operations will be scaled by 10,000.
        "u64" | "i64" | "GLuint64" => "Currency".to_string(),
        // Structs by value. VB6 cannot pass UDTs ByVal.
        // We map them to the UDT name, but they must be passed ByRef (Pointer) in Declare.
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "Long".to_string(),
    }
}

pub fn generate_vb6_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("Attribute VB_Name = \"Azul\"\n");
    code.push_str("' Auto-generated bindings for Azul GUI\n");
    code.push_str("' Target: 32-bit Windows (i686-pc-windows-msvc)\n\n");
    code.push_str("Option Explicit\n\n");

    // 2. Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    code.push_str(&format!("Public Enum {}\n", full_name));
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

    // 3. Types (Structs)
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("Public Type {}\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let vb_type = map_to_vb6_type(&field_data.r#type);
                        // VB6 Type fields cannot be initialized, no syntax needed
                        code.push_str(&format!("    {} As {}\n", field_name, vb_type));
                    }
                }
                code.push_str("End Type\n\n");
            }
        }
    }

    // 4. Declare Statements
    code.push_str("' --- Native Functions ---\n\n");
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut generate_decl = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                // VB6 uses 'Sub' for void, 'Function' for returns
                let ret_type_raw = if let Some(ret) = &fn_data.returns {
                     map_to_vb6_type(&ret.r#type)
                } else {
                    "".to_string()
                };
                
                let is_sub = ret_type_raw.is_empty();
                let decl_type = if is_sub { "Sub" } else { "Function" };
                let return_clause = if is_sub { "".to_string() } else { format!(" As {}", ret_type_raw) };

                // Build Args
                let mut args = Vec::new();
                if !is_ctor {
                    // Self pointer (ByVal because it's a pointer value, essentially)
                    args.push("ByVal instance As Long".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let vb_type = map_to_vb6_type(ty);
                        
                        // Heuristic:
                        // Pointers (*), Strings, and Primitives -> ByVal
                        // Structs (UDTs) -> ByRef
                        let is_pointer = ty.contains('*');
                        let is_primitive = is_primitive_arg(ty) || vb_type == "String";
                        
                        let by_clause = if is_pointer || is_primitive { "ByVal" } else { "ByRef" };
                        
                        args.push(format!("{} {} As {}", by_clause, name, vb_type));
                    }
                }

                code.push_str(&format!("Public Declare {} {} Lib \"{}\" ({}){}\n", 
                    decl_type, c_symbol, DLL_NAME, args.join(", "), return_clause));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_decl(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_decl(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 code.push_str(&format!("Public Declare Sub {}_delete Lib \"{}\" (ByVal instance As Long)\n", 
                    class_c_name, DLL_NAME));
            }
        }
        code.push_str("\n");
    }

    code
}