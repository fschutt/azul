use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";

/*
For **Fortran**, the modern standard (Fortran 2003+) includes the `iso_c_binding` intrinsic 
module. This provides standardized, compiler-independent interoperability with C.

This generator produces a single `azul.f90` module file.

### Usage in Fortran

Save the output as `azul.f90`. Compile your Rust library to `libazul.so` / `azul.dll`.

### Compilation

You need to link the Rust library.

```bash
# 1. Compile module
gfortran -c azul.f90

# 2. Compile and link main
gfortran main.f90 azul.o -L. -lazul -o azul_app

# 3. Run
./azul_app
```

### Key Fortran Nuances

1.  **`bind(c)`**: This is essential. It tells the compiler to use C ABI conventions 
    (no name mangling like `module_MP_func`, no hidden length arguments for strings 
    unless specified).
2.  **`value` attribute**: In standard Fortran, everything is pass-by-reference. In 
    C, primitives are pass-by-value. You must add the `, value` attribute to arguments 
    in the interface definition, or C will receive a pointer to the integer instead of 
    the integer itself, causing segfaults or garbage data.
3.  **`type(c_ptr)`**: This is the universal `void*`.
4.  **Derived Types**: If Rust returns a struct by value (e.g., `AzRect`), you must 
    define the `type, bind(c) :: AzRect` in Fortran matching the memory layout. The 
    generator handles this.

*/

/*

    program main
    use iso_c_binding
    use azul
    implicit none

    type(c_ptr) :: config
    type(c_ptr) :: opts
    type(c_ptr) :: app
    
    ! Create Config
    config = AzAppConfig_new()
    
    ! Create Options (Passing C_NULL_PTR for null)
    opts = AzWindowCreateOptions_new(c_null_ptr)
    
    ! Create App
    app = AzApp_new(c_null_ptr, config)
    
    ! Run
    call AzApp_run(app, opts)
    
    ! Cleanup
    ! call AzApp_delete(app) 
    end program main

*/

/// Maps C types to Fortran iso_c_binding types
fn map_to_fortran_type(ty: &str) -> String {
    // Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            return "type(c_ptr)".to_string(); // C Strings are pointers
        }
        // Generic or Typed pointer
        return "type(c_ptr)".to_string();
    }

    match ty {
        "void" | "c_void" | "GLvoid" => "type(c_ptr)".to_string(), // Usually handled by Subroutine vs Function logic
        "bool" | "GLboolean" => "logical(c_bool)".to_string(),
        "char" | "u8" | "i8" => "integer(c_int8_t)".to_string(),
        "u16" | "i16" => "integer(c_int16_t)".to_string(),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => "integer(c_int32_t)".to_string(),
        "u64" | "i64" | "GLuint64" | "GLint64" => "integer(c_int64_t)".to_string(),
        "f32" | "GLfloat" | "AzF32" => "real(c_float)".to_string(),
        "f64" | "GLdouble" => "real(c_double)".to_string(),
        "usize" | "size_t" => "integer(c_size_t)".to_string(),
        "isize" | "ssize_t" => "integer(c_intptr_t)".to_string(),
        // Derived types (Structs)
        // In Fortran, if it's passed by value, we need the derived type definition.
        s if s.starts_with(PREFIX) => format!("type({})", s),
        _ => "type(c_ptr)".to_string(),
    }
}

pub fn generate_fortran_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Module Header
    code.push_str("module azul\n");
    code.push_str("  use iso_c_binding\n");
    code.push_str("  implicit none\n\n");

    // 2. Constants / Enums
    // Fortran doesn't have C-style Enums. We use integer constants.
    code.push_str("  ! --- Enums ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    code.push_str(&format!("  ! Enum {}\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Fortran is case insensitive, so AzApp_Debug is same as azapp_debug.
                            // We define constants.
                            code.push_str(&format!("  integer(c_int), parameter :: {}_{} = {}\n", 
                                full_name, variant_name, idx));
                            idx += 1;
                        }
                    }
                    code.push_str("\n");
                }
            }
        }
    }

    // 3. Derived Types (Structs)
    code.push_str("  ! --- Structs ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("  type, bind(c) :: {}\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let f_type = map_to_fortran_type(&field_data.r#type);
                        code.push_str(&format!("    {} :: {}\n", f_type, field_name));
                    }
                }
                code.push_str("  end type\n\n");
            }
        }
    }

    // 4. Interfaces (C Functions)
    code.push_str("  interface\n\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };

                let ret_raw = if let Some(ret) = &fn_data.returns {
                     map_to_fortran_type(&ret.r#type)
                } else {
                    "void".to_string()
                };

                let is_subroutine = ret_raw == "void";

                if is_subroutine {
                    code.push_str(&format!("    subroutine {}(", c_symbol));
                } else {
                    code.push_str(&format!("    function {}(", c_symbol));
                }

                // Arguments
                let mut args = Vec::new();
                let mut arg_defs = Vec::new();

                if !is_ctor {
                    args.push("instance".to_string());
                    // C pointers passed to C must be by VALUE in Fortran interface
                    arg_defs.push("      type(c_ptr), value :: instance".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(name.clone());
                        
                        let f_ty = map_to_fortran_type(ty);
                        
                        // Rule: If it's a C primitive or pointer, it must be VALUE.
                        // If it's a struct passed by value in C, Fortran bind(c) handles it,
                        // but usually large structs are passed by pointer.
                        // Assuming primitives/pointers here:
                        arg_defs.push(format!("      {}, value :: {}", f_ty, name));
                    }
                }

                code.push_str(&args.join(", "));
                code.push_str(&format!(") bind(c, name=\"{}\")\n", c_symbol));
                
                code.push_str("      import\n"); // Access types defined in module
                for def in arg_defs {
                    code.push_str(&format!("{}\n", def));
                }

                if !is_subroutine {
                    code.push_str(&format!("      {} :: {}\n", ret_raw, c_symbol));
                    code.push_str(&format!("    end function {}\n", c_symbol));
                } else {
                    code.push_str(&format!("    end subroutine {}\n", c_symbol));
                }
                code.push_str("\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_func(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 let dtor_sym = format!("{}_delete", class_c_name);
                 code.push_str(&format!("    subroutine {}(instance) bind(c, name=\"{}\")\n", dtor_sym, dtor_sym));
                 code.push_str("      import\n");
                 code.push_str("      type(c_ptr), value :: instance\n");
                 code.push_str(&format!("    end subroutine {}\n\n", dtor_sym));
            }
        }
    }

    code.push_str("  end interface\n");
    code.push_str("end module azul\n");
    
    code
}
