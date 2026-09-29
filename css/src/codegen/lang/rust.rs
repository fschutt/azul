//! Rust: the `azul` crate (`dll/`, re-exports in `target/codegen/reexports.rs`).
//!
//! Spellings (from the generated `dll_api_external.rs` / `reexports.rs`):
//! types without the `Az` prefix under `azul::<api module>::*`; api.json
//! constructors keep their snake_case names (`CssProperty::text_color`,
//! `FloatValue::create`); enums are native Rust enums (variants, and
//! `LayoutWidthValue::Exact(..)` through the `CssPropertyValue<T>` alias);
//! structs have pub fields; `XxxVec: From<Vec<Xxx>>`; `azul::str::String:
//! From<&str>`.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::codegen::{
    doc::{render, Doc},
    ir::{EnumShape, Ident, Item, LibrarySpec, Module, Prim},
    lang::{
        dom::unused_params, expr_doc_top, item_comments, used_api_modules, ConcatPart, ExprSyntax,
        MethodLayout,
    },
    CodegenBackend, GeneratedFile, AZUL_VERSION,
};

/// The Rust printer.
#[derive(Debug, Copy, Clone, Default)]
pub struct Rust;

/// Rust keywords that need a raw identifier (`r#type`) as a field name.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
    "typeof", "unsized", "virtual", "yield", "try", "gen",
];

fn ident(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

impl ExprSyntax for Rust {
    fn int(&self, value: i128, _ty: Prim) -> String {
        value.to_string()
    }

    fn float(&self, text: &str, ty: Prim) -> String {
        let t = if ty == Prim::F64 { "f64" } else { "f32" };
        match text {
            "nan" => format!("{t}::NAN"),
            "inf" => format!("{t}::INFINITY"),
            "-inf" => format!("{t}::NEG_INFINITY"),
            _ => text.to_string(),
        }
    }

    fn boolean(&self, b: bool) -> String {
        b.to_string()
    }

    fn string(&self, s: &str) -> Doc {
        Doc::text(format!("azul::str::String::from({s:?})"))
    }

    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        Doc::call(format!("{class}::{}", ident(method)), args, broken)
    }

    fn variant(&self, ty: &str, _shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc {
        if args.is_empty() {
            Doc::text(format!("{ty}::{variant}"))
        } else {
            Doc::call(format!("{ty}::{variant}"), args, broken)
        }
    }

    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc {
        let fields = fields
            .into_iter()
            .map(|(k, v)| Doc::cat(vec![Doc::text(format!("{}: ", ident(&k))), v]))
            .collect();
        Doc::cat(vec![
            Doc::text(format!("{ty} ")),
            Doc::list("{", fields, ", ", "}", true, broken, true),
        ])
    }

    fn vec(&self, ty: &str, _elem: &str, items: Vec<Doc>, broken: bool) -> Doc {
        if items.is_empty() {
            return Doc::text(format!("{ty}::create()"));
        }
        Doc::cat(vec![
            Doc::text(format!("{ty}::from(vec!")),
            Doc::list("[", items, ", ", "]", false, broken, true),
            Doc::text(")"),
        ])
    }

    fn unsupported(&self, what: &str) -> Doc {
        Doc::text(format!("unimplemented!({what:?})"))
    }

    fn dom_limitation(&self) -> Option<&'static str> {
        None
    }

    /// A builder chain: `Dom::create_div()` then one `.with_x(..)` per line.
    fn method(&self, recv: Doc, _class: &str, method: &str, args: Vec<Doc>, layout: MethodLayout) -> Doc {
        Doc::chained(recv, Doc::call(format!(".{}", ident(method)), args, layout.args_tall))
    }

    /// A `&str` parameter as the `azul::str::String` a call takes.
    fn param(&self, name: &Ident) -> Doc {
        Doc::text(format!("azul::str::String::from({})", param_ident(name)))
    }

    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc {
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                ConcatPart::Lit(s) => {
                    let quoted = format!("{s:?}");
                    let body = &quoted[1..quoted.len() - 1];
                    fmt.push_str(&body.replace('{', "{{").replace('}', "}}"));
                }
                ConcatPart::Param(i) => {
                    fmt.push_str("{}");
                    args.push(param_ident(i));
                }
            }
        }
        Doc::text(format!(
            "azul::str::String::from(format!(\"{fmt}\", {}))",
            args.join(", ")
        ))
    }
}

