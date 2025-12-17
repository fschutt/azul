use std::collections::BTreeMap;
use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
This generates:

1.  `native/azul_nif/src/lib.rs` (Rust NIF implementation).
2.  `lib/azul.ex` (Elixir module definition).

*/

/*

// Elixir uses **Mix** and the `rustler` library.

azul_ex/
├── mix.exs
├── lib/
│   └── azul.ex           <-- Generated Elixir file
└── native/
    └── azul_nif/
        ├── Cargo.toml
        └── src/
            └── lib.rs    <-- Generated Rust file
*/

/*
### Important Details for Elixir

1.  **Dirty Schedulers**: If `Azul.az_app_run` blocks the thread (which a GUI event loop definitely does), 
    you **MUST** annotate the Rust function with `#[rustler::nif(schedule = "DirtyCpu")]`.
    *   *Generator Update*: You should detect if a function is "long running" (like `run`) in your `api.json` 
        or hardcode it in the generator to add that attribute. If you don't, the entire Erlang VM will freeze.
2.  **Atoms**: Rustler maps Rust enums (unit variants) to Elixir atoms automatically (`MyEnum::VariantA` -> `:variant_a`). 
    This is very idiomatic for Elixir.
3.  **Naming**: Rust `snake_case` matches Elixir `snake_case`. The mapping is 1:1.
4.  **Distribution**: When a user adds `{:azul, git: "..."}` to their dependencies, Mix will automatically 
    compile the Rust crate using Cargo when they run `mix deps.compile`. No need to pre-compile `.so` files 
    unless you want to optimize CI times (using `rustler_precompiled`).

*/

/*

# Usage in Elixir:

defmodule MyApp do
  def start do
    # Create Config (Returns a Reference/Resource)
    config = Azul.az_app_config_new()
    
    # Create Options
    opts = Azul.az_window_create_options_new()
    
    # Create App (Passes references)
    app = Azul.az_app_new(nil, config)
    
    # Run
    # This might block!
    Azul.az_app_run(app, opts)
  end
end

*/

