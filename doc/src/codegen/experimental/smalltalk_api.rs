use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";
// The name of the library class in Smalltalk
const LIB_CLASS: &str = "AzulLibrary"; 
const LIB_FILENAME: &str = "azul";

/*

    For **Smalltalk**, specifically modern environments like 
    **Pharo** or **Squeak**, the standard interface is **UnifiedFFI (UFFI)**.

    Smalltalk is a live environment. Unlike other languages where you 
    compile source files, in Smalltalk you typically "File In" a script 
    that defines classes and methods dynamically in the image.

    This generator produces `Azul.st`.

    ### Part 2: Usage in Pharo / Squeak

    1.  **Generate**: Output to `Azul.st`.
    2.  **Import**: Drag and drop `Azul.st` into the Pharo window, 
        or use the "File Browser" tool to "File In" the script.
    3.  **DLL**: Ensure `libazul.so`/`.dll` is in the VM folder or system path.

    ### Key Smalltalk Details

    1.  **Chunk Format**: The generator outputs the classic "Chunk" 
        format (`!`). This is the lowest common denominator for Smalltalk exchange.
    2.  **`FFIExternalStructure`**: Pharo automatically generates accessors 
        (getters/setters) for the fields defined in `fieldsDesc` when the class is initialized.
    3.  **Memory Management**:
        *   `AzAppConfig new` creates an empty struct in Smalltalk memory.
        *   `AzulLibrary azAppConfigNew` calls C and returns a pointer/struct from C.
        *   *Auto-Release*: To make it "Smalltalk-ish", you would wrap the handle in 
            an object that uses `FFIExternalResourceManager` or `finalizationRegistry` to 
            call `AzulLibrary azAppDelete: handle` when garbage collected. The generator 
            provides the raw API; you can add the finalization logic in the image.
    1.  **`module: AzulLibrary`**: This tells UFFI to look up the shared library name by 
        sending `moduleName` (or `macLibraryName` etc.) to the `AzulLibrary` class.

    ### Packaging

    In the Smalltalk world, you distribute code via **Monticello**, **Metacello**, or **Iceberg** (Git).

    1.  File In the `Azul.st`.
    2.  Open **Iceberg**, create a new repository/package "Azul".
    3.  Move the classes (`AzulLibrary`, `AzApp`, `AzulConstants`) into that package.
    4.  Commit and Push to GitHub.
    5.  Users load it via Metacello:
        ```smalltalk
        Metacello new
        baseline: 'Azul';
        repository: 'github://yourname/azul-st';
        load.
        ```
*/

/*
    "Define where the library is if not standard"
    "AzulLibrary moduleName: '/path/to/libazul.so'."

    "Create Config"
    | config opts app |

    "Note: The generator created methods on the CLASS side of AzulLibrary"
    config := AzulLibrary azAppConfigNew.
    "Config is an instance of AzAppConfig (FFIExternalStructure)"

    "Set fields (auto-generated accessors)"
    "config logLevel: 1." "Assuming Enum constant"

    "Create Options"
    opts := AzulLibrary azWindowCreateOptionsNew: nil.

    "Create App"
    app := AzulLibrary azAppNew: nil appConfig: config.

    "Run"
    AzulLibrary azAppRun: app options: opts.
*/

/// Maps C types to Pharo/UnifiedFFI types
fn map_to_uffi_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string(); // char* -> String (auto-marshalled)
        }
        if ty.contains("void") {
            return "void *".to_string(); // Generic pointer
        }
        // Pointer to a known struct
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        if inner.starts_with(PREFIX) {
            return format!("{} *", inner);
        }
        return "void *".to_string();
    }

    match ty {
        "void" | "c_void" => "void".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "char" | "u8" | "i8" => "int8".to_string(), // Byte
        "u16" | "i16" | "AzU16" => "int16".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "int32".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" | "usize" | "size_t" => "int64".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        // Struct passed by value (UFFI supports this if the class is defined)
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "void *".to_string(),
    }
}