/// A parameter's Rust name (a keyword as a raw identifier: `r#type`).
fn param_ident(name: &Ident) -> String {
    ident(&name.snake())
}

fn param_type(ty: &str) -> String {
    if ty == "String" {
        "&str".to_string()
    } else {
        ty.to_string()
    }
}

fn item_fn(item: &Item) -> String {
    let mut out = String::new();
    for line in &item_comments(&Rust, item) {
        out.push_str(&format!("/// {line}\n"));
    }
    let body = render(&expr_doc_top(&Rust, &item.value), "    ", 1);
    let params = item
        .params
        .iter()
        .map(|p| format!("{}: {}", param_ident(&p.name), param_type(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let unused: String = unused_params(item)
        .iter()
        .map(|p| format!("    let _ = {};\n", param_ident(&p.name)))
        .collect();
    out.push_str(&format!(
        "pub fn {}({params}) -> {} {{\n{unused}    {body}\n}}\n",
        item.name.snake(),
        item.ty
    ));
    out
}

/// What the Rust registration imports (the component types are not in the
/// IR's types).
const RUST_REGISTRATION_IMPORTS: &str = "use azul::component::{
    CompileTarget, ComponentDataField, ComponentDataModel, ComponentDef, ComponentDefaultValue,
    ComponentFieldType, ComponentId, ComponentLibrary, ComponentMap, ComponentSource,
};
use azul::error::{ResultStringCompileError, ResultStyledDomRenderDomError};
use azul::option::{OptionComponentDefaultValue, OptionString};
use azul::prelude::StyledDom;
use azul::vec::{ComponentDataFieldVec, ComponentDataModelVec, ComponentEnumModelVec};
";

/// What the registration calls when a component takes parameters.
const RUST_REGISTRATION_HELPERS: &str = "
/// The String value of the data-model field `name`, or `default`.
fn model_string(model: &ComponentDataModel, name: &str, default: &str) -> String {
    for field in model.fields.as_slice() {
        if field.name.as_str() == name {
            if let OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) =
                &field.default_value
            {
                return s.as_str().to_string();
            }
        }
    }
    default.to_string()
}

/// A String field of a component's data model.
fn string_field(name: &str, default: &str, description: &str) -> ComponentDataField {
    ComponentDataField {
        name: azul::str::String::from(name),
        field_type: ComponentFieldType::String,
        default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
            azul::str::String::from(default),
        )),
        required: false,
        description: azul::str::String::from(description),
    }
}
";

/// `azul::str::String::from("..")`.
fn az_str(s: &str) -> String {
    Rust.string(s).flat()
}

