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
//!
//! A DOM goes through the wrapper layer (`lang_go/wrappers.rs`): factories
//! `azul.<Class><Method>` (`azul.DomCreateDiv()`, `azul.SmallAriaInfoLabel(..)`),
//! by-value `self` methods that consume the receiver and return a new
//! `*azul.Dom` (`.WithChild(..)`), `*azul.String` arguments from
//! `azul.Str(..)`. A chain puts the dot at the END of a line: Go inserts a
//! semicolon after a line that ends in `)`. An app is
//! `azul.AppCreate(&appData{}, azul.AppConfigCreate())` +
//! `azul.WindowCreateOptionsCreate(azul.Bind(layout))`.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::codegen::{
    doc::{render, Doc},
    ir::{snake_to_lower_camel, snake_to_upper_camel, EnumShape, Ident, Item, Module, Prim},
    lang::{
        dom::{
            chained_dot_at_line_end, is_dom_item, one_line, registration_note, WrapperDom,
            WrapperDomSyntax,
        },
        escape_quoted, item_comments, item_doc, unicode_u4, uses_nonfinite_float, ConcatPart,
        ExprSyntax, MethodLayout,
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

/// Go keywords, and the names a parameter must not shadow (`azul` is the
/// bindings' package, `string` the parameter type).
const RESERVED: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range",
    "return", "select", "struct", "switch", "type", "var", "azul", "string",
];

/// A parameter's Go name.
fn go_param(name: &Ident) -> String {
    let n = name.lower_camel();
    if RESERVED.contains(&n.as_str()) {
        format!("{n}_")
    } else {
        n
    }
}

/// A Go string literal.
fn go_str(s: &str) -> String {
    format!("\"{}\"", escape_quoted(s, &[], &unicode_u4))
}

