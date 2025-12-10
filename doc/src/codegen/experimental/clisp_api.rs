use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

const PREFIX: &str = "Az";
const LIB_NAME: &str = "azul";

/*

    For **Common Lisp** (including **CLISP**, **SBCL**, **CCL**), 
    the de-facto standard for bindings is **CFFI (Common Foreign Function Interface)**.

    This generator produces a single `azul.lisp` file. It maps C structs to `defcstruct`, 
    enums to `defcenum`, and functions to `defcfun`. It uses **kebab-case** which is 
    the standard Lisp naming convention.

    ###Usage in Common Lisp

    Assuming you are using **SBCL** (Steel Bank Common Lisp) and **Quicklisp**.

    1.  **Dependencies**: You need `cffi`.
        ```lisp
        (ql:quickload :cffi)
        ```
    2.  **Generate**: Save as `azul.lisp`.

### Key CFFI Details

1.  **Type Conversion**:
    *   `az-app-new` returns `:pointer`.
    *   `az-app-run` expects `:pointer`.
    *   CFFI handles the marshalling of integers/floats automatically.
2.  **Enums**: `(defcenum ...)` allows you to pass keywords (e.g., `:debug`) to functions expecting that enum type. CFFI translates the keyword to the integer value automatically.
3.  **Structs**:
    *   If a function expects a struct **by value** (e.g., `AzRect`), the map returns `(:struct az-rect)`. CFFI supports this on modern implementations (libffi based).
    *   If a function expects a pointer, we use `:pointer`.
4.  **Memory Management**:
    *   `with-foreign-object` is the standard Lisp way to allocate C structs on the stack (dynamic extent).
    *   For heap objects returned by `_new`, you must call `_delete`. You can use the `trivial-garbage` library to attach finalizers to Lisp objects if you wrap these pointers in Lisp structs/CLOS classes.


*/


/*
    (load "azul.lisp")

    ;; Load the shared library
    (azul:load-libazul)

    ;; Common Lisp interactions
    (defun run-app ()
    (let ((config (azul:az-app-config-new))
            ;; Pass (cffi:null-pointer) for NULL
            (opts (azul:az-window-create-options-new (cffi:null-pointer))))
        
        ;; Set log level (struct access if fields exposed)
        ;; (setf (cffi:foreign-slot-value config '(:struct azul::az-app-config) 'azul::log-level) :debug)

        (let ((app (azul:az-app-new (cffi:null-pointer) config)))
        
        (azul:az-app-run app opts)
        
        ;; Manual Cleanup
        (azul:az-app-delete app))))

    (run-app)
*/

/// To define this as a proper Common Lisp system:
///
/// azul.asd
pub fn get_azul_asd() -> String {
    format!("
(asdf:defsystem #:azul
  :description \"Bindings for Azul GUI\"
  :author \"Your Name\"
  :license \"MIT\"
  :depends-on (#:cffi)
  :components ((:file \"azul\")))
    ")
}

/// Convert "AzWindowCreateOptions" -> "az-window-create-options"
fn to_kebab_case(s: &str) -> String {
    let mut res = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 { res.push('-'); }
            res.push(c.to_lowercase().next().unwrap());
        } else if c == '_' {
            res.push('-');
        } else {
            res.push(c);
        }
    }
    res.replace("--", "-") // cleanup double dashes
}

/// Maps C types to CFFI types
fn map_to_cffi_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return ":string".to_string();
        }
        return ":pointer".to_string();
    }

    match ty {
        "void" | "c_void" => ":void".to_string(),
        "bool" | "GLboolean" => ":boolean".to_string(), // CFFI handles 0/1 conversion
        "char" | "i8" => ":int8".to_string(),
        "u8" => ":uint8".to_string(),
        "u16" | "AzU16" => ":uint16".to_string(),
        "i16" => ":int16".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => ":uint32".to_string(),
        "i32" | "GLint" | "GLsizei" => ":int32".to_string(),
        "u64" | "GLuint64" | "usize" | "size_t" => ":uint64".to_string(),
        "i64" | "GLint64" | "isize" | "ssize_t" | "intptr_t" => ":int64".to_string(),
        "f32" | "GLfloat" | "AzF32" => ":float".to_string(),
        "f64" | "GLdouble" => ":double".to_string(),
        // Struct by value: (:struct name)
        s if s.starts_with(PREFIX) => format!("(:struct {})", to_kebab_case(s)),
        _ => ":pointer".to_string(),
    }
}