/// The registration of a component library: per component a
/// default-arguments wrapper, a render function reading the data model, a
/// compile function and its `ComponentDef`; then `register_<lib>_library`.
fn rust_registration(m: &Module, lib: &LibrarySpec) -> String {
    let lsn = Ident::from_text(&lib.name).snake();
    let mut s = String::from("\n// ── registration ──\n\n");
    s.push_str(RUST_REGISTRATION_IMPORTS);
    let mut defs: Vec<String> = Vec::new();
    let mut any_params = false;
    for c in &lib.components {
        let Some(item) = m.item(&c.item) else {
            continue;
        };
        any_params |= !item.params.is_empty();
        let item_fn = item.name.snake();
        let sn = Ident::from_text(&c.name).snake();
        let defaults: Vec<String> = item.params.iter().map(|p| format!("{:?}", p.default_text())).collect();
        s.push_str(&format!(
            "\n/// `{}:{}` with its default arguments.\npub fn {item_fn}_default() -> Dom {{\n    \
             {item_fn}({})\n}}\n",
            lib.name,
            c.name,
            defaults.join(", ")
        ));
        s.push_str(&format!(
            "\nextern \"C\" fn {sn}_render_fn(\n    _def: &ComponentDef,\n    model: \
             &ComponentDataModel,\n    _map: &ComponentMap,\n) -> ResultStyledDomRenderDomError \
             {{\n"
        ));
        let mut args: Vec<String> = Vec::new();
        if item.params.is_empty() {
            s.push_str("    let _ = model;\n");
        }
        for p in &item.params {
            let local = format!("arg_{}", p.name.snake());
            s.push_str(&format!(
                "    let {local} = model_string(model, {:?}, {:?});\n",
                p.name.snake(),
                p.default_text()
            ));
            args.push(format!("&{local}"));
        }
        s.push_str(&format!(
            "    ResultStyledDomRenderDomError::Ok(StyledDom::create_from_dom({item_fn}({})))\n}}\n",
            args.join(", ")
        ));
        s.push_str(&format!(
            "\nextern \"C\" fn {sn}_compile_fn(\n    _def: &ComponentDef,\n    _target: \
             &CompileTarget,\n    _model: &ComponentDataModel,\n    _indent: usize,\n) -> \
             ResultStringCompileError {{\n    ResultStringCompileError::Ok({})\n}}\n",
            az_str(&format!("{item_fn}_default()"))
        ));
        let fields = if item.params.is_empty() {
            "ComponentDataFieldVec::create()".to_string()
        } else {
            let items: String = item
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    format!(
                        "                string_field({:?}, {:?}, {:?}),\n",
                        p.name.snake(),
                        p.default_text(),
                        c.field_descriptions.get(i).map_or("", String::as_str)
                    )
                })
                .collect();
            format!("vec![\n{items}            ]\n            .into()")
        };
        s.push_str(&format!(
            "\nfn {sn}_def() -> ComponentDef {{\n    ComponentDef {{\n        id: \
             ComponentId::create({}, {}),\n        display_name: {},\n        description: {},\n   \
             \x20    // The CSS is applied per node by {item_fn}.\n        css: {},\n        \
             source: ComponentSource::UserDefined,\n        data_model: ComponentDataModel {{\n    \
             \x20       name: {},\n            description: {},\n            fields: {fields},\n   \
             \x20    }},\n        render_fn: {sn}_render_fn,\n        compile_fn: \
             {sn}_compile_fn,\n        render_fn_source: OptionString::None,\n        \
             compile_fn_source: OptionString::None,\n    }}\n}}\n",
            az_str(&lib.name),
            az_str(&c.name),
            az_str(&c.display_name),
            az_str(&c.description),
            az_str(""),
            az_str(&c.data_model),
            az_str(&c.data_model_description),
        ));
        defs.push(format!("{sn}_def()"));
    }
    s.push_str(&format!(
        "\n/// The component library `{}`:\n/// `config.add_component_library({:?}, \
         register_{lsn}_library);`\npub extern \"C\" fn register_{lsn}_library() -> \
         ComponentLibrary {{\n    ComponentLibrary {{\n        name: {},\n        version: {},\n \
         \x20      description: {},\n        components: {},\n        exportable: true,\n        \
         modifiable: false,\n        data_models: ComponentDataModelVec::create(),\n        \
         enum_models: ComponentEnumModelVec::create(),\n    }}\n}}\n",
        lib.name,
        lib.name,
        az_str(&lib.name),
        az_str(&lib.version),
        az_str("Exported from AzBuilder"),
        if defs.is_empty() {
            "Vec::<ComponentDef>::new().into()".to_string()
        } else {
            format!("vec![{}].into()", defs.join(", "))
        },
    ));
    if any_params {
        s.push_str(RUST_REGISTRATION_HELPERS);
    }
    s
}

