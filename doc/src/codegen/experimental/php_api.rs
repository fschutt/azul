use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
For PHP, the modern and standard solution (since PHP 7.4) is PHP FFI 
(Foreign Function Interface). It allows you to load shared libraries 
(.dll/.so) and define C headers directly in PHP code, skipping the need 
to compile a custom PHP C-extension (PECL).

This generator produces a single Azul.php file.

1.  **Configuration**: Ensure `ffi.enable=true` (or `preload`) is set 
    in your `php.ini`.
2.  **Files**: Place `Azul.php` and `libazul.so`/`azul.dll` in your 
    project definition.

### Key PHP FFI Details

1.  **Preloading**: For production performance, you should use PHP's **OPcache Preloading** 
    to load the FFI definitions once at server startup, rather than parsing the `cdef` string 
    on every request.
2.  **Memory Management**:
    *   `FFI::new("Type")` creates C memory managed by PHP (garbage collected).
    *   Pointers returned from C (like `AzApp_new`) are **unmanaged**. PHP knows it's a pointer, 
        but won't `free()` it automatically. You generally rely on the C library's cleanup 
        functions (`AzApp_run` might consume/freed `app`, or you call `_delete`).
3.  **Struct Access**:
    *   `$cdata->field`: You can read/write struct fields just like PHP object properties.
    *   `$cdata->method()`: Not supported on raw FFI objects. The generator provides static 
        wrappers `Native::Method($cdata)`.

### Shipping (Composer)

Standard PHP package layout:

```text
azul-php/
├── src/
│   └── Azul.php
├── lib/
│   ├── libazul.so
│   └── azul.dll
└── composer.json
```
*/

/*

<?php
require_once "Azul.php";

use Azul\Native;
use Azul\AzAppConfig;
use Azul\AzAppLogLevel;

// Create Config
// Note: Constants are ints, FFI handles conversion automatically for C int args
$config = Native::AzAppConfig_new();
// Struct fields are accessible via FFI CData objects
$config->log_level = AzAppLogLevel::Debug;

// Create Options
$opts = Native::AzWindowCreateOptions_new(null);

// Create App
$app = Native::AzApp_new(null, $config);

// Run
Native::AzApp_run($app, $opts);

// PHP garbage collection cleans up the PHP objects.
// Note: FFI\CData objects returned by 'new' are NOT automatically freed using the C destructor 
// unless you wrap them in a custom class with a __destruct method calling AzApp_delete.
// For the raw API generated above, you must call delete manually if necessary:
// Native::AzApp_delete($app);

*/

// composer.json file
pub fn get_composer_json() -> String {
    format!("
{
    \"name\": \"yourname/azul-php\",
    \"description\": \"FFI bindings for Azul\",
    \"autoload\": {
        \"psr-4\": {
            \"Azul\\\": \"src/\"
        }
    },
    \"require\": {
        \"ext-ffi\": \"*\"
    }
}
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/// Maps to C types for the FFI::cdef string
fn map_to_c_def_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") { return "const char*".to_string(); }
        return "void*".to_string(); // Generic pointer for wrappers
    }
    match ty {
        "bool" | "GLboolean" => "uint8_t".to_string(),
        "u8" | "i8" => "char".to_string(), // PHP FFI handles char as byte
        "u16" | "i16" => "int16_t".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "int32_t".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" | "usize" | "isize" | "size_t" => "int64_t".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        "c_void" | "GLvoid" => "void".to_string(),
        s if s.starts_with(PREFIX) => s.to_string(), // Structs
        _ => "void*".to_string(),
    }
}

/// Maps to PHP DocBlock types for IDE autocompletion
fn map_to_php_doc_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") { return "string"; }
        return "\\FFI\\CData"; // Pointer object
    }
    match ty {
        "void" => "void",
        "bool" | "GLboolean" => "bool",
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "usize" | "isize" => "int",
        "f32" | "f64" => "float",
        s if s.starts_with(PREFIX) => "\\FFI\\CData", // Struct object
        _ => "mixed",
    }
}

