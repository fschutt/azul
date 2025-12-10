use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
azul-kt/
├── build.gradle.kts
├── settings.gradle.kts
└── src/
    ├── main/
    │   ├── kotlin/
    │   │   └── rs/
    │   │       └── azul/
    │   │           └── Azul.kt    <-- Generated file
    │   └── resources/
    │       ├── linux-x86-64/
    │       │   └── libazul.so
    │       ├── win32-x86-64/
    │       │   └── azul.dll
    │       └── darwin/
    │           └── libazul.dylib

*/

/*

### Key Kotlin Specifics

1.  **`@JvmField`**: This is the most critical part for JNA structs in Kotlin. 
    Without it, JNA sees `private width` and `public getWidth()`, but it expects a 
    public field named `width`. `@JvmField` exposes the field directly to the JVM 
    bytecode as a public field.
2.  **`open class`**: Kotlin classes are `final` by default. JNA requires inheritance 
    for `Structure` and the `ByValue`/`ByReference` tagging interfaces.
3.  **Null Safety**: The generator uses `Pointer?` for C pointers. This forces the user to 
    handle potential nulls returned from C, which is safer than assuming non-null.
4.  **Singletons**: Using `object` for the Library loader and Enums provides a very clean, 
    idiomatic API (`Azul.Native.function()`).

### Shipping

You compile this into a `.jar`.

```bash
./gradlew jar
```

The resulting JAR will contain your compiled Kotlin classes and the embedded Rust shared 
libraries (if you placed them in `src/main/resources`). When a user adds this JAR to their project, 
JNA automatically extracts and loads the correct Rust binary.

*/

