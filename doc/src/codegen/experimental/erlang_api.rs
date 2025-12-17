use std::collections::BTreeMap;
use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
This generator produces two files:

- native/azul_nif/src/lib.rs (The Rust NIF logic)
- src/azul.erl (The Erlang module interface)

*/

/*

Erlang uses Rebar3 for build management. rustler integrates 
via the rebar3_rustler plugin, or simply by compiling the rust 
crate to the priv/ directory.

azul-erl/
├── rebar.config
├── src/
│   └── azul.erl          <-- Generated
└── native/
    └── azul_nif/
        ├── Cargo.toml
        └── src/
            └── lib.rs    <-- Generated
*/

/*
### Key Erlang Nuances

1.  **Immutability**: Erlang data is immutable. The "Resource" (Reference) approach fits perfectly here. 
    You pass a reference, C modifies the memory *pointed to* by the reference, but the Erlang reference 
    term itself remains unchanged.
2.  **Concurrency**: If you run `azul:az_app_run` (which loops forever), you **block the Erlang scheduler**.
    *   *Solution*: You must mark long-running functions (like `Run`) as `#[rustler::nif(schedule = "DirtyCpu")]`. This tells the 
        VM to move this call to a separate thread so it doesn't freeze the rest of the Erlang node.
3.  **Binaries vs Strings**: Erlang strings are lists of integers (slow). Erlang Binaries (`<<...>>`) are fast arrays. 
    Rustler usually maps `String` to Erlang binaries or char-lists. For API boundaries, binaries are preferred.

### How to Install

Users include your library in their `rebar.config`:

```erlang
{deps, [
    {azul, {git, "https://github.com/your/azul-erl.git", {branch, "main"}}}
]}.
```

Rebar3 will clone it, trigger the `cargo build` hook, and generate the NIF shared library.

*/

/*

// Because we mapped "Classes" to NIF Resources, Erlang 
// variables hold opaque handles.

-module(my_app).
-export([start/0]).

start() ->
    %% Create Config (Resource)
    Config = azul:az_app_config_new(),
    
    %% Create Window Options
    Opts = azul:az_window_create_options_new(),
    
    %% Create App
    App = azul:az_app_new(undefined, Config),
    
    %% Run
    azul:az_app_run(App, Opts).
    %% When 'App' goes out of scope and is GC'd by Erlang, 
    %% Rustler calls AzApp_delete automatically.

*/

