use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
For **Perl**, the modern standard for interacting with shared libraries without writing 
C extensions (XS) is **FFI::Platypus**. It is robust, handles types gracefully, and 
supports "Records" (Structs) and memory management.

This generator produces a single `Azul.pm` module.


1.  **Dependencies**:

    ```bash
    cpanm FFI::Platypus FFI::CheckLib
    ```

2.  **Setup**: Place `Azul.pm` in a `lib/` directory. Place `libazul.so`/`azul.dll` in a 
    place where the system can find it (or modify the `find_lib` line in the generated code).


### Key Perl Details

1.  **`FFI::Platypus::Record`**: This handles C structs perfectly. It creates a Perl class where you can get/set fields, and when you pass the object to an FFI function expecting `record(Type)`, it passes the underlying C struct by value (or pointer if specified).
2.  **Opaque Pointers**: For objects where we don't expose fields (like the App handle), we use a scalar reference `bless \$ptr, 'ClassName'`. This is a standard Perl idiom for opaque handles.
3.  **Garbage Collection**: Perl uses reference counting. The `DESTROY` method is called deterministically when the refcount hits zero. This maps perfectly to calling C destructors (`_delete`).
4.  **`find_lib`**: `FFI::CheckLib` is smart. It looks in `LD_LIBRARY_PATH`, system paths, and the current directory (`alien` logic) to find the correct dynamic library extension for the OS.

*/

/*
    use lib 'lib';
    use Azul;

    # --- Structs (Pass by Value) ---
    # FFI::Platypus::Record creates a class for the struct
    # We can initialize it naturally
    my $config = AzAppConfig->new(
        # Fields are generated as accessors
        log_level => Azul::Enum::AzAppLogLevel_Debug()
    );

    # --- Constructors (Opaque Objects) ---
    # AzWindowCreateOptions is opaque in this context (assuming no fields exposed in api.json)
    # We pass 'undef' for null pointers
    my $opts = Azul::AzWindowCreateOptions->new(undef);

    # --- Create App ---
    my $app = Azul::AzApp->new(undef, $config);

    # --- Run ---
    $app->run($opts);

    # When $app goes out of scope, DESTROY calls AzApp_delete

*/

// Makefile.PL file
pub fn get_makefile_pl() -> String {
    format!("
use ExtUtils::MakeMaker;

WriteMakefile(
    NAME         => 'Azul',
    VERSION      => '1.0.0',
    PREREQ_PM    => {
        'FFI::Platypus' => '2.00',
        'FFI::CheckLib' => '0.28',
    },
    # You might want to copy the DLL to blib/arch/auto/Azul/ here
);
    ")
}

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/// Maps C types to FFI::Platypus types
fn map_to_perl_ffi_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return "string".to_string(); // Auto-marshals char* <-> scalar string
        }
        // Generic pointer or pointer to struct (Opaque handle)
        return "opaque".to_string();
    }

    match ty {
        "void" | "c_void" => "void".to_string(),
        "bool" | "GLboolean" => "uint8".to_string(), // Perl uses integers for bools usually
        "char" | "i8" => "sint8".to_string(),
        "u8" => "uint8".to_string(),
        "i16" => "sint16".to_string(),
        "u16" | "AzU16" => "uint16".to_string(),
        "i32" | "GLint" | "GLsizei" => "sint32".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "uint32".to_string(),
        "i64" | "GLint64" | "isize" | "ssize_t" | "intptr_t" => "sint64".to_string(),
        "u64" | "GLuint64" | "usize" | "size_t" | "uintptr_t" => "uint64".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        // Struct by Value: FFI::Platypus maps this to record(Name)
        s if s.starts_with(PREFIX) => format!("record({})", s),
        _ => "opaque".to_string(),
    }
}