pub fn generate_smalltalk_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Define Package and Library Class
    // In chunk format, '!' separates chunks.
    
    code.push_str(&format!("Object subclass: #{}\n", LIB_CLASS));
    code.push_str("\tinstanceVariableNames: ''\n");
    code.push_str("\tclassVariableNames: ''\n");
    code.push_str("\tpoolDictionaries: ''\n");
    code.push_str("\tcategory: 'Azul-Core'!\n\n");

    // Library Name Method (mac/win/linux logic needed in real app, simplified here)
    code.push_str(&format!("!{} methodsFor: 'accessing' stamp: 'AutoGenerated'!\n", LIB_CLASS));
    code.push_str("macLibraryName\n");
    code.push_str(&format!("\t^ '{}.dylib'!\n\n", LIB_FILENAME));
    code.push_str("unixLibraryName\n");
    code.push_str(&format!("\t^ '{}.so'!\n\n", LIB_FILENAME));
    code.push_str("win32LibraryName\n");
    code.push_str(&format!("\t^ '{}.dll'!\n\n", LIB_FILENAME));
    code.push_str("!\n\n");

    // 2. Constants (SharedPool)
    // Smalltalk doesn't have Enums, we use a SharedPool for constants.
    code.push_str("SharedPool subclass: #AzulConstants\n");
    code.push_str("\tinstanceVariableNames: ''\n");
    code.push_str("\tclassVariableNames: '");
    
    // Gather all enum names for the classVar definition
    let mut var_names = Vec::new();
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            if let Some(enum_fields) = &class_data.enum_fields {
                 let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                 if is_simple {
                     for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            var_names.push(format!("{}_{}", full_name, variant_name));
                        }
                     }
                 }
            }
        }
    }
    code.push_str(&var_names.join(" "));
    code.push_str("'\n\tcategory: 'Azul-Core'!\n\n");

    // Initialize Constants
    code.push_str("!AzulConstants class methodsFor: 'initialization' stamp: 'AutoGenerated'!\n");
    code.push_str("initialize\n");
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("\t{}_{} := {}.\n", full_name, variant_name, idx));
                            idx += 1;
                        }
                    }
                }
            }
        }
    }
    code.push_str("!\n\n");
    code.push_str("AzulConstants initialize!\n\n"); // Run initialization immediately

    // 3. Structs (FFIExternalStructure)
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            // Check if it's a struct (has fields) or opaque (no fields)
            // Even opaque handles are useful as ExternalObjects, but usually strict structs need definition.
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("FFIExternalStructure subclass: #{}\n", full_name));
                code.push_str("\tinstanceVariableNames: ''\n");
                code.push_str("\tclassVariableNames: ''\n");
                code.push_str("\tpoolDictionaries: 'AzulConstants'\n");
                code.push_str("\tcategory: 'Azul-Structs'!\n\n");

                // fieldsDesc method
                code.push_str(&format!("!{} class methodsFor: 'field definition' stamp: 'AutoGenerated'!\n", full_name));
                code.push_str("fieldsDesc\n");
                code.push_str("\t^ #(\n");
                
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let st_type = map_to_uffi_type(&field_data.r#type);
                        code.push_str(&format!("\t\t{} {};\n", st_type, field_name));
                    }
                }
                
                code.push_str("\t)!\n\n");
            } else if class_data.is_boxed_object {
                // Opaque Object (Handle)
                // We define it as FFIExternalObject (generic pointer wrapper)
                code.push_str(&format!("FFIExternalObject subclass: #{}\n", full_name));
                code.push_str("\tinstanceVariableNames: ''\n");
                code.push_str("\tclassVariableNames: ''\n");
                code.push_str("\tpoolDictionaries: ''\n");
                code.push_str("\tcategory: 'Azul-Structs'!\n\n");
            }
        }
    }

    // 4. Methods (Bound on the AzulLibrary class side)
    // Smalltalk FFI calls are usually placed on the Library class or the Struct class.
    // Placing them on the Library class behaves like a C API namespace.
    
    code.push_str(&format!("!{} class methodsFor: 'api' stamp: 'AutoGenerated'!\n", LIB_CLASS));
    
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_ffi_call = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                
                // Smalltalk Selector Construction
                // azWindowNew: ptr with: opts
                let mut selector = String::new();
                let mut arg_defines = Vec::new(); // For the <ffiCall>
                
                // First part of selector
                selector.push_str(&snake_case_to_lower_camel(&format!("{}{}", class_name, fn_name)));

                let mut args = Vec::new();
                if !is_ctor {
                    args.push("self_ptr".to_string());
                    arg_defines.push(format!("{} * self_ptr", class_c_name));
                }

                for (i, arg_map) in fn_data.fn_args.iter().enumerate() {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        
                        // Add keyword to selector
                        if i == 0 && is_ctor {
                            selector.push_str(": ");
                        } else if i == 0 && !is_ctor {
                             selector.push_str(": ");
                        } else {
                             // Subsequent args get "with:" or just the name
                             selector.push_str(&format!(" {}: ", name));
                        }
                        
                        args.push(name.clone());
                        arg_defines.push(format!("{} {}", map_to_uffi_type(ty), name));
                    }
                }
                
                let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_uffi_type(&r.r#type));
                
                // Method signature
                if args.is_empty() {
                    code.push_str(&format!("{}\n", selector));
                } else {
                    // Combine selector parts with arg names
                    // This logic is slightly tricky because selector was built with colons.
                    // We need to interleave args.
                    // Simplified approach: just emit `methodName: arg1 name: arg2`
                    // Rebuilding selector loop:
                    let mut full_sig = String::new();
                    // Base name
                    let base = snake_case_to_lower_camel(&format!("{}{}", class_name, fn_name));
                    
                    if args.is_empty() {
                        full_sig = base;
                    } else {
                        full_sig.push_str(&base);
                        full_sig.push_str(": ");
                        full_sig.push_str(&args[0]);
                        
                        for k in 1..args.len() {
                            // Use arg name as keyword
                            full_sig.push_str(&format!(" {}: {}", args[k], args[k]));
                        }
                    }
                    code.push_str(&format!("{}\n", full_sig));
                }

                code.push_str(&format!("\t<ffiCall: {} {}({}) module: {}>^ self externalCallFailed!\n\n", 
                    ret_type, c_symbol, arg_defines.join(", "), LIB_CLASS));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_ffi_call(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_ffi_call(name, data, false); }
            }
            
             // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 let method_name = snake_case_to_lower_camel(&format!("{}_delete", class_name));
                 let c_symbol = format!("{}_delete", class_c_name);
                 code.push_str(&format!("{}: handle\n", method_name));
                 code.push_str(&format!("\t<ffiCall: void {}({} * handle) module: {}>^ self externalCallFailed!\n\n", 
                    c_symbol, class_c_name, LIB_CLASS));
            }
        }
    }
    code.push_str("!\n"); // End methods

    code
}
