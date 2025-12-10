use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
};

const PREFIX: &str = "AZ";

/*
    For **COBOL**, the primary modern open-source compiler is **GnuCOBOL** (formerly OpenCOBOL), 
    which transpiles COBOL to C. This makes it fully capable of linking against C-ABI shared libraries.

    This generator produces a **Copybook (`AZUL.CPY`)**.

    COBOL programmers use `COPY AZUL.` in their `DATA DIVISION` to get the struct layouts and constants.

    ### The Constraints

    1.  **Case Sensitivity**: COBOL is case-insensitive, but C linkers are **not**. The user must 
        invoke calls using string literals: `CALL "AzApp_new"`.
    2.  **Hyphens**: COBOL uses hyphens (`AZ-APP-CONFIG`), C uses underscores. The generator handles 
        this conversion for variable names.
    3.  **Types**: We use `COMP-5` (Native Binary) to ensure the memory layout matches C integers exactly.

    ### Usage in GnuCOBOL

    1.  **Generate**: Output to `AZUL.CPY`.
    2.  **Compile**: `libazul` must be installed.

    ### Compilation

    ```bash
    # Link against azul library
    cobc -x MAIN.COB -L. -lazul -o azul_app
    ./azul_app
    ```


    ### Key COBOL Nuances

    1.  **`BY VALUE` vs `BY REFERENCE`**:
        *   COBOL defaults to `BY REFERENCE`.
        *   C expects arguments `BY VALUE` (unless they are pointers).
        *   **Crucial Rule**: When calling C from COBOL:
            *   Pass Pointers (`USAGE POINTER`) -> `BY VALUE`.
            *   Pass Integers (`BINARY-LONG`) -> `BY VALUE`.
            *   Pass Structs -> `BY REFERENCE` (Equivalent to passing a pointer to the struct in C).
    2.  **`RETURNING`**: Used to capture the return value (usually a Pointer for constructors).
    3.  **`78` Level**: These act like preprocessor `#define`. `CALL FN-AZ-APP-NEW` gets compiled as 
        `CALL "AzApp_new"`, preserving the case sensitivity required by the linker.
    4.  **`TYPEDEF`**: The `01 TY-AZ-BLAH IS TYPEDEF` syntax allows you to create instances of that 
        struct in `WORKING-STORAGE` cleanly (`01 MY-VAR USAGE TY-AZ-BLAH`). This matches the C struct 
        layout (provided `COMP-5`/`BINARY-CHAR` are used).
*/

/*

    *> Usage.

    IDENTIFICATION DIVISION.
    PROGRAM-ID. AZUL-TEST.

    DATA DIVISION.
    WORKING-STORAGE SECTION.
    *> Include the bindings
    COPY "AZUL.CPY".

    *> Variables using defined typedefs
    01  WS-CONFIG       USAGE POINTER.
    01  WS-OPTS         USAGE POINTER.
    01  WS-APP          USAGE POINTER.

    *> Struct passed by value (if needed, example only)
    *> 01  WS-COLOR        USAGE TY-AZ-COLOR-U.

    PROCEDURE DIVISION.
    MAIN-LOGIC.
        DISPLAY "Starting Azul COBOL App...".

        *> Create Config (Call constructor)
        *> C: AzAppConfig_new()
        *> Passing NULL (0) or arguments BY VALUE
        CALL FN-AZ-APP-CONFIG-NEW RETURNING WS-CONFIG.
        
        *> Create Window Options
        *> C: AzWindowCreateOptions_new(NULL)
        CALL FN-AZ-WINDOW-CREATE-OPTIONS-NEW 
            USING BY VALUE 0 
            RETURNING WS-OPTS.
            
        *> Create App
        *> C: AzApp_new(NULL, config)
        CALL FN-AZ-APP-NEW 
            USING BY VALUE 0
                    BY VALUE WS-CONFIG
            RETURNING WS-APP.
            
        *> Run
        *> C: AzApp_run(app, opts)
        CALL FN-AZ-APP-RUN 
            USING BY VALUE WS-APP 
                    BY VALUE WS-OPTS.
        
        *> Cleanup (Manual)
        CALL FN-AZ-APP-DELETE USING BY VALUE WS-APP.

        STOP RUN.
*/


/// Convert snake_case or PascalCase to COBOL-CASE (Upper with hyphens)
fn to_cobol_case(s: &str) -> String {
    let mut res = String::new();
    let s = s.replace("_", "-"); // Rust snake_case to COBOL-CASE
    
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 && s.chars().nth(i-1).map_or(false, |p| p != '-') {
            res.push('-');
            res.push(c);
        } else {
            res.push(c.to_ascii_uppercase());
        }
    }
    // Clean up double dashes from previous replacement
    res.replace("--", "-").trim_matches('-').to_string()
}

