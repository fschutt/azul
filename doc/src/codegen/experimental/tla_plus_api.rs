use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
};

const PREFIX: &str = "Az";

/*

    NOTE: TLA+ is not a programming language. You don't "run" TLA+ to create a GUI window; 
    you use it to **model** the logic of your application to prove it has no bugs.

    This generator produces `Azul.tla`. It maps C structs to **TLA+ Sets of Records** and C enums 
    to **Sets of Strings**.

    Output the generator result to `Azul.tla` - then create a spec:

*/

/*
    ---------------------------- MODULE MyGuiSpec ----------------------------
    EXTENDS Azul, Integers

    VARIABLES 
        windowOptions, 
        appState

    (* Initialize state using generated constructors *)
    Init ==
        /\ windowOptions = AzWindowCreateOptions_new(800, 600, "My App")
        /\ appState = AzApp_Set (* Start in set of valid apps *)

    (* Define a transition (Next) *)
    ResizeWindow ==
        (* Use type checking from generated Sets *)
        /\ windowOptions \in AzWindowCreateOptions_Set
        /\ windowOptions' = [windowOptions EXCEPT !.width = 1024]
        /\ UNCHANGED <<appState>>

    Next == ResizeWindow

    =============================================================================
*/

/// Maps C types to TLA+ Sets (Types)
/// TLA+ is untyped, but we define Sets that represent types (e.g., Int, BOOLEAN).
fn map_to_tla_set(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return "STRING".to_string();
        }
        // Pointers are modeled as Integers (memory addresses) or Model Values
        return "Int".to_string(); 
    }

    match ty {
        "void" | "c_void" => "{0}".to_string(), // Unit set
        "bool" | "GLboolean" => "BOOLEAN".to_string(),
        // TLA+ Integers are infinite precision. We just use 'Int'.
        // If you want bounds checking, you'd generate (-128..127).
        "char" | "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" | "usize" | "isize" | "u64" | "i64" => "Int".to_string(),
        "f32" | "GLfloat" | "AzF32" | "f64" | "GLdouble" => "Real".to_string(), // Requires EXTENDS Reals
        // Structs: We refer to the generated Set definition
        s if s.starts_with(PREFIX) => format!("{}_Set", s),
        _ => "Int".to_string(),
    }
}

pub fn generate_tla_module(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Module Header
    code.push_str("---------------------------- MODULE Azul ----------------------------\n");
    code.push_str("EXTENDS Integers, Sequences, FiniteSets, Reals, TLC\n\n");
    code.push_str("(* Auto-generated TLA+ definitions for the Azul GUI Toolkit *)\n\n");

    // 2. Constants / Enums
    // In TLA+, enums are usually represented as Strings for readability in traces.
    code.push_str("(* --- Enums --- *)\n\n");
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    // Define each variant as a string constant
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                             code.push_str(&format!("{}_{} == \"{}_{}\"\n", 
                                full_name, variant_name, full_name, variant_name));
                        }
                    }
                    
                    // Define the Set containing all variants
                    code.push_str(&format!("\n{}_Set == {{\n", full_name));
                    let mut variants = Vec::new();
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            variants.push(format!("  {}_{}", full_name, variant_name));
                        }
                    }
                    code.push_str(&variants.join(",\n"));
                    code.push_str("\n}\n\n");
                }
            }
        }
    }

    // 3. Struct Definitions (Sets of Records)
    code.push_str("(* --- Data Structures (Sets of Records) --- *)\n\n");
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                // TLA+ Record Set Syntax: [ key: Set, key2: Set ]
                code.push_str(&format!("{}_Set == [\n", full_name));
                
                let mut fields = Vec::new();
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let tla_type = map_to_tla_set(&field_data.r#type);
                        fields.push(format!("  {} : {}", field_name, tla_type));
                    }
                }
                code.push_str(&fields.join(",\n"));
                code.push_str("\n]\n\n");
                
                // 3b. Constructor Operators
                // These are helpers to create a valid record for use in specs
                // "AzRect_new(w, h)"
                code.push_str(&format!("(* Constructor for {} *)\n", full_name));
                
                let mut arg_names = Vec::new();
                let mut record_assigns = Vec::new();
                
                for field_map in struct_fields {
                    for (field_name, _) in field_map {
                        arg_names.push(field_name.clone());
                        record_assigns.push(format!("  {} |-> {}", field_name, field_name));
                    }
                }
                
                code.push_str(&format!("{}_new({}) == [\n", full_name, arg_names.join(", ")));
                code.push_str(&record_assigns.join(",\n"));
                code.push_str("\n]\n\n");
            } else {
                // Opaque Handle: Model it as an Integer (Pointer address)
                // or just the set of Integers for type checking
                if class_data.enum_fields.is_none() {
                    code.push_str(&format!("{}_Set == Int\n", full_name));
                }
            }
        }
    }

    // 4. Function Signatures (Comments)
    // TLA+ cannot natively call C functions. 
    // We generate comments showing the signatures so the user can model them as Actions.
    code.push_str("(* --- Function Models (Reference) --- *)\n");
    code.push_str("(* To use these in TLA+, define operators that mimic the state change *)\n\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            if let Some(functions) = &class_data.functions {
                for (fn_name, fn_data) in functions {
                    code.push_str(&format!("(* {}_{} *)\n", full_name, fn_name));
                    code.push_str(&format!("(* Arguments: *)\n"));
                    for arg in &fn_data.fn_args {
                         for (name, ty) in arg {
                             code.push_str(&format!("(*   {}: {} *)\n", name, ty));
                         }
                    }
                    if let Some(ret) = &fn_data.returns {
                         code.push_str(&format!("(* Returns: {} *)\n", ret.r#type));
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    code.push_str("=============================================================================\n");
    code
}