// native/azul_nif/Cargo.toml
pub fn get_azul_nif_cargo_toml() -> String {
    format!("
[package]
name = \"azul_nif\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
crate-type = [\"cdylib\"]

[dependencies]
rustler = \"0.30\"

# Link to C library
# [dependencies.azul-sys] 
# path = \"../../../azul-sys\"
    ").trim().to_string()
}

// rebar.config 
pub fn get_rebar_config() -> String {
    format!("
{erl_opts, [debug_info]}.
{deps, []}.

{pre_hooks,
  [{\"(linux|darwin|solaris)\", compile, \"cargo build --release --manifest-path=native/azul_nif/Cargo.toml\"},
   {\"(win32)\", compile, \"cargo build --release --manifest-path=native/azul_nif/Cargo.toml\"}
  ]}.

{post_hooks,
  [{\"(linux|darwin|solaris)\", compile, \"cp native/azul_nif/target/release/libazul_nif.so priv/libazul_nif.so\"},
   {\"(win32)\", compile, \"copy native\\azul_nif\\target\\release\\azul_nif.dll priv\\libazul_nif.dll\"}
  ]}.
    ").trim().to_string()
}

const PREFIX: &str = "Az";

/// Maps C/Rust types to Rustler types
fn map_to_rustler_arg_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string();
        }
        // Opaque pointers are passed as ResourceArcs
        let clean = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        if clean == "c_void" || clean == "void" {
            return "rustler::Term".to_string(); // Generic term for void*
        }
        return format!("rustler::ResourceArc<{}Wrapper>", clean);
    }

    match ty {
        "void" | "c_void" => "()".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "GLuint" | "GLint" | "GLenum" => "i32".to_string(), // Erlang integers
        "u64" | "i64" | "usize" | "isize" => "i64".to_string(),
        "f32" | "f64" | "GLfloat" | "GLdouble" => "f64".to_string(),
        "AzString" => "String".to_string(),
        s if s.starts_with(PREFIX) => {
            // Structs by value are tricky in NIFs without definitions.
            // We assume simple structs are mapped to NifStruct or NifTuple.
            // For this generator, we treat named types as Enums (Atoms) or Structs.
            format!("{}", s)
        }
        _ => "rustler::Term".to_string(),
    }
}

/// Convert PascalCase (AzWindow) to snake_case (az_window) for Erlang atoms/funcs
fn to_snake_case(s: &str) -> String {
    let mut res = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 { res.push('_'); }
            res.push(c.to_lowercase().next().unwrap());
        } else {
            res.push(c);
        }
    }
    res
}

pub fn generate_erlang_binding(api_data: &ApiData, version: &str) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let version_data = api_data.get_version(version).unwrap();

    // ---------------------------------------------------------
    // 1. Rust NIF Code (lib.rs)
    // ---------------------------------------------------------
    let mut rs = String::new();
    
    rs.push_str("use rustler::{Env, Term, NifResult, ResourceArc, Atom};\n");
    rs.push_str("use std::ffi::c_void;\n");
    rs.push_str("// Assume sys crate exists with C definitions\n");
    rs.push_str("use crate::sys::*;\n\n");

    // Generate Wrapper Structs (Resources)
    // Erlang doesn't have pointers, so we wrap C pointers in "Resources"
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // If it's an object with a destructor or fields, we wrap it
            if class_data.struct_fields.is_some() || class_data.is_boxed_object {
                rs.push_str("#[repr(transparent)]\n");
                rs.push_str(&format!("pub struct {}Wrapper(pub *mut {});\n", full_name, full_name));
                
                // Implement Drop to call C destructor
                rs.push_str(&format!("impl Drop for {}Wrapper {{\n", full_name));
                rs.push_str("    fn drop(&mut self) {\n");
                if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                    rs.push_str(&format!("        unsafe {{ {}_delete(self.0) }};\n", full_name));
                }
                rs.push_str("    }\n");
                rs.push_str("}\n\n");
                // Register as Rustler resource handled in `load`
            }
            
            // If it's an enum, generate NifUnitEnum
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    rs.push_str("#[derive(rustler::NifUnitEnum)]\n");
                    rs.push_str(&format!("pub enum {} {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            rs.push_str(&format!("    {},\n", variant_name));
                        }
                    }
                    rs.push_str("}\n\n");
                }
            }
        }
    }

    // Generate NIF Functions
    let mut nif_funcs = Vec::new();

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helpers for logic
            let mut emit_nif = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                // erlang name: az_window_new
                let erl_name = to_snake_case(&format!("{}{}", class_name, fn_name));
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                nif_funcs.push((erl_name.clone(), 0)); // Arity needs calculation

                rs.push_str("#[rustler::nif]\n");
                rs.push_str(&format!("fn {}(", erl_name));

                let mut rust_args = Vec::new();
                let mut c_call_args = Vec::new();

                if !is_ctor {
                    rs.push_str(&format!("wrapper: rustler::ResourceArc<{}Wrapper>, ", class_c_name));
                    c_call_args.push("wrapper.0".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let rustler_type = map_to_rustler_arg_type(ty);
                        rs.push_str(&format!("{}: {}, ", name, rustler_type));
                        
                        if rustler_type.contains("ResourceArc") {
                            c_call_args.push(format!("{}.0", name));
                        } else if rustler_type == "String" {
                            // CString conversion needed (simplified here)
                             c_call_args.push(format!("{}.as_ptr() as *const i8", name));
                        } else {
                            c_call_args.push(name.clone());
                        }
                    }
                }
                
                // Return Type
                let ret_ty_str = if let Some(ret) = &fn_data.returns {
                     map_to_rustler_arg_type(&ret.r#type)
                } else {
                    "()".to_string()
                };

                let return_expr = if ret_ty_str.contains("ResourceArc") {
                    // Extract inner type wrapper
                    let inner = ret_ty_str.replace("rustler::ResourceArc<", "").replace(">", "");
                    format!("ResourceArc::new({}(res))", inner)
                } else {
                    "res".to_string()
                };

                rs.push_str(") -> NifResult<");
                rs.push_str(&ret_ty_str);
                rs.push_str("> {\n");
                
                // Body
                rs.push_str("    let res = unsafe { ");
                rs.push_str(&format!("{}({})", c_symbol, c_call_args.join(", ")));
                rs.push_str(" };\n");
                rs.push_str(&format!("    Ok({})\n", return_expr));
                rs.push_str("}\n\n");
            };
            
            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_nif(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_nif(name, data, false); }
            }
        }
    }

    // Init Block
    rs.push_str("fn load(env: Env, _: Term) -> bool {\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
             if class_data.struct_fields.is_some() || class_data.is_boxed_object {
                 rs.push_str(&format!("    rustler::resource!(Az{}Wrapper, env);\n", class_name));
             }
        }
    }
    rs.push_str("    true\n");
    rs.push_str("}\n\n");

    rs.push_str("rustler::init!(\"azul\", [\n");
    for (name, _) in &nif_funcs {
        rs.push_str(&format!("    {},\n", name));
    }
    rs.push_str("], load = load);\n");

    files.insert("native/azul_nif/src/lib.rs".to_string(), rs);

    // ---------------------------------------------------------
    // 2. Erlang Code (azul.erl)
    // ---------------------------------------------------------
    let mut erl = String::new();
    erl.push_str("-module(azul).\n");
    
    // Exports
    erl.push_str("-export([\n");
    // Add init
    erl.push_str("    init/0");
    for (name, _) in &nif_funcs {
        erl.push_str(&format!(",\n    {}/TODO_ARITY", name)); // Real generator must calc arity
    }
    erl.push_str("\n]).\n");
    erl.push_str("-on_load(init/0).\n\n");

    erl.push_str("init() ->\n");
    erl.push_str("    PrivDir = case code:priv_dir(?MODULE) of\n");
    erl.push_str("        {error, _} ->\n");
    erl.push_str("            EbinDir = filename:dirname(code:which(?MODULE)),\n");
    erl.push_str("            AppDir = filename:dirname(EbinDir),\n");
    erl.push_str("            filename:join(AppDir, \"priv\");\n");
    erl.push_str("        Path -> Path\n");
    erl.push_str("    end,\n");
    erl.push_str("    erlang:load_nif(filename:join(PrivDir, \"libazul_nif\"), 0).\n\n");

    // Stubs
    for (name, _) in &nif_funcs {
        erl.push_str(&format!("{}(_Args) ->\n", name));
        erl.push_str("    erlang:nif_error(nif_not_loaded).\n\n");
    }

    files.insert("src/azul.erl".to_string(), erl);

    files
}
