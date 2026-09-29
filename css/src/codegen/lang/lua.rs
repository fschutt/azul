//! Lua (LuaJIT FFI): the `azul` module (`target/codegen/azul.lua`, generated
//! by `doc/src/codegen/v2/lang_lua`).
//!
//! The generated code drives the raw C library the module exports as
//! `azul.C` (`C.AzCssProperty_textColor(..)`, variant constructors
//! `C.AzLayoutWidth_px(..)`, enum constants `C.AzLayoutDisplay_Flex`), builds
//! structs and unions with `ffi.new` table initialisers
//! (`ffi.new('AzColorU', { r = 255, .. })`,
//! `ffi.new('AzLayoutWidthValue', { Exact = { tag = 6, payload = .. } })`),
//! a Vec from a C array (`C.AzXxxVec_copyFromPtr(ffi.new('AzXxx[2]', { a, b }), 2)`)
//! and strings with the unarmed `azul._az_string` (the C call consumes it).
//!
//! A DOM goes through the wrapper layer instead (`lang_lua/wrappers.rs`):
//! static factories under the api snake_case name (`azul.Dom.create_div()`,
//! `azul.SmallAriaInfo.label(..)`), by-value `self` methods called with `:`
//! that consume the receiver and return a new `AzDom` (`:with_child(..)`,
//! chained one link per line), Lua string arguments (the wrappers convert
//! them) and `..` for a joined text. A DOM module needs neither `ffi` nor
//! `C`. An app is `azul.WindowCreateOptions.create(layout)` with a layout
//! function, the title through `:with({ window_state = { title = .. } })`,
//! and `azul.App.create(app_data, azul.AppConfig.create())` +
//! `app:run(window)`.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::codegen::{
    doc::{render, Doc},
    ir::{snake_to_lower_camel, EnumShape, Ident, Item, Module, Prim},
    lang::{
        dom::{is_dom_item, one_line, registration_note, WrapperDom, WrapperDomSyntax},
        escape_quoted, item_comments, item_doc, unicode_braced, variant_ctor_method, ConcatPart,
        ExprSyntax, MethodLayout,
    },
    lower_types::union_tag,
    CodegenBackend, GeneratedFile,
};

/// The Lua printer.
#[derive(Debug, Copy, Clone, Default)]
pub struct Lua;

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

fn key(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("['{name}'] = ")
    } else {
        format!("{name} = ")
    }
}

impl ExprSyntax for Lua {
    fn int(&self, value: i128, _ty: Prim) -> String {
        value.to_string()
    }

    fn float(&self, text: &str, _ty: Prim) -> String {
        match text {
            "nan" => "(0 / 0)".to_string(),
            "inf" => "math.huge".to_string(),
            "-inf" => "(-math.huge)".to_string(),
            _ => text.to_string(),
        }
    }

    fn boolean(&self, b: bool) -> String {
        b.to_string()
    }

    fn string(&self, s: &str) -> Doc {
        Doc::text(format!(
            "azul._az_string(\"{}\")",
            escape_quoted(s, &[], &unicode_braced)
        ))
    }

    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        Doc::call(format!("C.Az{class}_{}", snake_to_lower_camel(method)), args, broken)
    }

    fn variant(&self, ty: &str, shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc {
        match shape {
            EnumShape::CLike => Doc::text(format!("C.Az{ty}_{variant}")),
            EnumShape::Tagged => Doc::call(
                format!("C.Az{ty}_{}", variant_ctor_method(variant)),
                args,
                broken,
            ),
            EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                let tag = union_tag(ty, variant).unwrap_or(0);
                let mut inner = vec![Doc::text(format!("tag = {tag}"))];
                if let Some(payload) = args.into_iter().next() {
                    inner.push(Doc::cat(vec![Doc::text("payload = "), payload]));
                }
                Doc::call(
                    "ffi.new",
                    vec![
                        Doc::text(format!("'Az{ty}'")),
                        Doc::list(
                            "{",
                            vec![Doc::cat(vec![
                                Doc::text(format!("{variant} = ")),
                                Doc::list("{", inner, ", ", "}", true, broken, false),
                            ])],
                            ", ",
                            "}",
                            true,
                            broken,
                            false,
                        ),
                    ],
                    broken,
                )
            }
        }
    }

    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc {
        let fields = fields
            .into_iter()
            .map(|(k, v)| Doc::cat(vec![Doc::text(key(&k)), v]))
            .collect();
        Doc::call(
            "ffi.new",
            vec![
                Doc::text(format!("'Az{ty}'")),
                Doc::list("{", fields, ", ", "}", true, broken, true),
            ],
            broken,
        )
    }

    fn vec(&self, ty: &str, elem: &str, items: Vec<Doc>, broken: bool) -> Doc {
        if items.is_empty() {
            return Doc::text(format!("C.Az{ty}_create()"));
        }
        let n = items.len();
        Doc::cat(vec![
            Doc::text(format!("C.Az{ty}_copyFromPtr(ffi.new('Az{elem}[{n}]', ")),
            Doc::list("{", items, ", ", "}", true, broken, true),
            Doc::text(format!("), {n})")),
        ])
    }

    fn unsupported(&self, what: &str) -> Doc {
        Doc::text(format!("error({:?})", format!("not expressible: {what}")))
    }
}