/// The Go `string` joined from `parts` (`"by " + author`; `""` for none).
fn go_string(parts: &[ConcatPart<'_>]) -> String {
    if parts.is_empty() {
        return go_str("");
    }
    parts
        .iter()
        .map(|p| match p {
            ConcatPart::Lit(s) => go_str(s),
            ConcatPart::Param(i) => go_param(i),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// `azul.Str(x)`: the `*azul.String` the wrapper methods take, from the Go
/// `string` expression `x`.
fn azul_str(x: &str) -> Doc {
    Doc::text(format!("azul.Str({x})"))
}

/// The DOM through the wrapper layer (`*azul.Dom`, `*azul.SmallAriaInfo`).
#[derive(Debug, Copy, Clone, Default)]
struct GoDom;

impl WrapperDomSyntax for GoDom {
    fn base(&self) -> &dyn ExprSyntax {
        &Go
    }

    fn native_string(&self, s: &str) -> Doc {
        azul_str(&go_str(s))
    }

    fn factory(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        go_call(format!("azul.{class}{}", snake_to_upper_camel(method)), args, broken)
    }

    fn method(&self, recv: Doc, _class: &str, method: &str, args: Vec<Doc>, layout: MethodLayout) -> Doc {
        chained_dot_at_line_end(recv, go_call(snake_to_upper_camel(method), args, layout.args_tall))
    }

    fn param(&self, name: &Ident) -> Doc {
        azul_str(&go_param(name))
    }

    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc {
        azul_str(&go_string(parts))
    }

    fn item_call_limitation(&self) -> Option<&'static str> {
        None
    }

    /// `RenderCard("Hi", title)`: another function of the package (named
    /// like `dom_item_fn` names it).
    fn item_call(&self, item: &Ident, _params: &[Ident], args: Vec<Doc>, broken: bool) -> Doc {
        go_call(item.upper_camel(), args, broken)
    }

    /// A DOM item takes Go `string`s: the plain string, no `azul.Str(..)`.
    fn string_arg(&self, parts: &[ConcatPart<'_>]) -> Doc {
        Doc::text(go_string(parts))
    }
}

/// A DOM item: a function taking its parameters as Go strings.
fn dom_item_fn(item: &Item) -> String {
    let syntax = WrapperDom(GoDom);
    let mut out = String::new();
    for line in &item_comments(&syntax, item) {
        out.push_str(&format!("// {line}\n"));
    }
    let name = item.name.upper_camel();
    let params = item
        .params
        .iter()
        .map(|p| format!("{} string", go_param(&p.name)))
        .collect::<Vec<_>>()
        .join(", ");
    match item_doc(&syntax, item) {
        Ok(doc) => out.push_str(&format!(
            "func {name}({params}) *azul.Dom {{\n\treturn {}\n}}\n",
            render(&doc, "\t", 1)
        )),
        Err(reason) => out.push_str(&format!(
            "// not expressible with the Go bindings: {reason}\nfunc {name}({params}) *azul.Dom \
             {{\n\treturn nil\n}}\n"
        )),
    }
    out
}

/// Why the Go bindings cannot register a component library.
const NO_REGISTRATION: &str = "the Go bindings cannot make one: a purego callback returns only \
                               an integer, a pointer or a bool (never a \
                               ResultStyledDomRenderDomError by value), and ComponentDef / \
                               ComponentLibrary have no constructor";

/// The app's `main` around a DOM module (`m.app`) in package `ui`. The
/// wrapper layer sets no window title (it is a raw `window_state` field).
fn app_main(m: &Module) -> String {
    let Some(app) = &m.app else {
        return String::new();
    };
    let root = format!("ui.{}()", app.root.upper_camel());
    let body = if app.is_body {
        root
    } else {
        format!("azul.DomCreateBody().WithChild({root})")
    };
    format!(
        "// {} - generated by AzBuilder (azul-css codegen, Go).\npackage main\n\nimport (\n\tazul \
         \"azul.rs/ui/go\"\n\n\t\"azul-app/ui\"\n)\n\n// The app's data: the layout callback gets \
         it back.\ntype appData struct{{}}\n\nfunc layout(_ *appData, _ *azul.LayoutCallbackInfo) \
         *azul.Dom {{\n\treturn {body}\n}}\n\nfunc main() {{\n\tif err := azul.LoadLibrary(\"\"); \
         err != nil {{\n\t\tpanic(err)\n\t}}\n\twindow := \
         azul.WindowCreateOptionsCreate(azul.Bind(layout))\n\tapp := azul.AppCreate(&appData{{}}, \
         azul.AppConfigCreate())\n\tapp.Run(window)\n}}\n",
        one_line(&app.title)
    )
}

/// The `go.mod` of a project named `module`.
fn go_mod(module: &str) -> String {
    format!(
        "module {module}\n\ngo 1.21\n\nrequire azul.rs/ui/go v0.0.0\n\nrequire \
         github.com/ebitengine/purego v0.10.2 // indirect\n\n// Copy target/codegen/go/ to \
         ./azul-go (and libazul next to the binary).\nreplace azul.rs/ui/go => ./azul-go\n"
    )
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

    fn exports_dom(&self) -> bool {
        true
    }

    fn emit_module(&self, m: &Module) -> String {
        let package = if m.is_dom() { "ui" } else { "styles" };
        let mut out = format!(
            "// Generated by azul-css codegen (Go). Do not edit by hand.\npackage {package}\n\n"
        );
        if uses_nonfinite_float(m) {
            out.push_str("import (\n\t\"math\"\n\n\tazul \"azul.rs/ui/go\"\n)\n");
        } else {
            out.push_str("import azul \"azul.rs/ui/go\"\n");
        }
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
                out.push_str(&format!("// {line}\n"));
            }
        }
        out
    }

    fn emit_project_files(&self, m: &Module) -> Vec<GeneratedFile> {
        if m.app.is_some() {
            return vec![
                GeneratedFile {
                    path: "go.mod".to_string(),
                    contents: go_mod("azul-app"),
                },
                GeneratedFile {
                    path: "ui/ui.go".to_string(),
                    contents: self.emit_module(m),
                },
                GeneratedFile {
                    path: "main.go".to_string(),
                    contents: app_main(m),
                },
            ];
        }
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
                contents: go_mod("azul-styles"),
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