pub fn generate_php_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. PHP Header and Class Definition
    code.push_str("<?php\n");
    code.push_str("/**\n");
    code.push_str(" * Auto-generated FFI bindings for Azul GUI\n");
    code.push_str(" * Requires 'ffi' extension in php.ini\n");
    code.push_str(" */\n");
    code.push_str("namespace Azul;\n\n");
    
    code.push_str("class Native {\n");
    code.push_str("    private static ?\\FFI $ffi = null;\n\n");
    
    // Library Loader
    code.push_str("    public static function getFFI(): \\FFI {\n");
    code.push_str("        if (self::$ffi === null) {\n");
    code.push_str("            $lib = self::getLibraryPath();\n");
    code.push_str("            self::$ffi = \\FFI::cdef(self::C_DEF, $lib);\n");
    code.push_str("        }\n");
    code.push_str("        return self::$ffi;\n");
    code.push_str("    }\n\n");

    code.push_str("    private static function getLibraryPath(): string {\n");
    code.push_str("        $os = PHP_OS_FAMILY;\n");
    code.push_str(&format!("        $name = \"{}\";\n", LIB_NAME));
    code.push_str("        switch ($os) {\n");
    code.push_str("            case 'Windows': return $name . \".dll\";\n");
    code.push_str("            case 'Darwin':  return \"lib\" . $name . \".dylib\";\n");
    code.push_str("            default:        return \"lib\" . $name . \".so\";\n");
    code.push_str("        }\n");
    code.push_str("    }\n\n");

    // 2. Build the C Definition String (heredoc)
    code.push_str("    private const C_DEF = <<<'CDEF'\n");
    code.push_str("    typedef void* AzHwndHandle;\n"); // Common typedefs
    
    // Forward Declarations
    for (_, module) in &version_data.api {
        for (class_name, _) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            code.push_str(&format!("    typedef struct {} {};\n", full_name, full_name));
        }
    }
    code.push_str("\n");

    // Struct Definitions (Must align with C layout for FFI to work)
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Structs
            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("    struct {} {{\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let c_type = map_to_c_def_type(&field_data.r#type);
                        code.push_str(&format!("        {} {};\n", c_type, field_name));
                    }
                }
                code.push_str("    };\n");
            }
            
            // Enums (Mapped as C enums)
            if let Some(enum_fields) = &class_data.enum_fields {
                 let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                 if is_simple {
                    code.push_str(&format!("    enum {} {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("        {}_{},\n", full_name, variant_name));
                        }
                    }
                    code.push_str("    };\n");
                 }
            }
        }
    }

    // Function Signatures
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_sig = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };

                let ret = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_c_def_type(&r.r#type));
                
                let mut args = Vec::new();
                if !is_ctor {
                    args.push(format!("{}* instance", class_c_name));
                }
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(format!("{} {}", map_to_c_def_type(ty), name));
                    }
                }
                
                code.push_str(&format!("    {} {}({});\n", ret, c_symbol, args.join(", ")));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_sig(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_sig(name, data, false); }
            }
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                code.push_str(&format!("    void {}_delete({}* instance);\n", class_c_name, class_c_name));
            }
        }
    }
    
    code.push_str("CDEF;\n\n");

    // 3. Static Wrapper Methods (For Autocomplete/Type Hinting)
    code.push_str("    // --- Wrapper Methods ---\n\n");
    
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_wrapper = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let php_method = c_symbol.clone(); // keep 1:1 naming or simplify
                let php_ret = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_php_doc_type(&r.r#type));

                let mut params = Vec::new();
                let mut call_args = Vec::new();
                
                if !is_ctor {
                    params.push(format!("\\FFI\\CData $instance"));
                    call_args.push("$instance".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let doc_type = map_to_php_doc_type(ty);
                        // In PHP arguments we don't strictly type hint CData usually because FFI handles it dynamically,
                        // but we can type hint primitives.
                        let type_hint = match doc_type.as_str() {
                            "int" | "float" | "bool" | "string" => format!("{} ", doc_type),
                            _ => "".to_string()
                        };
                        params.push(format!("{}${}", type_hint, name));
                        call_args.push(format!("${}", name));
                    }
                }

                code.push_str("    /**\n");
                code.push_str(&format!("     * @return {}\n", php_ret));
                code.push_str("     */\n");
                code.push_str(&format!("    public static function {}({}) {{\n", php_method, params.join(", ")));
                
                if php_ret == "void" {
                     code.push_str(&format!("        self::getFFI()->{}({});\n", c_symbol, call_args.join(", ")));
                } else {
                     code.push_str(&format!("        return self::getFFI()->{}({});\n", c_symbol, call_args.join(", ")));
                }
                code.push_str("    }\n\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_wrapper(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_wrapper(name, data, false); }
            }
        }
    }

    code.push_str("}\n\n"); // End Class Native

    // 4. Constants / Enums (PHP 8.1+ Enums or Classes with Consts)
    // We will use Classes with Constants for maximum compatibility (PHP 7.4+)
    for (_, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("class {} {{\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            code.push_str(&format!("    public const {} = {};\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("}\n\n");
                }
            }
        }
    }
    
    code // Return PHP file content
}