/// A method name as the generator spells it (`sanitize_lua_ident` in
/// `lang_lua/wrappers.rs`: a keyword gets a trailing `_`).
fn lua_method(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// A parameter's Lua name: a keyword, or `azul` (the bindings' module
/// local the body uses), gets a trailing `_`.
fn lua_param(name: &Ident) -> String {
    let n = name.snake();
    if KEYWORDS.contains(&n.as_str()) || n == "azul" {
        format!("{n}_")
    } else {
        n
    }
}

/// A double-quoted Lua string.
fn lua_str(s: &str) -> String {
    format!("\"{}\"", escape_quoted(s, &[], &unicode_braced))
}

/// The DOM through the wrapper layer (`azul.Dom`, `azul.SmallAriaInfo`).
#[derive(Debug, Copy, Clone, Default)]
struct LuaDom;

impl WrapperDomSyntax for LuaDom {
    fn base(&self) -> &dyn ExprSyntax {
        &Lua
    }

    fn native_string(&self, s: &str) -> Doc {
        Doc::text(lua_str(s))
    }

    fn factory(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        Doc::call(format!("azul.{class}.{}", lua_method(method)), args, broken)
    }

    fn method(&self, recv: Doc, _class: &str, method: &str, args: Vec<Doc>, layout: MethodLayout) -> Doc {
        Doc::chained(
            recv,
            Doc::call(format!(":{}", lua_method(method)), args, layout.args_tall),
        )
    }

    fn param(&self, name: &Ident) -> Doc {
        Doc::text(lua_param(name))
    }

    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc {
        Doc::text(
            parts
                .iter()
                .map(|p| match p {
                    ConcatPart::Lit(s) => lua_str(s),
                    ConcatPart::Param(i) => lua_param(i),
                })
                .collect::<Vec<_>>()
                .join(" .. "),
        )
    }

    fn item_call_limitation(&self) -> Option<&'static str> {
        None
    }

    /// `M.render_card("Hi", title)`: another function of the module table
    /// (named like `dom_item_fn` names it).
    fn item_call(&self, item: &Ident, _params: &[Ident], args: Vec<Doc>, broken: bool) -> Doc {
        Doc::call(format!("M.{}", item.snake()), args, broken)
    }
}

/// A DOM item: a module function taking Lua strings.
fn dom_item_fn(item: &Item) -> String {
    let syntax = WrapperDom(LuaDom);
    let mut out = String::new();
    for line in &item_comments(&syntax, item) {
        out.push_str(&format!("-- {line}\n"));
    }
    let name = item.name.snake();
    let params = item
        .params
        .iter()
        .map(|p| lua_param(&p.name))
        .collect::<Vec<_>>()
        .join(", ");
    match item_doc(&syntax, item) {
        Ok(doc) => out.push_str(&format!(
            "function M.{name}({params})\n    return {}\nend\n",
            render(&doc, "    ", 1)
        )),
        Err(reason) => out.push_str(&format!(
            "-- not expressible with the Lua bindings: {reason}\nfunction M.{name}({params})\n    \
             return nil\nend\n"
        )),
    }
    out
}

