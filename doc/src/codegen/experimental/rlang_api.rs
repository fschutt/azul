use std::collections::HashMap;
use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

For **R**, the standard approach for high-performance libraries is to create 
an **R Package** that contains **C Glue Code**.

R does not have a built-in FFI that reads C headers (like LuaJIT or PHP). Instead, 
R uses the `.Call` interface, which passes S-Expressions (`SEXP`) between R and C.

This generator produces two main files:

1.  `src/azul_wrapper.c`: The C glue layer that translates R `SEXP` objects to C 
    types (`int`, `float`, pointers) and calls your `azul.h` functions.
2.  `R/Azul.R`: The R functions that wrap the `.Call` invocations.

In R, when you install a package with a `src/` directory, R calls the system compiler 
(GCC/Clang) to compile `.c` files into a shared object (`azul.so` / `azul.dll`) that 
R can load.

You need to tell the R compiler where `azul.h` and the rust binary `libazul` are. This 
is done via `src/Makevars`.

### Important R Details

1.  **Garbage Collection**: R uses a mark-and-sweep GC. We use `R_MakeExternalPtr` to wrap 
    C pointers. We use `R_RegisterCFinalizerEx` to tell R: "When you free this pointer object, 
    run this C function (`_delete`)". This ensures no memory leaks.
2.  **`PROTECT` / `UNPROTECT`**: In the C glue, every `SEXP` created must be `PROTECT`-ed, 
    otherwise R's GC might delete it *during* the C function execution if allocation triggers GC. The generator handles this carefully with `unprotect_count`.
3.  **Naming**: R usually uses dots (`AzApp.new`) or CamelCase. The generator outputs `Class.method`.
4.  **Libraries**: The hardest part for users is ensuring `libazul.so` is in `LD_LIBRARY_PATH` 
    when R runs. Typically, R packages bundle static libraries (`.a`) to avoid this, but since you 
    are shipping a dynamic Rust library, users must install it or you must bundle it in `inst/libs`.
*/

/*

azul-r/
├── DESCRIPTION
├── NAMESPACE
├── R/
│   └── Azul.R        <-- Generated R code
├── src/
│   ├── azul_wrapper.c <-- Generated C glue
│   ├── azul.h         <-- Header from C generator
│   └── libazul.so     <-- Rust binary (or linked)
└── man/              <-- Documentation (Optional)

*/

/*

    library(azul)

    # Create Config
    config <- AzAppConfig.new()

    # Create Options
    # Passing NULL for raw pointers works in R C-API
    opts <- AzWindowCreateOptions.new(NULL) 

    # Create App
    app <- AzApp.new(NULL, config)

    # Run
    AzApp.run(app, opts)

    # Garbage collection happens automatically via the registered finalizers
    rm(app)
    gc()

*/

