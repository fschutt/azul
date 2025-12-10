use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/*

While the original standard didn't define a C FFI, the modern open-source implementation, 
**Algol 68 Genie (a68g)**, supports calling C functions via the `ALIEN` definition.

This generator produces a single `azul.a68` file compatible with **Algol 68 Genie**.

We need the **Algol 68 Genie (a68g)** interpreter.

1.  **Generate**: Output the file to `azul.a68`.
2.  **Shared Library**: Ensure `libazul.so` (Linux) or `azul.dll` (Windows) is built.
3.  **Include File**: `azul.a68` is generated as a header-like file. You include it 
    in your main program.

A68G links dynamically at runtime using the `ALIEN` pragma.

**Linux:**
```bash
# Set library path so A68G finds libazul.so
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:.
a68g main.a68
```

**Windows:**
ensure `azul.dll` is in the current folder.

```cmd
a68g main.a68
```

### Important Algol 68 Details

1.  **`ALIEN`**: This keyword is specific to Algol 68 Genie. Standard Algol 68 
    (from the 70s) did not define this.
2.  **Naming**: A68 is often case-insensitive depending on compiler flags, but 
    C is case-sensitive. The generator maps the *Procedure Name* (A68 side) to 
    lowercase, but keeps the *Alien String* (C side) exactly as it appears in 
    the symbol table.
3.  **Strings**: `STRING` in A68G is a flexible array of characters. When passing 
    to `ALIEN`, A68G usually handles null-termination if the C prototype expects `char*`.
4.  **`VOID` vs `REF CHAR`**: A68's `VOID` is strictly for return types (like `void func()`). 
    It cannot be used as a type for variables (`void*`). We use `REF CHAR` as the generic 
    pointer type.

### Distribution

You distribute `azul.a68` + `libazul.so`. The user includes `azul.a68` at the top of 
their script.

*/

/*
    PROGRAM azul_app CONTEXT VOID
    USE "azul.a68" 
    BEGIN
        # Algol 68 comments #
        
        # 1. Create Config #
        REF AZAPPCONFIG config := azappconfig_new;
        
        # 2. Set options #
        # Note: Struct field access in A68 #
        # Accessing C structs passed by pointer usually requires dereferencing #
        # if mapped that way #
        # But for Opaque handles (REF STRUCT(INT opaque_dummy)), we just pass them around. #
        
        # 3. Create Window Options #
        REF AZWINDOWCREATEOPTIONS opts := azwindowcreateoptions_new(NIL);
        
        # 4. Create App #
        # NIL is the null pointer #
        REF AZAPP app := azapp_new(NIL, config);
        
        # 5. Run #
        azapp_run(app, opts);
        
        # 6. Cleanup (Manual, no RAII in A68) #
        azapp_delete(app)
    END
    FINISH
*/

/// Maps C types to Algol 68 Modes
fn map_to_algol_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "STRING".to_string(); // A68G marshals STRING to char*
        }
        // Generic pointer. In A68, we can use REF CHAR as a void* equivalent
        // or a specific Mode if defined.
        // For type safety, we check if it's one of our structs.
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        if inner.starts_with(PREFIX) {
            return format!("REF {}", inner.to_uppercase());
        }
        return "REF CHAR".to_string(); // Void* fallback
    }

    match ty {
        "void" | "c_void" => "VOID".to_string(),
        "bool" | "GLboolean" => "BOOL".to_string(),
        "char" | "u8" | "i8" => "CHAR".to_string(), // Byte
        // A68G ints vary, but usually INT is 32-bit or machine word.
        // SHORT INT is 16-bit, LONG INT 64-bit.
        "u16" | "i16" => "SHORT INT".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "INT".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" | "usize" | "size_t" => "LONG INT".to_string(),
        "f32" | "GLfloat" | "AzF32" => "REAL".to_string(),
        "f64" | "GLdouble" => "LONG REAL".to_string(),
        // Value structs
        s if s.starts_with(PREFIX) => s.to_uppercase(), 
        _ => "REF CHAR".to_string(),
    }
}

pub fn generate_algol68_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("CO Auto-generated Bindings for Azul GUI (Algol 68 Genie) CO\n");
    code.push_str("PRAGMAT\n");
    code.push_str("  alien_convention \"cdecl\"\n"); // Tell A68G to use C calling convention
    code.push_str("PRAGMAT\n\n");

    // 2. Forward Declarations (Modes)
    // Algol 68 is strictly typed. We declare opaque modes first.
    code.push_str("CO --- Forward Declarations --- CO\n");
    for (_, module) in &version_data.api {
        for (class_name, _) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name).to_uppercase();
            // We define a stub mode. If it's redefined later as a STRUCT, A68 allows this.
            // However, simply using the name requires it to be declared.
            // For now, we rely on the order of definition below.
        }
    }
    // Define a generic handle for void*
    code.push_str("MODE AZHANDLE = REF CHAR;\n\n");

    // 3. Enums (Int Constants)
    code.push_str("CO --- Constants / Enums --- CO\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name).to_uppercase();
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("CO Enum {} CO\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Algol 68 constants: INT name = value;
                            code.push_str(&format!("INT {}_{} = {};\n", full_name, variant_name.to_uppercase(), idx));
                            idx += 1;
                        }
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    // 4. Structs (Modes)
    code.push_str("CO --- Struct definitions --- CO\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name).to_uppercase();

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("MODE {} = STRUCT (\n", full_name));
                
                let mut fields = Vec::new();
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let a68_type = map_to_algol_type(&field_data.r#type);
                        fields.push(format!("  {} {}", a68_type, field_name));
                    }
                }
                code.push_str(&fields.join(",\n"));
                code.push_str("\n);\n\n");
            } else {
                // Opaque struct (used for pointers)
                // In A68, if we only use REF NAME, we can define NAME as CHAR or STRUCT(INT dummy).
                // Let's declare it as a dummy struct to allow strong typing.
                if class_data.enum_fields.is_none() {
                    code.push_str(&format!("MODE {} = STRUCT (INT opaque_dummy);\n", full_name));
                }
            }
        }
    }

    // 5. Foreign Function Declarations (ALIEN)
    code.push_str("CO --- External Procedures --- CO\n");
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_proc = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                // Algol Identifiers usually don't support underscores mixed nicely or are case sensitive.
                // We will use snake_case for A68 identifiers.
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                
                // Name in Algol source
                let a68_name = c_symbol.to_lowercase(); 

                let ret_raw = if let Some(ret) = &fn_data.returns {
                     map_to_algol_type(&ret.r#type)
                } else {
                    "VOID".to_string()
                };

                let mut args = Vec::new();
                if !is_ctor {
                    // Method: struct* self
                    args.push(format!("REF {} instance", class_c_name.to_uppercase()));
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(format!("{} {}", map_to_algol_type(ty), name));
                    }
                }
                
                let arg_sig = if args.is_empty() { "".to_string() } else { format!("({})", args.join(", ")) };

                code.push_str(&format!("PROC {} = {} {}: ALIEN \"{}\";\n", 
                    a68_name, arg_sig, ret_raw, c_symbol));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_proc(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_proc(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 let c_symbol = format!("{}_delete", class_c_name);
                 let a68_name = c_symbol.to_lowercase();
                 code.push_str(&format!("PROC {} = (REF {} instance) VOID: ALIEN \"{}\";\n",
                    a68_name, class_c_name.to_uppercase(), c_symbol));
            }
        }
        code.push_str("\n");
    }

    code
}