pub fn generate_perl_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("package Azul;\n");
    code.push_str("use strict;\n");
    code.push_str("use warnings;\n");
    code.push_str("use FFI::Platypus 2.00;\n");
    code.push_str("use FFI::CheckLib;\n\n");
    
    code.push_str("# Initialize FFI\n");
    code.push_str("my $ffi = FFI::Platypus->new(api => 2);\n");
    code.push_str(&format!("$ffi->lib(find_lib_or_die lib => '{}', symbol => '{}_app_new');\n\n", LIB_NAME, PREFIX.to_lowercase()));

    // 2. Struct Definitions (Records)
    // FFI::Platypus needs layout definitions for pass-by-value structs
    code.push_str("# --- Struct Definitions ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(struct_fields) = &class_data.struct_fields {
                // Define the package for the struct
                code.push_str(&format!("package {};\n", full_name));
                code.push_str("use FFI::Platypus::Record;\n");
                
                // Define layout
                code.push_str("record_layout_1($Azul::ffi,\n");
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let perl_type = map_to_perl_ffi_type(&field_data.r#type);
                        // FFI::Platypus record syntax: 'type' => 'name'
                        code.push_str(&format!("    '{}' => '{}',\n", perl_type, field_name));
                    }
                }
                code.push_str(");\n");
                // Register the type alias so 'record(AzRect)' works
                code.push_str(&format!("$Azul::ffi->type('record({})' => '{}');\n\n", full_name, full_name));
            }
        }
    }

    // 3. Enums (Constants)
    code.push_str("package Azul::Enum;\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("# Enum {}\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Define constant: Azul::Enum::AzWindowTheme_Dark()
                            code.push_str(&format!("sub {}_{} {{ {} }}\n", full_name, variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    // 4. Attach Raw Functions
    code.push_str("package Azul::API;\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut attach_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, suffix)
                } else {
                     format!("{}_{}", class_c_name, suffix)
                };
                
                let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_perl_ffi_type(&r.r#type));
                
                let mut args = Vec::new();
                if !is_ctor {
                    args.push("'opaque'".to_string()); // Self pointer
                }
                
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(format!("'{}'", map_to_perl_ffi_type(ty)));
                    }
                }
                
                let args_str = format!("[{}]", args.join(", "));
                code.push_str(&format!("$Azul::ffi->attach('{}' => {} => '{}');\n", c_symbol, args_str, ret_type));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { attach_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { attach_func(name, data, false); }
            }
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                let c_symbol = format!("{}_delete", class_c_name);
                code.push_str(&format!("$Azul::ffi->attach('{}' => ['opaque'] => 'void');\n", c_symbol));
            }
        }
    }
    code.push_str("\n");

    // 5. Object Oriented Wrappers
    // We create a Perl package for every Opaque Struct (Handle) to provide new(), methods, and DESTROY
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            // Only generate wrappers for Opaque handles or types with destructors.
            // Pure value structs are handled by FFI::Platypus::Record above.
            let full_name = format!("{}{}", PREFIX, class_name);
            let has_struct_fields = class_data.struct_fields.is_some();
            let has_dtor = class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object;
            
            // If it has struct fields, it's already a Perl Class via FFI::Platypus::Record. 
            // We can extend it, but usually Record classes are just data.
            // If it's opaque (no fields), we generate a wrapper class holding the pointer.
            if !has_struct_fields {
                code.push_str(&format!("package Azul::{};\n", full_name));
                
                // Constructors
                if let Some(ctors) = &class_data.constructors {
                    for (fn_name, fn_data) in ctors {
                        let suffix = snake_case_to_lower_camel(fn_name); // new or create
                        let c_symbol = format!("{}_{}", full_name, suffix);
                        let perl_method_name = if fn_name == "new" { "new" } else { suffix };

                        code.push_str(&format!("sub {} {{\n", perl_method_name));
                        code.push_str("    my ($class, @args) = @_;\n");
                        code.push_str(&format!("    my $ptr = Azul::API::{}(@args);\n", c_symbol));
                        code.push_str("    return bless \\$ptr, $class;\n");
                        code.push_str("}\n");
                    }
                }

                // Methods
                if let Some(fns) = &class_data.functions {
                    for (fn_name, fn_data) in fns {
                         let suffix = snake_case_to_lower_camel(fn_name);
                         let c_symbol = format!("{}_{}", full_name, suffix);
                         
                         code.push_str(&format!("sub {} {{\n", suffix));
                         code.push_str("    my ($self, @args) = @_;\n");
                         // Dereference scalar ref to get opaque pointer
                         code.push_str(&format!("    return Azul::API::{}($$self, @args);\n", c_symbol));
                         code.push_str("}\n");
                    }
                }

                // Destructor
                if has_dtor {
                    code.push_str("sub DESTROY {\n");
                    code.push_str("    my ($self) = @_;\n");
                    code.push_str(&format!("    Azul::API::{}_delete($$self);\n", full_name));
                    code.push_str("}\n");
                }
                
                code.push_str("\n");
            }
        }
    }

    // End main package
    code.push_str("package Azul;\n");
    code.push_str("1;\n");
    
    code
}