/*

import rs.azul.*
import com.sun.jna.Pointer

fun main() {
    // Initialize Config (Struct passed by pointer usually, so simple class is fine)
    val config = AzAppConfig()
    config.log_level = AzAppLogLevel.Debug // Accessing object constant
    
    // If Rust expects ByValue, pass AzAppConfig.ByValue()
    // But destructors usually take Pointer.
    
    // Create struct options
    // Assuming AzWindowCreateOptions_new returns a Pointer
    val optsPtr: Pointer? = Azul.Native.AzWindowCreateOptions_new(null)
    
    // Create App
    val appPtr: Pointer? = Azul.Native.AzApp_new(null, config)
    
    // Run
    Azul.Native.AzApp_run(appPtr, optsPtr)
}

*/
// build.gradle.kts file
pub fn get_build_gradle_kts() -> String {
    format!("
plugins {
    kotlin(\"jvm\") version \"1.9.0\"
}

group = \"rs.azul\"
version = \"1.0.0\"

repositories {
    mavenCentral()
}

dependencies {
    implementation(kotlin(\"stdlib\"))
    // JNA Dependency
    implementation(\"net.java.dev.jna:jna:5.13.0\")
}
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIBRARY_NAME: &str = "azul";

/// Maps C/Rust types to Kotlin JNA types
fn map_to_kotlin_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string(); // JNA marshals String <-> char*
        }
        return "Pointer".to_string();
    }
    
    match ty {
        "void" | "c_void" | "GLvoid" => "Unit".to_string(), // Return type
        "bool" | "GLboolean" => "Boolean".to_string(),
        "char" | "u8" | "i8" => "Byte".to_string(),
        "u16" | "i16" => "Short".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "Int".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" => "Long".to_string(),
        "f32" | "GLfloat" | "AzF32" => "Float".to_string(),
        "f64" | "GLdouble" => "Double".to_string(),
        "usize" | "size_t" => "Long".to_string(), // JVM has no unsigned, use Long
        // Structs/Enums
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "Pointer".to_string(),
    }
}

/// Helper for struct fields (Kotlin needs specific initialization)
fn get_kotlin_default_value(ty: &str) -> String {
    match ty {
        "Boolean" => "false".to_string(),
        "Byte" => "0".to_string(),
        "Short" => "0".to_string(),
        "Int" => "0".to_string(),
        "Long" => "0L".to_string(),
        "Float" => "0.0f".to_string(),
        "Double" => "0.0".to_string(),
        "String" => "\"\"".to_string(), // Or null, but empty is safer for initializers
        _ => "null".to_string(), // Pointers/Structs
    }
}

pub fn generate_kotlin_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header & Package
    code.push_str("package rs.azul\n\n");
    code.push_str("import com.sun.jna.*\n");
    code.push_str("import com.sun.jna.ptr.*\n");
    code.push_str("import java.util.Arrays\n");
    code.push_str("import java.util.List\n\n");
    
    // 2. Main Interface
    code.push_str("interface AzulLib : Library {\n");
    
    // Functions
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helpers to generate method sigs
            let mut generate_fn = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let ret_raw = if let Some(ret) = &fn_data.returns {
                     map_to_kotlin_type(&ret.r#type)
                } else {
                    "Unit".to_string()
                };

                // JNA interface methods usually return void, not Unit
                let ret_type = if ret_raw == "Unit" { "void" } else { &ret_raw };

                let mut args_str = Vec::new();
                if !is_ctor {
                    args_str.push("instance: Pointer?".to_string());
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let kt_type = map_to_kotlin_type(ty);
                        // Nullable pointers for safety
                        let final_type = if kt_type == "Pointer" || kt_type.starts_with(PREFIX) { 
                            format!("{}?", kt_type) 
                        } else { 
                            kt_type 
                        };
                        args_str.push(format!("{}: {}", name, final_type));
                    }
                }

                code.push_str(&format!("    fun {}({}): {}\n", c_symbol, args_str.join(", "), ret_type));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_fn(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_fn(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 code.push_str(&format!("    fun {}_delete(instance: Pointer?)\n", class_c_name));
            }
        }
    }
    code.push_str("}\n\n");

    // 3. Singleton Loader
    code.push_str("object Azul {\n");
    code.push_str(&format!("    val Native: AzulLib = Native.load(\"{}\", AzulLib::class.java)\n", LIBRARY_NAME));
    code.push_str("}\n\n");

    // 4. Structs & Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Generate Enums (as Objects with consts, safer for C interop than enum classes)
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    code.push_str(&format!("object {} {{\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    const val {}: Int = {}\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("}\n\n");
                }
            }

            // Generate Structures
            if let Some(struct_fields) = &class_data.struct_fields {
                // Must be 'open' to allow ByValue/ByReference subclasses
                code.push_str(&format!("open class {} : Structure() {{\n", full_name));
                
                // Fields
                // Crucial: Use @JvmField so JNA can write to the field directly via reflection.
                // Kotlin properties are private+get/set by default, which confuses JNA struct mapping.
                let mut field_names = Vec::new();
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let kt_type = map_to_kotlin_type(&field_data.r#type);
                        let default_val = get_kotlin_default_value(&kt_type);
                        
                        // Handle ByValue vs Pointer
                        // Similar to Java logic: primitives are fine, structs need nesting
                        let final_type = if kt_type.starts_with(PREFIX) {
                            // In struct definition, if it's a value type, it should technically be initialized
                            // For simplicity in this generator, we use the type name, user handles instantiation
                            kt_type
                        } else {
                            kt_type
                        };

                        // Use nullable for non-primitives to allow null init
                        let type_sig = if default_val == "null" { 
                            format!("{}?", final_type) 
                        } else { 
                            final_type 
                        };

                        code.push_str(&format!("    @JvmField var {}: {} = {}\n", field_name, type_sig, default_val));
                        field_names.push(format!("\"{}\"", field_name));
                    }
                }
                
                // Field Order (Required by JNA)
                code.push_str("\n    override fun getFieldOrder(): List<String> {\n");
                code.push_str(&format!("        return listOf({})\n", field_names.join(", ")));
                code.push_str("    }\n");
                
                // Inner classes for ByValue / ByReference passing
                code.push_str(&format!("    class ByValue : {}(), Structure.ByValue\n", full_name));
                code.push_str(&format!("    class ByReference : {}(), Structure.ByReference\n", full_name));
                
                code.push_str("}\n\n");
            }
        }
    }

    code
}