/// Map C types to COBOL Picture/Usage clauses
fn map_to_cobol_pic(ty: &str) -> String {
    if ty.contains('*') {
        // All pointers are just USAGE POINTER
        return "USAGE POINTER".to_string();
    }

    match ty {
        "void" | "c_void" | "GLvoid" => "USAGE POINTER".to_string(), // Void* 
        // 1 byte int (approx) - COBOL doesn't have strict 1-byte without internal digits logic
        // COMP-5 is native binary. PIC 9(2) fits in 1 byte usually, but alignment varies.
        // BINARY-CHAR is standard COBOL 2002, GnuCOBOL supports it.
        "bool" | "GLboolean" => "USAGE BINARY-CHAR UNSIGNED".to_string(), 
        "char" | "u8" | "i8" => "USAGE BINARY-CHAR".to_string(),
        
        "u16" | "i16" | "AzU16" => "USAGE BINARY-SHORT".to_string(),
        
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" | "i32" | "GLint" | "GLsizei" => 
            "USAGE BINARY-LONG".to_string(), // 32-bit
            
        "u64" | "GLuint64" | "i64" | "GLint64" => 
            "USAGE BINARY-DOUBLE".to_string(), // 64-bit
        
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" => 
            "USAGE POINTER".to_string(), // Safest bet for size_t matching machine word
            
        "f32" | "GLfloat" | "AzF32" => "USAGE COMP-1".to_string(), // IEEE Float
        "f64" | "GLdouble" => "USAGE COMP-2".to_string(), // IEEE Double
        
        // Nested structs
        s if s.starts_with("Az") => {
            // In generated copybook, structs will be defined as 01 levels.
            // When used inside another struct, we ideally flatten it or refer to it.
            // COBOL doesn't support "Type" references easily in old standards, 
            // but GnuCOBOL supports `TYPEDEF`.
            // Here we assume we define them as TYPEDEFs.
            format!("USAGE IS TY-{}", to_cobol_case(s))
        }
        _ => "USAGE POINTER".to_string(),
    }
}

pub fn generate_cobol_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("      ******************************************************************\n");
    code.push_str("      * AZUL-GUI BINDINGS FOR GnuCOBOL                                 *\n");
    code.push_str("      * Auto-generated from api.json                                   *\n");
    code.push_str("      * Usage: COPY AZUL.                                              *\n");
    code.push_str("      ******************************************************************\n\n");

    // 2. Constants (Enums)
    // We use Level 78 (Micro Focus / GnuCOBOL extension) for true constants
    code.push_str("       *> --- ENUM CONSTANTS ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_cobol_name = to_cobol_case(&format!("{}{}", PREFIX, class_name));
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("       *> ENUM {}\n", class_cobol_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            let var_cobol = to_cobol_case(variant_name);
                            code.push_str(&format!("       78  {}-{:<30} VALUE {}.\n", 
                                class_cobol_name, var_cobol, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    // 3. Typedefs (Structs)
    // We use the `IS TYPEDEF` clause so users can declare `01 MY-VAR USAGE TY-AZ-RECT.`
    code.push_str("       *> --- DATA STRUCTURES ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_cobol_name = to_cobol_case(&format!("{}{}", PREFIX, class_name));

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("       01  TY-{} IS TYPEDEF.\n", class_cobol_name));
                
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let field_cobol = to_cobol_case(field_name);
                        let pic_usage = map_to_cobol_pic(&field_data.r#type);
                        
                        // Level 05 for fields
                        code.push_str(&format!("           05  {:<30} {}.\n", field_cobol, pic_usage));
                    }
                }
                code.push_str("\n");
            }
        }
    }
    
    // 4. Function Names (Constants)
    // To avoid case-sensitivity issues, we define constants for function names
    // CALL AZ-APP-NEW ... is easier than remembering CALL "AzApp_new"
    code.push_str("       *> --- C FUNCTION NAMES ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Generator helper
            let mut emit_const_name = |fn_name: &str, is_ctor: bool| {
                 let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                 } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                 };
                 
                 // COBOL constant: FN-AZ-APP-NEW
                 let cobol_sym = to_cobol_case(&c_symbol);
                 code.push_str(&format!("       78  FN-{} VALUE \"{}\".\n", cobol_sym, c_symbol));
                 
                 // Generate Destructor name if needed
                 if is_ctor && (class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object) {
                     let dtor_c = format!("{}_delete", class_c_name);
                     let dtor_cob = to_cobol_case(&dtor_c);
                     code.push_str(&format!("       78  FN-{} VALUE \"{}\".\n", dtor_cob, dtor_c));
                 }
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, _) in ctors { emit_const_name(name, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, _) in fns { emit_const_name(name, false); }
            }
        }
    }

    code
}