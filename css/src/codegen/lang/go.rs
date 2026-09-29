//! Go: the purego bindings `azul.rs/ui/go` (`target/codegen/go/`, generated
//! by `doc/src/codegen/v2/lang_go`).
//!
//! Spellings: the raw C functions keep their C names
//! (`azul.AzCssProperty_textColor(..)`, `azul.AzPixelValue_px(10)`); enum
//! variants are Go-side builders `azul.Az<Enum>_<Variant>(..)` (PascalCase
//! variant: `azul.AzLayoutWidth_Px(..)`, `azul.AzLayoutWidthValue_Exact(..)`,
//! unit `azul.AzLayoutWidth_Auto()`), the C variant constructors are not
//! bound; C-like enum values are typed constants without the `Az` prefix
//! (`azul.LayoutDisplay_Flex`); structs have PascalCase exported fields
//! (`azul.AzColorU{R: 255, ..}`); a Vec is copied from a slice
//! (`azul.AzXxxVec_copyFromPtr(&[]azul.AzXxx{..}[0], n)`); strings come from
//! `azul.Str("..").Raw()`. `azul.LoadLibrary("")` must run before any call,
//! so styles are functions, never package-level variables.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::codegen::{
    doc::{render, Doc},
    ir::{snake_to_lower_camel, snake_to_upper_camel, EnumShape, Item, Module, Prim},
    lang::{
        escape_quoted, item_comments, item_doc, unicode_u4, uses_nonfinite_float, ExprSyntax,
    },
    CodegenBackend, GeneratedFile,
};

/// The Go printer.
#[derive(Debug, Copy, Clone, Default)]
pub struct Go;

/// `f(a, b)` with gofmt's trailing comma when broken.
fn go_call(name: String, args: Vec<Doc>, broken: bool) -> Doc {
    Doc::cat(vec![
        Doc::text(name),
        Doc::list("(", args, ", ", ")", false, broken, true),
    ])
}

impl ExprSyntax for Go {
    fn int(&self, value: i128, _ty: Prim) -> String {
        value.to_string()
    }

    fn float(&self, text: &str, ty: Prim) -> String {
        let wrap = |s: &str| {
            if ty == Prim::F64 {
                s.to_string()
            } else {
                format!("float32({s})")
            }
        };
        match text {
            "nan" => wrap("math.NaN()"),
            "inf" => wrap("math.Inf(1)"),
            "-inf" => wrap("math.Inf(-1)"),
            _ => text.to_string(),
        }
    }

    fn boolean(&self, b: bool) -> String {
        b.to_string()
    }

    fn string(&self, s: &str) -> Doc {
        Doc::text(format!(
            "azul.Str(\"{}\").Raw()",
            escape_quoted(s, &[], &unicode_u4)
        ))
    }

    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        go_call(
            format!("azul.Az{class}_{}", snake_to_lower_camel(method)),
            args,
            broken,
        )
    }

    fn variant(&self, ty: &str, shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc {
        match shape {
            EnumShape::CLike => Doc::text(format!("azul.{ty}_{variant}")),
            EnumShape::Tagged | EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                go_call(format!("azul.Az{ty}_{variant}"), args, broken)
            }
        }
    }

    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc {
        let fields = fields
            .into_iter()
            .map(|(k, v)| Doc::cat(vec![Doc::text(format!("{}: ", snake_to_upper_camel(&k))), v]))
            .collect();
        Doc::cat(vec![
            Doc::text(format!("azul.Az{ty}")),
            Doc::list("{", fields, ", ", "}", false, broken, true),
        ])
    }

    fn vec(&self, ty: &str, elem: &str, items: Vec<Doc>, broken: bool) -> Doc {
        if items.is_empty() {
            return Doc::text(format!("azul.Az{ty}_create()"));
        }
        let n = items.len();
        Doc::cat(vec![
            Doc::text(format!("azul.Az{ty}_copyFromPtr(&[]azul.Az{elem}")),
            Doc::list("{", items, ", ", "}", false, broken, true),
            Doc::text(format!("[0], {n})")),
        ])
    }

    fn unsupported(&self, what: &str) -> Doc {
        Doc::text(format!("panic({:?})", format!("not expressible: {what}")))
    }
}

fn item_fn(item: &Item) -> String {
    let mut out = String::new();
    for line in &item_comments(&Go, item) {
        out.push_str(&format!("// {line}\n"));
    }
    let name = item.name.upper_camel();
    match item_doc(&Go, item) {
        Ok(doc) => out.push_str(&format!(
            "func {name}() azul.Az{} {{\n\treturn {}\n}}\n",
            item.ty,
            render(&doc, "\t", 1)
        )),
        Err(reason) => out.push_str(&format!(
            "// not expressible with the Go bindings: {reason}\nfunc {name}() (v azul.Az{}) \
             {{\n\treturn\n}}\n",
            item.ty
        )),
    }
    out
}

impl CodegenBackend for Go {
    fn lang(&self) -> &'static str {
        "go"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["golang"]
    }

    fn display_name(&self) -> &'static str {
        "Go"
    }

    fn extension(&self) -> &'static str {
        "go"
    }

    fn emit_module(&self, m: &Module) -> String {
        let mut out = String::from(
            "// Generated by azul-css codegen (Go). Do not edit by hand.\npackage styles\n\n",
        );
        if uses_nonfinite_float(m) {
            out.push_str("import (\n\t\"math\"\n\n\tazul \"azul.rs/ui/go\"\n)\n");
        } else {
            out.push_str("import azul \"azul.rs/ui/go\"\n");
        }
        for item in &m.items {
            out.push('\n');
            out.push_str(&item_fn(item));
        }
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        let mut main = String::from(
            "package main\n\nimport (\n\t\"fmt\"\n\n\tazul \"azul.rs/ui/go\"\n\n\t\
             \"azul-styles/styles\"\n)\n\nfunc main() {\n\tif err := azul.LoadLibrary(\"\"); err \
             != nil {\n\t\tpanic(err)\n\t}\n",
        );
        for item in &m.items {
            let name = item.name.upper_camel();
            let var = item.name.lower_camel();
            main.push_str(&format!(
                "\t{var} := styles.{name}()\n\tfmt.Printf(\"{name}: %d properties\\n\", {var}.Len)\n"
            ));
        }
        main.push_str("}\n");
        vec![
            GeneratedFile {
                path: "go.mod".to_string(),
                contents: "module azul-styles\n\ngo 1.21\n\nrequire azul.rs/ui/go v0.0.0\n\n\
                           require github.com/ebitengine/purego v0.10.2 // indirect\n\n// Copy \
                           target/codegen/go/ to ./azul-go (and libazul next to the binary).\n\
                           replace azul.rs/ui/go => ./azul-go\n"
                    .to_string(),
            },
            GeneratedFile {
                path: "styles/styles.go".to_string(),
                contents: self.emit_module(m),
            },
            GeneratedFile {
                path: "main.go".to_string(),
                contents: main,
            },
        ]
    }
}