/// Why the Lua bindings cannot register a component library.
const NO_REGISTRATION: &str = "the Lua bindings cannot make one: their cdef declares that field \
                               void*, no FFI callback can return the tagged union \
                               ResultStyledDomRenderDomError by value (a LuaJIT callback returns \
                               only void, a number, an enum or a pointer; cffi-lua has no union \
                               by value), no host invoker covers ComponentRenderFn, and \
                               ComponentDef / ComponentLibrary have no constructor";

/// The app's `main.lua` around a DOM module (`m.app`) in `ui.lua`.
fn app_main(m: &Module) -> String {
    let Some(app) = &m.app else {
        return String::new();
    };
    let root = format!("ui.{}()", app.root.snake());
    let body = if app.is_body {
        root
    } else {
        format!("azul.Dom.create_body():with_child({root})")
    };
    format!(
        "-- {} - generated by AzBuilder (azul-css codegen, Lua).\n-- Copy target/codegen/azul.lua \
         and libazul next to this file, then: luajit main.lua\nlocal azul = require(\"azul\")\nlocal \
         ui = require(\"ui\")\n\n-- The app's data: the layout callback gets it back.\nlocal \
         app_data = {{}}\n\nlocal function layout(_data, _info)\n    return {body}\nend\n\nlocal \
         window = azul.WindowCreateOptions.create(layout):with({{ window_state = {{ title = {} }} \
         }})\nlocal app = azul.App.create(app_data, azul.AppConfig.create())\napp:run(window)\n",
        one_line(&app.title),
        lua_str(&app.title)
    )
}

fn item_fn(item: &Item) -> String {
    let mut out = String::new();
    for line in &item_comments(&Lua, item) {
        out.push_str(&format!("-- {line}\n"));
    }
    let name = item.name.snake();
    match item_doc(&Lua, item) {
        Ok(doc) => out.push_str(&format!(
            "function M.{name}()\n    return {}\nend\n",
            render(&doc, "    ", 1)
        )),
        Err(reason) => out.push_str(&format!(
            "-- not expressible with the Lua bindings: {reason}\nfunction M.{name}()\n    return \
             nil\nend\n"
        )),
    }
    out
}

impl CodegenBackend for Lua {
    fn lang(&self) -> &'static str {
        "lua"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["luajit"]
    }

    fn display_name(&self) -> &'static str {
        "Lua"
    }

    fn extension(&self) -> &'static str {
        "lua"
    }

    fn exports_dom(&self) -> bool {
        true
    }

    fn emit_module(&self, m: &Module) -> String {
        // `ffi` and `C` serve CSS values only.
        let css = !m.is_dom() || m.items.iter().any(|i| !is_dom_item(i));
        let mut out = String::from(
            "-- Generated by azul-css codegen (Lua). Do not edit by hand.\nlocal azul = \
             require('azul')\n",
        );
        if css {
            out.push_str("local ffi = require('ffi')\nlocal C = azul.C\n");
        }
        out.push_str("\nlocal M = {}\n");
        for item in &m.items {
            out.push('\n');
            if is_dom_item(item) {
                out.push_str(&dom_item_fn(item));
            } else {
                out.push_str(&item_fn(item));
            }
        }
        if let Some(lib) = &m.library {
            out.push('\n');
            for line in registration_note(lib, NO_REGISTRATION) {
                out.push_str(&format!("-- {line}\n"));
            }
        }
        out.push_str("\nreturn M\n");
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        if m.app.is_some() {
            return vec![
                GeneratedFile {
                    path: "ui.lua".to_string(),
                    contents: self.emit_module(m),
                },
                GeneratedFile {
                    path: "main.lua".to_string(),
                    contents: app_main(m),
                },
            ];
        }
        let mut main = String::from(
            "-- Copy target/codegen/azul.lua and libazul next to this file, then: luajit \
             main.lua\nlocal styles = require('styles')\n",
        );
        for item in &m.items {
            let name = item.name.snake();
            main.push_str(&format!(
                "\nlocal {name} = styles.{name}()\nprint(('{name}: %d properties'):format(tonumber({name}.len)))\n"
            ));
        }
        vec![
            GeneratedFile {
                path: "styles.lua".to_string(),
                contents: self.emit_module(m),
            },
            GeneratedFile {
                path: "main.lua".to_string(),
                contents: main,
            },
        ]
    }
}