pub fn get_description_file() -> String {
    format!("
Package: azul
Title: Azul GUI Bindings
Version: 1.0.0
Author: Your Name
Maintainer: Your Email <you@example.com>
Description: R bindings for the Azul GUI toolkit.
License: MIT
Encoding: UTF-8
LazyData: true
Imports: methods
SystemRequirements: libazul
    ").trim().to_string()
}

pub fn get_namespace_file() -> String {
    format!("
useDynLib(azul, .registration = TRUE)
exportPattern(\"^[[:alpha:]]+\")
    ").trim().to_string()
}

pub fn get_src_makevars() -> String {
    format!("
PKG_LIBS = -L. -lazul
PKG_CPPFLAGS = -I.
    ").trim().to_string()
}

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/// Maps to C Macros for Rinternals.h (Converting SEXP to C)
fn map_to_r_input_converter(ty: &str, var_name: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            // SEXP -> const char*
            return format!("const char* c_{0} = CHAR(STRING_ELT({0}, 0));", var_name);
        }
        // SEXP -> void* (ExternalPtr)
        return format!("void* c_{0} = R_ExternalPtrAddr({0});", var_name);
    }

    match ty {
        "bool" | "GLboolean" => format!("int c_{0} = LOGICAL({0})[0];", var_name),
        "u8" | "i8" => format!("unsigned char c_{0} = (unsigned char)INTEGER({0})[0];", var_name), // R ints are 32bit
        "u16" | "i16" => format!("short c_{0} = (short)INTEGER({0})[0];", var_name),
        "u32" | "i32" | "GLuint" | "GLint" | "GLenum" | "AzScanCode" => format!("int c_{0} = INTEGER({0})[0];", var_name),
        // R doesn't support 64-bit int natively in SEXP, usually generic vectors or double. We use double here.
        "u64" | "i64" | "usize" | "size_t" => format!("size_t c_{0} = (size_t)REAL({0})[0];", var_name),
        "f32" | "GLfloat" | "AzF32" => format!("float c_{0} = (float)REAL({0})[0];", var_name),
        "f64" | "GLdouble" => format!("double c_{0} = REAL({0})[0];", var_name),
        // Struct by value: In simple bindings, we usually assume the user passed an ExternalPtr 
        // to a struct, and we dereference it. 
        s if s.starts_with(PREFIX) => format!("{0}* ptr_{1} = ({0}*)R_ExternalPtrAddr({1}); {0} c_{1} = *ptr_{1};", s, var_name),
        _ => format!("void* c_{0} = R_ExternalPtrAddr({0});", var_name),
    }
}

/// Maps to R Output Constructors (C -> SEXP)
fn map_to_r_output_converter(ty: &str, expr: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
             return format!("PROTECT(r_ret = mkString({})); unprotect_count++;", expr);
        }
        // Pointer -> ExternalPtr
        // We need a tag/type name usually, but for simplicity here we use NULL tag
        return format!("PROTECT(r_ret = R_MakeExternalPtr({}, R_NilValue, R_NilValue)); unprotect_count++;", expr);
    }

    match ty {
        "void" | "c_void" => format!("{}; r_ret = R_NilValue;", expr),
        "bool" | "GLboolean" => format!("PROTECT(r_ret = allocVector(LGLSXP, 1)); LOGICAL(r_ret)[0] = {}; unprotect_count++;", expr),
        "i32" | "u32" | "GLint" | "GLuint" | "u8" | "i8" => 
            format!("PROTECT(r_ret = allocVector(INTSXP, 1)); INTEGER(r_ret)[0] = {}; unprotect_count++;", expr),
        "f32" | "f64" | "double" | "float" | "u64" | "i64" | "usize" => 
            format!("PROTECT(r_ret = allocVector(REALSXP, 1)); REAL(r_ret)[0] = (double){}; unprotect_count++;", expr),
        // Returning struct by value: We must heap allocate it and return XPtr
        s if s.starts_with(PREFIX) => {
             format!("{0}* heap_res = ({0}*)malloc(sizeof({0})); *heap_res = {1}; \
                     PROTECT(r_ret = R_MakeExternalPtr(heap_res, R_NilValue, R_NilValue)); \
                     R_RegisterCFinalizerEx(r_ret, simple_free_finalizer, TRUE); \
                     unprotect_count++;", s, expr)
        }
        _ => format!("{}; r_ret = R_NilValue;", expr),
    }
}