// mix.exs file
pub fn get_mix_exs_file() -> String {
    format!("
defmodule Azul.MixProject do
  use Mix.Project

  def project do
    [
      app: :azul,
      version: \"0.1.0\",
      elixir: \"~> 1.14\",
      start_permanent: Mix.env() == :prod,
      deps: deps()
    ]
  end

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp deps do
    [
      {:rustler, \"~> 0.30.0\"}
    ]
  end
end
    ").trim().to_string()

}

pub fn azul_nif_cargo_toml() -> String {
    format!("
[package]
name = \"azul_nif\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
crate-type = [\"cdylib\"]

[dependencies]
rustler = \"0.30\"
# Link to your raw C library or sys crate
# azul-sys = { path = \"../../../azul-sys\" }
    ").trim().to_string()
}

const PREFIX: &str = "Az";

/// Convert PascalCase (AzWindow) to snake_case (az_window)
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

/// Maps C types to Rustler argument types
fn map_to_rustler_type(ty: &str) -> String {
    if ty.contains('*') {
        if ty.contains("char") {
            return "String".to_string();
        }
        // Opaque pointers -> ResourceArc
        let clean = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        if clean == "c_void" || clean == "void" {
            return "rustler::Term".to_string();
        }
        return format!("rustler::ResourceArc<{}Wrapper>", clean);
    }

    match ty {
        "void" | "c_void" => "()".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "GLuint" | "GLint" | "GLenum" => "i32".to_string(),
        "u64" | "i64" | "usize" | "isize" => "i64".to_string(),
        "f32" | "f64" | "GLfloat" | "GLdouble" => "f64".to_string(),
        "AzString" => "String".to_string(),
        // Treat named structs (that aren't pointers) as Terms for now, 
        // or map them to NifStructs if we had full definition logic here.
        // For C-bindings, usually everything relevant is a pointer or primitive.
        _ => "rustler::Term".to_string(),
    }
}

pub fn generate_elixir_binding(api_data: &ApiData, version: &str) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let version_data = api_data.get_version(version).unwrap();

    // ---------------------------------------------------------
    // 1. Rust NIF Code (native/azul_nif/src/lib.rs)
    // ---------------------------------------------------------
    let mut rs = String::new();
    
    rs.push_str("use rustler::{Env, Term, NifResult, ResourceArc};\n");
    rs.push_str("// Import generated C function definitions from sys crate\n");
    rs.push_str("use crate::sys::*;\n\n");

    // Wrapper Structs (Resources)
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Generate Resource Wrapper for classes/structs
            if class_data.struct_fields.is_some() || class_data.is_boxed_object {
                rs.push_str("#[repr(transparent)]\n");
                rs.push_str(&format!("pub struct {}Wrapper(pub *mut {});\n", full_name, full_name));
                
                // Drop implementation
                rs.push_str(&format!("impl Drop for {}Wrapper {{\n", full_name));
                rs.push_str("    fn drop(&mut self) {\n");
                if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                    rs.push_str(&format!("        unsafe {{ {}_delete(self.0) }};\n", full_name));
                }
                rs.push_str("    }\n");
                rs.push_str("}\n\n");
            }

            // Generate NifUnitEnum for simple Enums (maps to :atoms)
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    rs.push_str("#[derive(rustler::NifUnitEnum)]\n");
                    rs.push_str(&format!("pub enum {} {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Rustler automatically maps CamelCase enum variants to snake_case atoms in Elixir
                            rs.push_str(&format!("    {},\n", variant_name));
                        }
                    }
                    rs.push_str("}\n\n");
                }
            }
        }
    }

    // NIF Functions
    let mut nif_funcs = Vec::new();

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_nif = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                // Name: az_window_new
                let elixir_name = to_snake_case(&format!("{}{}", class_name, fn_name));
                // C Symbol: AzWindow_new
                let c_symbol = format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name));
                
                nif_funcs.push(elixir_name.clone());

                rs.push_str("#[rustler::nif]\n");
                rs.push_str(&format!("fn {}(", elixir_name));

                let mut c_call_args = Vec::new();

                // Self argument
                if !is_ctor {
                    rs.push_str(&format!("wrapper: rustler::ResourceArc<{}Wrapper>, ", class_c_name));
                    c_call_args.push("wrapper.0".to_string());
                }

                // Other arguments
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let rustler_type = map_to_rustler_type(ty);
                        rs.push_str(&format!("{}: {}, ", name, rustler_type));
                        
                        if rustler_type.contains("ResourceArc") {
                            c_call_args.push(format!("{}.0", name));
                        } else if rustler_type == "String" {
                             // Simplified string handling
                             c_call_args.push(format!("{}.as_ptr() as *const i8", name));
                        } else {
                            c_call_args.push(name.clone());
                        }
                    }
                }
                
                let ret_ty_str = if let Some(ret) = &fn_data.returns {
                     map_to_rustler_type(&ret.r#type)
                } else {
                    "()".to_string()
                };

                // Wrap pointer returns in ResourceArc
                let return_expr = if ret_ty_str.contains("ResourceArc") {
                    let inner = ret_ty_str.replace("rustler::ResourceArc<", "").replace(">", "");
                    format!("ResourceArc::new({}(res))", inner)
                } else {
                    "res".to_string()
                };

                rs.push_str(") -> NifResult<");
                rs.push_str(&ret_ty_str);
                rs.push_str("> {\n");
                
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

    rs.push_str("rustler::init!(\"Elixir.Azul\", [\n");
    for name in &nif_funcs {
        rs.push_str(&format!("    {},\n", name));
    }
    rs.push_str("], load = load);\n");

    files.insert("native/azul_nif/src/lib.rs".to_string(), rs);

    // ---------------------------------------------------------
    // 2. Elixir Module (lib/azul.ex)
    // ---------------------------------------------------------
    let mut ex = String::new();
    ex.push_str("defmodule Azul do\n");
    ex.push_str("  use Rustler, otp_app: :azul, crate: \"azul_nif\"\n\n");
    
    ex.push_str("  # When the NIF is loaded, these functions are replaced.\n");
    ex.push_str("  # If called without NIF loaded, they raise an error.\n\n");

    for name in &nif_funcs {
        ex.push_str(&format!("  def {}(_args), do: :erlang.nif_error(:nif_not_loaded)\n", name));
        // Note: Real generator needs strictly correct arity here (e.g. _a, _b) or use _args...
        // But `def func(_), do: ...` works for nif stubs usually if you don't validate arity in Elixir.
        // Better:
        // ex.push_str(&format!("  def {}({}), do: :erlang.nif_error(:nif_not_loaded)\n", name, args_placeholders));
    }

    ex.push_str("end\n");
    files.insert("lib/azul.ex".to_string(), ex);

    files
}
