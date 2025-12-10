use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

// SBT (Simple Build Tool) is the standard build system for Scala.

azul-scala/
├── build.sbt
├── project/
│   └── build.properties
└── src/
    ├── main/
    │   ├── scala/
    │   │   └── rs/
    │   │       └── azul/
    │   │           └── Azul.scala    <-- Generated file
    │   └── resources/
    │       ├── linux-x86-64/
    │       │   └── libazul.so
    │       ├── win32-x86-64/
    │       │   └── azul.dll
    │       └── darwin/
    │           └── libazul.dylib
*/

/*

### Key Scala Specifics

1.  **`var` vs `val`**: JNA requires fields to be mutable so it can write data into them from C. We generate `var`.
2.  **`Arrays.asList`**: Scala Lists are not Java Lists. JNA's `getFieldOrder` expects `java.util.List`. We use `Arrays.asList` directly to avoid implicit conversion overhead/imports issues.
3.  **Companion Objects**: In Java, we put `ByValue` / `ByReference` static classes inside the class. In Scala, we put them in the **Companion Object** to keep the namespace clean and accessible (e.g., `new AzRect.ByValue()`).
4.  **`trait AzulLibrary`**: Scala traits interface perfectly with JNA's `Library` interface.

### Compiling

```bash
sbt compile
sbt package
```

This produces a jar file that can be distributed.

*/

/*
package example

import rs.azul._
import com.sun.jna.Pointer

object Main extends App {
  // Create Config
  // Since Azul structs in Scala are JNA Structures, we can instantiate them directly
  // Note: For C pointers passed by value, use AzAppConfig.ByValue
  val config = new AzAppConfig()
  config.log_level = AzAppLogLevel.Debug // Accessing object constant
  
  // Call Rust (C) Constructor
  // If function returns Pointer
  val optionsPtr: Pointer = Azul.Native.AzWindowCreateOptions_new(null)
  
  // Create App
  val appPtr: Pointer = Azul.Native.AzApp_new(null, config)
  
  // Run
  Azul.Native.AzApp_run(appPtr, optionsPtr)
}
*/

// build.sbt
pub fn get_build_sbt() -> String {
    format!("
name := \"azul-scala\"
version := \"1.0.0\"
scalaVersion := \"3.3.1\" // Or 2.13.12

// JNA Dependency
libraryDependencies += \"net.java.dev.jna\" % \"jna\" % \"5.13.0\"
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIBRARY_NAME: &str = "azul";

/// Maps C/Rust types to Scala JNA types
fn map_to_scala_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string();
        }
        return "Pointer".to_string();
    }
    
    match ty {
        "void" | "c_void" | "GLvoid" => "Unit".to_string(),
        "bool" | "GLboolean" => "Boolean".to_string(),
        "char" | "u8" | "i8" => "Byte".to_string(),
        "u16" | "i16" => "Short".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "Int".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" => "Long".to_string(),
        "f32" | "GLfloat" | "AzF32" => "Float".to_string(),
        "f64" | "GLdouble" => "Double".to_string(),
        "usize" | "size_t" => "Long".to_string(), // Scala Int is 32-bit, use Long for size_t
        // Structs/Enums
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "Pointer".to_string(),
    }
}

/// Helper for struct field initialization
fn get_scala_default_value(ty: &str) -> String {
    match ty {
        "Boolean" => "false".to_string(),
        "Byte" => "0".to_string(),
        "Short" => "0".to_string(),
        "Int" => "0".to_string(),
        "Long" => "0L".to_string(),
        "Float" => "0.0f".to_string(),
        "Double" => "0.0".to_string(),
        "String" => "\"\"".to_string(), 
        "Unit" => "()".to_string(),
        _ => "null".to_string(), 
    }
}

pub fn generate_scala_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header & Package
    code.push_str("package rs.azul\n\n");
    code.push_str("import com.sun.jna._\n");
    code.push_str("import com.sun.jna.ptr._\n");
    code.push_str("import java.util.Arrays\n");
    code.push_str("import java.util.List\n\n");
    
    // 2. Library Interface
    code.push_str("trait AzulLibrary extends Library {\n");
    
    // Functions
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut generate_fn = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let ret_raw = if let Some(ret) = &fn_data.returns {
                     map_to_scala_type(&ret.r#type)
                } else {
                    "Unit".to_string()
                };

                // JNA Void
                let ret_type = if ret_raw == "Unit" { "void" } else { &ret_raw };

                let mut args_str = Vec::new();
                if !is_ctor {
                    args_str.push("instance: Pointer".to_string());
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args_str.push(format!("{}: {}", name, map_to_scala_type(ty)));
                    }
                }

                code.push_str(&format!("  def {}({}): {}\n", c_symbol, args_str.join(", "), ret_type));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_fn(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_fn(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 code.push_str(&format!("  def {}_delete(instance: Pointer): Unit\n", class_c_name));
            }
        }
    }
    code.push_str("}\n\n");

    // 3. Singleton Loader
    code.push_str("object Azul {\n");
    code.push_str(&format!("  val Native: AzulLibrary = Native.load(\"{}\", classOf[AzulLibrary])\n", LIBRARY_NAME));
    code.push_str("}\n\n");

    // 4. Structs & Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Enums (Use Objects with Constants for C interop)
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    code.push_str(&format!("object {} {{\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("  val {}: Int = {}\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("}\n\n");
                }
            }

            // Structures
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("class {} extends Structure {{\n", full_name));
                
                // Fields
                let mut field_names = Vec::new();
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let sc_type = map_to_scala_type(&field_data.r#type);
                        let default_val = get_scala_default_value(&sc_type);
                        
                        // Handle ByValue structs
                        let final_type = if sc_type.starts_with(PREFIX) {
                            // If it's another struct, we define it as that struct type.
                            // JNA handles mapping if it's a Structure subclass.
                            sc_type
                        } else {
                            sc_type
                        };

                        code.push_str(&format!("  var {}: {} = {}\n", field_name, final_type, default_val));
                        field_names.push(format!("\"{}\"", field_name));
                    }
                }
                
                // Field Order (Must return java.util.List)
                code.push_str("\n  override protected def getFieldOrder(): List[String] = {\n");
                code.push_str(&format!("    Arrays.asList({})\n", field_names.join(", ")));
                code.push_str("  }\n");
                
                // Inner traits for ByValue / ByReference
                code.push_str("}\n"); // End Class

                // Companion object for ByValue/ByRef types
                code.push_str(&format!("object {} {{\n", full_name));
                code.push_str(&format!("  class ByValue extends {} with Structure.ByValue\n", full_name));
                code.push_str(&format!("  class ByReference extends {} with Structure.ByReference\n", full_name));
                code.push_str("}\n\n");
            }
        }
    }

    code
}