pub fn generate_r_api(api_data: &ApiData, version: &str) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let version_data = api_data.get_version(version).unwrap();

    // -------------------------------------------------------------------------
    // 1. C Glue Code (src/azul_wrapper.c)
    // -------------------------------------------------------------------------
    let mut c_code = String::new();
    c_code.push_str("#include <R.h>\n");
    c_code.push_str("#include <Rinternals.h>\n");
    c_code.push_str("#include <stdlib.h>\n"); // malloc/free
    c_code.push_str("#include \"azul.h\"\n\n"); // Your API header
    
    // Generic finalizer for malloc'd structs returned by value
    c_code.push_str("static void simple_free_finalizer(SEXP ptr) {\n");
    c_code.push_str("    void* p = R_ExternalPtrAddr(ptr);\n");
    c_code.push_str("    if (p) free(p);\n");
    c_code.push_str("    R_ClearExternalPtr(ptr);\n");
    c_code.push_str("}\n\n");

    let mut registrations = Vec::new();

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Generate glue for Destructor (if exists)
            // Used for R_RegisterCFinalizerEx
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                let dtor_name = format!("{}_finalizer", class_c_name);
                c_code.push_str(&format!("static void {}(SEXP ptr) {{\n", dtor_name));
                c_code.push_str("    void* p = R_ExternalPtrAddr(ptr);\n");
                c_code.push_str(&format!("    if (p) {}_delete(({}*)p);\n", class_c_name, class_c_name));
                c_code.push_str("    R_ClearExternalPtr(ptr);\n");
                c_code.push_str("}\n\n");
            }

            // Function Generator
            let mut emit_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                let r_symbol = format!("R_{}", c_symbol);
                
                registrations.push((r_symbol.clone(), fn_data.fn_args.len()));

                c_code.push_str(&format!("SEXP {}(", r_symbol));
                
                let mut args = Vec::new();
                if !is_ctor { args.push("SEXP self".to_string()); }
                for arg in &fn_data.fn_args {
                    for (name, _) in arg {
                        if name == "self" { continue; }
                        args.push(format!("SEXP {}", name));
                    }
                }
                c_code.push_str(&args.join(", "));
                c_code.push_str(") {\n");
                
                // Body
                c_code.push_str("    int unprotect_count = 0;\n");
                c_code.push_str("    SEXP r_ret = R_NilValue;\n");

                // Input conversions
                let mut call_args = Vec::new();
                if !is_ctor {
                    // Extract self
                    c_code.push_str(&format!("    {}* c_self = ({0}*)R_ExternalPtrAddr(self);\n", class_c_name));
                    call_args.push("c_self".to_string());
                }

                for arg in &fn_data.fn_args {
                    for (name, ty) in arg {
                        if name == "self" { continue; }
                        c_code.push_str(&format!("    {}\n", map_to_r_input_converter(ty, name)));
                        call_args.push(format!("c_{}", name));
                    }
                }

                // Call and Return conversion
                let ret_type = fn_data.returns.as_ref().map(|r| r.r#type.as_str()).unwrap_or("void");
                let call_expr = format!("{}({})", c_symbol, call_args.join(", "));
                c_code.push_str(&format!("    {}\n", map_to_r_output_converter(ret_type, &call_expr)));

                // If this is a constructor/factory and we have a destructor, attach finalizer
                let has_dtor = class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object;
                if is_ctor && has_dtor && (ret_type.contains('*') || ret_type.starts_with(PREFIX)) {
                    c_code.push_str(&format!("    R_RegisterCFinalizerEx(r_ret, {}_finalizer, TRUE);\n", class_c_name));
                }

                c_code.push_str("    if (unprotect_count > 0) UNPROTECT(unprotect_count);\n");
                c_code.push_str("    return r_ret;\n");
                c_code.push_str("}\n\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_func(name, data, false); }
            }
        }
    }

    // R Registration Boilerplate
    c_code.push_str("#include <R_ext/Rdynload.h>\n\n");
    c_code.push_str("static const R_CallMethodDef CallEntries[] = {\n");
    for (name, argc) in &registrations {
        c_code.push_str(&format!("    {{\"{}\", (DL_FUNC) &{}, {}}},\n", name, name, argc));
    }
    c_code.push_str("    {NULL, NULL, 0}\n");
    c_code.push_str("};\n\n");
    c_code.push_str("void R_init_azul(DllInfo *dll) {\n");
    c_code.push_str("    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);\n");
    c_code.push_str("    R_useDynamicSymbols(dll, FALSE);\n");
    c_code.push_str("}\n");

    files.insert("src/azul_wrapper.c".to_string(), c_code);

    // -------------------------------------------------------------------------
    // 2. R Script (R/Azul.R)
    // -------------------------------------------------------------------------
    let mut r_code = String::new();
    r_code.push_str("#' @useDynLib azul, .registration = TRUE\n");
    r_code.push_str("#' @importFrom Rcpp sourceCpp\n\n"); // often standard boilerplate

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);
            
            // Enums as simple named lists
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    r_code.push_str(&format!("{} <- list(\n", class_c_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            r_code.push_str(&format!("  {} = {},\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    r_code.push_str(")\n\n");
                }
            }

            // Functions
            let mut emit_r_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                 let suffix = snake_case_to_lower_camel(fn_name);
                 let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                 let r_symbol = format!("R_{}", c_symbol);
                 let r_func_name = if is_ctor { format!("{}.{}", class_c_name, fn_name) } else { format!("{}.${}", class_c_name, fn_name) }; // R naming convention variable

                 // R arguments
                 let mut args = Vec::new();
                 if !is_ctor { args.push("self".to_string()); }
                 for arg in &fn_data.fn_args {
                     for (name, _) in arg {
                         if name == "self" { continue; }
                         args.push(name.clone());
                     }
                 }

                 r_code.push_str(&format!("{} <- function({}) {{\n", r_func_name, args.join(", ")));
                 r_code.push_str(&format!("  .Call(\"{}\", {})\n", r_symbol, args.join(", ")));
                 r_code.push_str("}\n\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_r_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_r_func(name, data, false); }
            }
        }
    }
    
    files.insert("R/Azul.R".to_string(), r_code);

    files
}