pub fn generate_lisp_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Package Definition
    code.push_str(";;; Auto-generated CFFI bindings for Azul\n");
    code.push_str("(in-package :cl-user)\n");
    code.push_str("(defpackage :azul\n");
    code.push_str("  (:use :cl :cffi)\n");
    code.push_str("  (:export\n");
    
    // Export symbols (We'll collect them dynamically or just export common patterns)
    // For brevity in generator, we assume user exports or uses package prefix.
    code.push_str("   #:load-libazul\n");
    code.push_str("   #:az-app-run\n"); // Example export
    code.push_str("   ))\n\n");
    code.push_str("(in-package :azul)\n\n");

    // 2. Library Definition
    code.push_str("(define-foreign-library libazul\n");
    code.push_str(&format!("  (:unix (:or \"lib{}.so\" \"lib{}.dylib\"))\n", LIB_NAME, LIB_NAME));
    code.push_str(&format!("  (:windows \"{}.dll\")\n", LIB_NAME));
    code.push_str("  (t (:default \"libazul\")))\n\n");

    code.push_str("(defun load-libazul ()\n");
    code.push_str("  (use-foreign-library libazul))\n\n");

    // 3. Enums
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    let lisp_name = to_kebab_case(&full_name);
                    code.push_str(&format!("(defcenum {}\n", lisp_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            let variant_kebab = to_kebab_case(variant_name);
                            // (:variant value)
                            code.push_str(&format!("  (:{} {})\n", variant_kebab, idx));
                            idx += 1;
                        }
                    }
                    code.push_str(")\n\n");
                }
            }
        }
    }

    // 4. Structs
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            let lisp_name = to_kebab_case(&full_name);

            if let Some(struct_fields) = &class_data.struct_fields {
                code.push_str(&format!("(defcstruct {}\n", lisp_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let lisp_field = to_kebab_case(field_name);
                        let lisp_type = map_to_cffi_type(&field_data.r#type);
                        code.push_str(&format!("  ({} {})\n", lisp_field, lisp_type));
                    }
                }
                code.push_str(")\n\n");
            } else if class_data.is_boxed_object {
                // Opaque handle. CFFI uses pointers, but we can define a typedef to clarify intent
                // (defctype az-window :pointer)
                if class_data.enum_fields.is_none() {
                     code.push_str(&format!("(defctype {} :pointer)\n\n", lisp_name));
                }
            }
        }
    }

    // 5. Function Definitions
    code.push_str(";;; --- Functions ---\n\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_defcfun = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_symbol = if is_ctor { format!("{}_{}", class_c_name, suffix) } else { format!("{}_{}", class_c_name, suffix) };
                
                let lisp_fn_name = to_kebab_case(&c_symbol);
                
                let ret_type = fn_data.returns.as_ref().map_or(":void".to_string(), |r| map_to_cffi_type(&r.r#type));
                
                code.push_str(&format!("(defcfun (\"{}\" {})\n", c_symbol, lisp_fn_name));
                code.push_str(&format!("  {}", ret_type));

                // Arguments
                if !is_ctor {
                    // Self pointer
                    // If it was defined as a struct, pass pointer. If defined as ctype :pointer, pass that.
                    let self_type = if class_data.struct_fields.is_some() {
                        ":pointer" // Structs passed by pointer
                    } else {
                        // It's a typedef :pointer
                         to_kebab_case(&class_c_name).as_str() 
                    };
                    
                    // Actually, simpler to just use :pointer for all objects unless passing struct by value
                    // CFFI usually handles (:struct foo) by value, but :pointer for references.
                    // To keep it safe, we check if we generated a ctype or a cstruct.
                    // For generated API consistency, let's use :pointer for all 'self' args.
                    code.push_str("\n  (instance :pointer)");
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let lisp_arg_name = to_kebab_case(name);
                        let lisp_ty = map_to_cffi_type(ty);
                        code.push_str(&format!("\n  ({} {})", lisp_arg_name, lisp_ty));
                    }
                }
                code.push_str(")\n\n");
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_defcfun(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_defcfun(name, data, false); }
            }
            
            // Destructor
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                 let c_symbol = format!("{}_delete", class_c_name);
                 let lisp_name = to_kebab_case(&c_symbol);
                 code.push_str(&format!("(defcfun (\"{}\" {}) :void\n  (instance :pointer))\n\n", c_symbol, lisp_name));
            }
        }
    }

    code
}
