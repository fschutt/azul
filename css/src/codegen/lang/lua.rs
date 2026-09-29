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

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::codegen::{
    doc::{render, Doc},
    ir::{snake_to_lower_camel, EnumShape, Item, Module, Prim},
    lang::{
        escape_quoted, item_comments, item_doc, unicode_braced, variant_ctor_method, ExprSyntax,
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

    fn emit_module(&self, m: &Module) -> String {
        let mut out = String::from(
            "-- Generated by azul-css codegen (Lua). Do not edit by hand.\nlocal azul = \
             require('azul')\nlocal ffi = require('ffi')\nlocal C = azul.C\n\nlocal M = {}\n",
        );
        for item in &m.items {
            out.push('\n');
            out.push_str(&item_fn(item));
        }
        out.push_str("\nreturn M\n");
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
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
