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

use super::c::{c_param, C};
use crate::codegen::{
    doc::{render, Doc},
    ir::{EnumShape, Ident, Item, Module, Prim},
    lang::{
        c_escape,
        dom::{c_family_registration, uses_concat, uses_params},
        expr_doc_top, item_comments, uses_nonfinite_float, ConcatPart, ExprSyntax, MethodLayout,
    },
    CodegenBackend, GeneratedFile,
};

/// C++ keywords beyond C's a parameter name must not be.
const CPP_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "asm", "catch", "class", "concept", "constexpr", "consteval",
    "constinit", "decltype", "delete", "explicit", "export", "friend", "mutable", "namespace",
    "new", "noexcept", "not", "nullptr", "operator", "or", "private", "protected", "public",
    "requires", "template", "this", "throw", "try", "typeid", "typename", "using", "virtual",
    "xor", "std", "azul",
];

/// A parameter's C++ name (`const std::string&`).
fn cpp_param(name: &Ident) -> String {
    let s = c_param(name);
    if CPP_KEYWORDS.contains(&s.as_str()) {
        format!("{s}_")
    } else {
        s
    }
}

/// What an item parameter or a joined string turns into an `AzString`.
/// Guarded: an export writes several headers that may be included together.
const AZ_STRING_HELPER: &str = "
#ifndef AZ_CODEGEN_STRING
#define AZ_CODEGEN_STRING
// A std::string as an AzString (a fresh copy at every use: calls take their
// AzString by value).
inline AzString az_string(const std::string& s) {
    return AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s.data()), 0, s.size());
}
#endif
";

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

    fn dom_limitation(&self) -> Option<&'static str> {
        None
    }

    /// The C API, like every other node: `AzDom_withChild(recv, child)`.
    fn method(&self, recv: Doc, class: &str, method: &str, args: Vec<Doc>, layout: MethodLayout) -> Doc {
        C.method(recv, class, method, args, layout)
    }

    fn param(&self, name: &Ident) -> Doc {
        Doc::text(format!("az_string({})", cpp_param(name)))
    }

    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc {
        Doc::text(format!("az_string({})", string_sum(parts)))
    }

    fn item_call_limitation(&self) -> Option<&'static str> {
        None
    }

    /// `render_card("Hi", title)`: another function of the header (defined
    /// above it: the module lists callees first).
    fn item_call(&self, item: &Ident, _params: &[Ident], args: Vec<Doc>, broken: bool) -> Doc {
        Doc::call(item.snake(), args, broken)
    }

    /// A `const std::string&` argument: a literal, a parameter passed on, or
    /// a `std::string` sum.
    fn native_string(&self, parts: &[ConcatPart<'_>]) -> Doc {
        match parts {
            [] => Doc::text("\"\""),
            [ConcatPart::Lit(s)] => Doc::text(format!("\"{}\"", c_escape(s))),
            [ConcatPart::Param(p)] => Doc::text(cpp_param(p)),
            _ => Doc::text(string_sum(parts)),
        }
    }
}

/// `std::string("by ") + author`: a `std::string` joined from `parts`.
fn string_sum(parts: &[ConcatPart<'_>]) -> String {
    parts
        .iter()
        .map(|p| match p {
            ConcatPart::Lit(s) => format!("std::string(\"{}\")", c_escape(s)),
            ConcatPart::Param(i) => cpp_param(i),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn cpp_param_type(ty: &str) -> String {
    if ty == "String" {
        "const std::string&".to_string()
    } else {
        format!("Az{ty}")
    }
}

fn item_fn(item: &Item) -> String {
    let mut out = String::new();
    for line in &item_comments(&Cpp, item) {
        out.push_str(&format!("// {line}\n"));
    }
    let body = render(&expr_doc_top(&Cpp, &item.value), "    ", 1);
    let sig = item
        .params
        .iter()
        .map(|p| format!("{} {}", cpp_param_type(&p.ty), cpp_param(&p.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let unused: String = super::dom::unused_params(item)
        .iter()
        .map(|p| format!("    (void){};\n", cpp_param(&p.name)))
        .collect();
    out.push_str(&format!(
        "inline Az{} {}({sig}) {{\n{unused}    return {body};\n}}\n",
        item.ty,
        item.name.snake()
    ));
    out
}

fn cpp_az_str(s: &str) -> String {
    Cpp.string(s).flat()
}

/// The app around a DOM module (`m.app`), booted through the C++17 wrapper.
fn cpp_app_main(m: &Module, module_file: &str) -> String {
    let Some(app) = &m.app else {
        return String::new();
    };
    let root = app.root.snake();
    let body = if app.is_body {
        format!("{root}()")
    } else {
        format!("AzDom_withChild(AzDom_createBody(), {root}())")
    };
    format!(
        "// {title} - generated by AzBuilder (azul-css codegen, C++17).\n#include <utility>\n#include \
         \"{module_file}\"\n\nstruct AppData {{}};\n\nstatic azul::ffi::Dom \
         layout(azul::ffi::RefAny data, azul::ffi::LayoutCallbackInfo info) {{\n    azul::RefAny \
         adopted(data); // the callback owns its RefAny: release it\n    (void)info;\n    return \
         {body};\n}}\n\nint main() {{\n    azul::RefAny data = \
         azul::RefAny::create(AppData{{}});\n    azul::WindowCreateOptions window = \
         azul::WindowCreateOptions::create(layout);\n    azul::App app = \
         azul::App::create(std::move(data), azul::AppConfig::create());\n    \
         app.run(std::move(window));\n    return 0;\n}}\n",
        title = app.title.replace(|c: char| c == '\n' || c == '\r', " "),
    )
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

    fn exports_dom(&self) -> bool {
        true
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
        let strings = uses_params(m) || uses_concat(m);
        if strings {
            out.push_str("#include <string>\n");
        }
        if m.library.is_some() {
            out.push_str("#include <stdlib.h>\n#include <string.h>\n");
        }
        if strings {
            out.push_str(AZ_STRING_HELPER);
        }
        for item in &m.items {
            out.push('\n');
            out.push_str(&item_fn(item));
        }
        if let Some(lib) = &m.library {
            out.push_str(&c_family_registration(m, lib, &cpp_param, &cpp_az_str));
        }
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        if m.app.is_some() {
            return vec![
                GeneratedFile {
                    path: "ui.hpp".to_string(),
                    contents: self.emit_module(m),
                },
                GeneratedFile {
                    path: "main.cpp".to_string(),
                    contents: cpp_app_main(m, "ui.hpp"),
                },
                GeneratedFile {
                    path: "Makefile".to_string(),
                    contents: "# AZUL_INCLUDE: the directory holding azul17.hpp + azul.h; AZUL_LIB: \
                               the one holding libazul\nAZUL_INCLUDE ?= .\nAZUL_LIB ?= .\n\napp: \
                               main.cpp ui.hpp\n\t$(CXX) -std=c++17 -o app main.cpp -I$(AZUL_INCLUDE) \
                               -L$(AZUL_LIB) -lazul\n"
                        .to_string(),
                },
            ];
        }
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
