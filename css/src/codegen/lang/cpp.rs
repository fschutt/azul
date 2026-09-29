//! C++17: the C API of `azul.h`, as pulled in by `azul17.hpp`.
//!
//! The `azul::` wrapper classes of `azul17.hpp` mix raw C returns
//! (`azul::CssProperty::width` returns an `AzCssProperty`) with move-only
//! RAII classes (`azul::CssPropertyWithConditions`), which a nested
//! expression cannot combine without `.release()` calls everywhere. The
//! generated code therefore calls the C API directly (valid, idiomatic C++
//! with the same names as C), and replaces the C99-only constructs:
//! aggregate initialisation `AzColorU{ 255, 0, 0, 255 }` for struct literals
//! (positional, C++17 has no designated initialisers), an immediately
//! invoked lambda for a union member (`[]{ AzLayoutWidthValue v{}; v.Exact =
//! ..; return v; }()`), `std::vector<AzXxx>{ .. }.data()` for the array a Vec
//! is copied from.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use super::c::C;
use crate::codegen::{
    doc::{render, Doc},
    ir::{EnumShape, Item, Module, Prim},
    lang::{c_escape, expr_doc_top, item_comments, uses_nonfinite_float, ExprSyntax},
    CodegenBackend, GeneratedFile,
};

/// The C++ printer.
#[derive(Debug, Copy, Clone, Default)]
pub struct Cpp;

impl ExprSyntax for Cpp {
    fn int(&self, value: i128, ty: Prim) -> String {
        C.int(value, ty)
    }

    fn float(&self, text: &str, ty: Prim) -> String {
        let t = if ty == Prim::F64 { "double" } else { "float" };
        match text {
            "nan" => format!("std::numeric_limits<{t}>::quiet_NaN()"),
            "inf" => format!("std::numeric_limits<{t}>::infinity()"),
            "-inf" => format!("-std::numeric_limits<{t}>::infinity()"),
            _ => C.float(text, ty),
        }
    }

    fn boolean(&self, b: bool) -> String {
        b.to_string()
    }

    fn string(&self, s: &str) -> Doc {
        Doc::text(format!(
            "AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(\"{}\"), 0, {})",
            c_escape(s),
            s.len()
        ))
    }

    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        C.call(class, method, args, broken)
    }

    fn variant(&self, ty: &str, shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc {
        match shape {
            EnumShape::CLike | EnumShape::Tagged => C.variant(ty, shape, variant, args, broken),
            EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                let mut fields = vec![Doc::text(format!("Az{ty}_Tag_{variant}"))];
                fields.extend(args);
                Doc::cat(vec![
                    Doc::text(format!("[]{{ Az{ty} v{{}}; v.{variant} = Az{ty}Variant_{variant}")),
                    Doc::list("{", fields, ", ", "}", true, broken, false),
                    Doc::text("; return v; }()"),
                ])
            }
        }
    }

    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc {
        Doc::cat(vec![
            Doc::text(format!("Az{ty}")),
            Doc::list(
                "{",
                fields.into_iter().map(|(_, v)| v).collect(),
                ", ",
                "}",
                true,
                broken,
                true,
            ),
        ])
    }

    fn vec(&self, ty: &str, elem: &str, items: Vec<Doc>, broken: bool) -> Doc {
        if items.is_empty() {
            return Doc::text(format!("Az{ty}_create()"));
        }
        let n = items.len();
        Doc::cat(vec![
            Doc::text(format!("Az{ty}_copyFromPtr")),
            Doc::list(
                "(",
                vec![
                    Doc::cat(vec![
                        Doc::text(format!("std::vector<Az{elem}>")),
                        Doc::list("{", items, ", ", "}", true, broken, true),
                        Doc::text(".data()"),
                    ]),
                    Doc::text(n.to_string()),
                ],
                ", ",
                ")",
                false,
                broken,
                false,
            ),
        ])
    }

    fn unsupported(&self, what: &str) -> Doc {
        Doc::text(format!("/* unsupported: {} */ {{}}", super::block_comment_safe(what)))
    }
}

fn item_fn(item: &Item) -> String {
    let mut out = String::new();
    for line in &item_comments(&Cpp, item) {
        out.push_str(&format!("// {line}\n"));
    }
    let body = render(&expr_doc_top(&Cpp, &item.value), "    ", 1);
    out.push_str(&format!(
        "inline Az{} {}() {{\n    return {body};\n}}\n",
        item.ty,
        item.name.snake()
    ));
    out
}

impl CodegenBackend for Cpp {
    fn lang(&self) -> &'static str {
        "cpp"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["c++", "cxx", "cpp17"]
    }

    fn display_name(&self) -> &'static str {
        "C++"
    }

    fn extension(&self) -> &'static str {
        "hpp"
    }

    fn emit_module(&self, m: &Module) -> String {
        let mut out = String::from(
            "// Generated by azul-css codegen (C++17). Do not edit by hand.\n#pragma once\n",
        );
        if uses_nonfinite_float(m) {
            out.push_str("#include <limits>\n");
        }
        out.push_str("#include <vector>\n#include \"azul17.hpp\"\n");
        for item in &m.items {
            out.push('\n');
            out.push_str(&item_fn(item));
        }
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        let mut main = String::from("#include <cstdio>\n#include \"styles.hpp\"\n\nint main() {\n");
        for item in &m.items {
            let name = item.name.snake();
            main.push_str(&format!("    Az{} {name}_value = {name}();\n", item.ty));
            if item.ty.ends_with("Vec") {
                main.push_str(&format!(
                    "    std::printf(\"{name}: %zu properties\\n\", {name}_value.len);\n"
                ));
            } else {
                main.push_str(&format!("    std::printf(\"{name}: built\\n\");\n"));
            }
            main.push_str(&format!("    Az{}_delete(&{name}_value);\n", item.ty));
        }
        main.push_str("    return 0;\n}\n");
        vec![
            GeneratedFile {
                path: "styles.hpp".to_string(),
                contents: self.emit_module(m),
            },
            GeneratedFile {
                path: "main.cpp".to_string(),
                contents: main,
            },
            GeneratedFile {
                path: "Makefile".to_string(),
                contents: "# AZUL_INCLUDE: the directory holding azul17.hpp + azul.h; AZUL_LIB: the \
                           one holding libazul\nAZUL_INCLUDE ?= .\nAZUL_LIB ?= .\n\napp: main.cpp \
                           styles.hpp\n\t$(CXX) -std=c++17 -o app main.cpp -I$(AZUL_INCLUDE) \
                           -L$(AZUL_LIB) -lazul\n"
                    .to_string(),
            },
        ]
    }
}