/// The app around a DOM module (`m.app`).
fn rust_app_main(m: &Module) -> String {
    let Some(app) = &m.app else {
        return String::new();
    };
    let root = app.root.snake();
    let body = if app.is_body {
        format!("ui::{root}()")
    } else {
        format!("Dom::create_body().with_child(ui::{root}())")
    };
    format!(
        "//! {} - generated by AzBuilder (azul-css codegen, Rust).\n\nmod ui;\n\nuse \
         azul::prelude::*;\n\nstruct AppData {{}}\n\nextern \"C\" fn layout(_data: RefAny, _info: \
         LayoutCallbackInfo) -> Dom {{\n    {body}\n}}\n\nfn main() {{\n    let app = \
         App::create(RefAny::new(AppData {{}}), AppConfig::create());\n    let mut window = \
         WindowCreateOptions::create(layout);\n    window.window_state.title = {};\n    \
         app.run(window);\n}}\n",
        app.title.replace(|c: char| c == '\n' || c == '\r', " "),
        az_str(&app.title),
    )
}

impl CodegenBackend for Rust {
    fn lang(&self) -> &'static str {
        "rust"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn display_name(&self) -> &'static str {
        "Rust"
    }

    fn extension(&self) -> &'static str {
        "rs"
    }

    fn emit_module(&self, m: &Module) -> String {
        let mut out = String::from("// Generated by azul-css codegen (Rust). Do not edit by hand.\n\n");
        for module in used_api_modules(m) {
            if module == "str" {
                continue; // strings are spelled `azul::str::String::from(..)`
            }
            out.push_str(&format!("#[allow(unused_imports)]\nuse azul::{module}::*;\n"));
        }
        for item in &m.items {
            out.push('\n');
            out.push_str(&item_fn(item));
        }
        if let Some(lib) = &m.library {
            out.push_str(&rust_registration(m, lib));
        }
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        if m.app.is_some() {
            return vec![
                GeneratedFile {
                    path: "Cargo.toml".to_string(),
                    contents: format!(
                        "[package]\nname = \"azul-app\"\nversion = \"0.1.0\"\nedition = \
                         \"2021\"\n\n[dependencies]\n# The bindings are served from the azul.rs \
                         registry (see .cargo/config.toml).\n# Link against the prebuilt library: \
                         export AZ_LINK_PATH=/path/to/libazul.so\nazul = {{ version = \
                         \"{AZUL_VERSION}\", registry = \"azul\" }}\n"
                    ),
                },
                GeneratedFile {
                    path: ".cargo/config.toml".to_string(),
                    contents: "[registries]\nazul = { index = \"sparse+https://azul.rs/ui/cargo/\" }\n"
                        .to_string(),
                },
                GeneratedFile {
                    path: "src/ui.rs".to_string(),
                    contents: self.emit_module(m),
                },
                GeneratedFile {
                    path: "src/main.rs".to_string(),
                    contents: rust_app_main(m),
                },
            ];
        }
        let mut main = String::from("mod styles;\n\nfn main() {\n");
        for item in &m.items {
            let name = item.name.snake();
            if item.ty.ends_with("Vec") {
                main.push_str(&format!(
                    "    let {name} = styles::{name}();\n    println!(\"{name}: {{}} properties\", {name}.len());\n"
                ));
            } else {
                main.push_str(&format!(
                    "    let _{name} = styles::{name}();\n    println!(\"{name}: built\");\n"
                ));
            }
        }
        main.push_str("}\n");
        vec![
            GeneratedFile {
                path: "Cargo.toml".to_string(),
                contents: format!(
                    "[package]\nname = \"azul-styles\"\nversion = \"0.1.0\"\nedition = \
                     \"2021\"\n\n[dependencies]\n# The bindings are served from the azul.rs \
                     registry (see .cargo/config.toml).\n# Link against the prebuilt library: \
                     export AZ_LINK_PATH=/path/to/libazul.so\nazul = {{ version = \
                     \"{AZUL_VERSION}\", registry = \"azul\" }}\n"
                ),
            },
            GeneratedFile {
                path: ".cargo/config.toml".to_string(),
                contents: "[registries]\nazul = { index = \"sparse+https://azul.rs/ui/cargo/\" }\n"
                    .to_string(),
            },
            GeneratedFile {
                path: "src/styles.rs".to_string(),
                contents: self.emit_module(m),
            },
            GeneratedFile {
                path: "src/main.rs".to_string(),
                contents: main,
            },
        ]
    }
}
