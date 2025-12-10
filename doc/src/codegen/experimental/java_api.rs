use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};


/*

azul-java/
├── pom.xml
└── src/
    ├── main/
    │   ├── java/
    │   │   └── rs/
    │   │       └── azul/
    │   │           └── Azul.java  <-- Generated file
    │   └── resources/
    │       ├── linux-x86-64/
    │       │   └── libazul.so     <-- Rust binary (Linux)
    │       ├── win32-x86-64/
    │       │   └── azul.dll       <-- Rust binary (Windows)
    │       └── darwin/
    │           └── libazul.dylib  <-- Rust binary (Mac)

*/

/*

### Key Java Nuances

1.  **Structs By Value vs Reference**:
    *   In C: `void func(AzRect rect)` (Value) vs `void func(AzRect* rect)` (Pointer).
    *   In JNA: You must implement `Structure.ByValue` marker interface for 
        Values, and `Structure.ByReference` (or just `Structure`) for pointers.
    *   *Generator logic update*: The generator provided adds `public static class 
        ByValue ...` inside every struct. If the API definition says the argument is 
        a Value (`RefKind::Value`), map the type to `AzRect.ByValue` in the function 
        signature. If it's a pointer, map to `AzRect`.

2.  **Memory Management**:
    *   Java GC handles the Java objects, but not the C memory they point to.
    *   You often need to wrap the `Pointer` returned by `_new` functions in a Java class 
        that implements `AutoCloseable` and calls `_delete` in `close()`.

3.  **Callbacks**:
    *   If your Rust API takes callbacks, you need to generate a Java interface extending `Callback`.
    *   *Crucial*: You **must** keep a strong reference to the callback object in Java as long 
        as Rust might call it. If the Java GC collects the callback object, the C function pointer 
        passed to Rust becomes invalid, causing a segfault.
*/

/*

// Usage in Java:
// 
// JNA automatically extracts the DLL from src/main/resources if you conform 
// to its path standards (e.g., linux-x86-64).


import rs.azul.Azul;
import com.sun.jna.Pointer;

public class Main {
    public static void main(String[] args) {
        // Create options
        // Note: For actual structs passed by value, use new Azul.AzWindowCreateOptions.ByValue()
        Pointer options = Azul.NativeLib.INSTANCE.AzWindowCreateOptions_new(null);
        
        // Configuration
        Azul.AzAppConfig config = new Azul.AzAppConfig();
        config.log_level = Azul.AzAppLogLevel.Debug;
        
        // Create App
        Pointer app = Azul.NativeLib.INSTANCE.AzApp_new(null, config);
        
        // Run
        Azul.NativeLib.INSTANCE.AzApp_run(app, options);
    }
}

*/


// For Maven
pub fn get_pom_xml() -> String {
    format!("
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>rs.azul</groupId>
    <artifactId>azul</artifactId>
    <version>1.0.0</version>

    <dependencies>
        <!-- The JNA Dependency -->
        <dependency>
            <groupId>net.java.dev.jna</groupId>
            <artifactId>jna</artifactId>
            <version>5.13.0</version>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <!-- Compiler -->
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.8.1</version>
                <configuration>
                    <source>1.8</source>
                    <target>1.8</target>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIBRARY_NAME: &str = "azul";

/// Maps C/Rust types to JNA Java types
fn map_to_java_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string(); // JNA automatically converts String <-> char*
        }
        return "Pointer".to_string();
    }
    
    match ty {
        "void" | "c_void" | "GLvoid" => "void".to_string(),
        "bool" | "GLboolean" => "boolean".to_string(),
        "char" | "u8" | "i8" => "byte".to_string(),
        "u16" | "i16" => "short".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "int".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" => "long".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        "usize" | "size_t" => "long".to_string(), // Java doesn't have unsigned, use long for size_t
        // If it starts with Az, it's a struct or enum.
        // In JNA, structs passed by value MUST be the class type.
        // Enums are usually just ints in C bindings.
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "Pointer".to_string(),
    }
}

pub fn generate_java_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header & Package
    code.push_str("package rs.azul;\n\n");
    code.push_str("import com.sun.jna.*;\n");
    code.push_str("import com.sun.jna.ptr.*;\n");
    code.push_str("import java.util.Arrays;\n");
    code.push_str("import java.util.List;\n\n");
    
    code.push_str("/** Auto-generated JNA bindings for Azul */\n");
    code.push_str("public class Azul {\n");
    
    // 2. Load Library
    code.push_str("    public interface NativeLib extends Library {\n");
    code.push_str(&format!("        NativeLib INSTANCE = Native.load(\"{}\", NativeLib.class);\n\n", LIBRARY_NAME));
    
    // 3. Functions (inside Interface)
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helper to generate method sigs
            let mut generate_fn = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                // Java method name
                let java_name = if is_ctor {
                    // e.g. AzDom_new
                    c_symbol.clone() 
                } else {
                    // e.g. AzDom_appendChild
                    c_symbol.clone()
                };

                let ret_type = if let Some(ret) = &fn_data.returns {
                     map_to_java_type(&ret.r#type)
                } else {
                    "void".to_string()
                };

                let mut args_str = Vec::new();
                if !is_ctor {
                    args_str.push(format!("Pointer instance"));
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args_str.push(format!("{} {}", map_to_java_type(ty), name));
                    }
                }

                code.push_str(&format!("        {} {}({});\n", ret_type, java_name, args_str.join(", ")));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { generate_fn(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { generate_fn(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 code.push_str(&format!("        void {}_delete(Pointer instance);\n", class_c_name));
            }
        }
    }
    code.push_str("    }\n\n"); // End Interface

    // 4. Structs & Enums (Static Inner Classes)
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Generate Enums (as static classes with constants)
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    code.push_str(&format!("    public static class {} {{\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("        public static final int {} = {};\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("    }\n\n");
                }
            }

            // Generate Structures
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("    public static class {} extends Structure {{\n", full_name));
                
                // Fields
                let mut field_names = Vec::new();
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let java_type = map_to_java_type(&field_data.r#type);
                        
                        // Handle ByValue structs
                        if java_type.starts_with(PREFIX) {
                            code.push_str(&format!("        public {} {};\n", java_type, field_name));
                        } else {
                            code.push_str(&format!("        public {} {};\n", java_type, field_name));
                        }
                        field_names.push(format!("\"{}\"", field_name));
                    }
                }
                
                // JNA FieldOrder annotation (Required)
                code.push_str("\n        @Override\n");
                code.push_str("        protected List<String> getFieldOrder() {\n");
                code.push_str(&format!("            return Arrays.asList({});\n", field_names.join(", ")));
                code.push_str("        }\n");
                
                // ByValue inner class (required for passing structs by value)
                code.push_str(&format!("        public static class ByValue extends {} implements Structure.ByValue {{}}\n", full_name));
                code.push_str(&format!("        public static class ByReference extends {} implements Structure.ByReference {{}}\n", full_name));
                
                code.push_str("    }\n\n");
            }
        }
    }

    code.push_str("}\n");
    code
}
