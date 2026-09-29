//! What the DOM-exporting printers share (see the DOM section of
//! [`crate::codegen::ir`]): which parameters an item never reads, the C
//! string joiner, the component-library registration of the two printers
//! that write C API code (C and C++), the DOM syntax of the printers that
//! build a DOM through their binding's wrapper layer ([`WrapperDom`]), and
//! the note of a printer that cannot register a library
//! ([`registration_note`]).
//!
//! Everything language-specific stays in the printer files; this module only
//! holds text that is identical for the languages that use it.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt::Write;

use crate::codegen::{
    doc::Doc,
    ir::{EnumShape, Expr, Ident, Item, ItemParam, LibrarySpec, Module, Prim},
    lang::{block_comment_safe, ConcatPart, ExprSyntax, MethodLayout},
};

/// The parameters of `item` its value never reads (a placeholder in an
/// attribute no constructor takes): printers mark them used, so the
/// generated code compiles without warnings.
#[must_use]
pub fn unused_params(item: &Item) -> Vec<&ItemParam> {
    let mut used: Vec<&Ident> = Vec::new();
    item.value.walk(&mut |e| {
        if let Expr::Param(i) = e {
            used.push(i);
        }
    });
    item.params
        .iter()
        .filter(|p| !used.contains(&&p.name))
        .collect()
}

/// `true` if any item of the module has a [`Expr::Concat`].
#[must_use]
pub fn uses_concat(m: &Module) -> bool {
    super::module_any(m, &|e| matches!(e, Expr::Concat(_)))
}

/// `true` if any item of the module takes parameters.
#[must_use]
pub fn uses_params(m: &Module) -> bool {
    m.items.iter().any(|i| !i.params.is_empty())
}

/// `s` on one line (a window title or a library name in a line comment).
#[must_use]
pub fn one_line(s: &str) -> String {
    s.replace(|c: char| c == '\n' || c == '\r', " ")
}

/// The comment lines a printer prints instead of the registration of `lib`
/// when its bindings cannot build one (`reason`: why, without a final
/// period). The render functions above it stay usable.
#[must_use]
pub fn unregistered_library_note(lib: &LibrarySpec, reason: &str) -> Vec<String> {
    alloc::vec![
        format!(
            "The component library `{}` is not registered here: {reason}.",
            one_line(&lib.name)
        ),
        "Register it from the Rust, C or C++ export; the render functions above build its \
         components."
            .into(),
    ]
}

/// C: the helper an [`Expr::Concat`] calls (C has no string formatting in
/// the azul API). Needs `<stdarg.h>`, `<stdlib.h>`, `<string.h>`. Guarded:
/// an export writes several headers (the app, one per component library)
/// that may be included together.
pub const C_CONCAT_HELPER: &str = "
#ifndef AZ_CODEGEN_CONCAT
#define AZ_CODEGEN_CONCAT
/* Joins NUL-terminated strings (the last argument is NULL) into an AzString. */
static AzString az_concat(const char* first, ...) {
    size_t len = 0;
    va_list ap;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) len += strlen(s);
    va_end(ap);
    char* buf = (char*)malloc(len + 1);
    size_t at = 0;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) {
        size_t n = strlen(s);
        memcpy(buf + at, s, n);
        at += n;
    }
    va_end(ap);
    AzString out = AzString_copyFromBytes((const uint8_t*)buf, 0, len);
    free(buf);
    return out;
}
#endif
";

/// C / C++: what the registration calls (C-style casts, valid in both).
/// Guarded like [`C_CONCAT_HELPER`].
const C_REGISTRATION_HELPERS: &str = "
#ifndef AZ_CODEGEN_REGISTRATION
#define AZ_CODEGEN_REGISTRATION
/* The String value of the data-model field `name` as a NUL-terminated copy
 * (free() it), or a copy of `fallback`. */
static char* az_model_string(const AzComponentDataModel* model, const char* name, const char* fallback) {
    size_t name_len = strlen(name);
    const char* src = fallback;
    size_t len = strlen(fallback);
    for (size_t i = 0; i < model->fields.len; i++) {
        const AzComponentDataField* f = &model->fields.ptr[i];
        if (f->name.vec.len == name_len && memcmp(f->name.vec.ptr, name, name_len) == 0
            && f->default_value.Some.tag == AzOptionComponentDefaultValue_Tag_Some
            && f->default_value.Some.payload.String.tag == AzComponentDefaultValue_Tag_String) {
            const AzString* s = &f->default_value.Some.payload.String.payload;
            src = (const char*)s->vec.ptr;
            len = s->vec.len;
            break;
        }
    }
    char* out = (char*)malloc(len + 1);
    memcpy(out, src, len);
    out[len] = 0;
    return out;
}

/* A String field of a component's data model. */
static AzComponentDataField az_string_field(const char* name, const char* value, const char* description) {
    AzComponentDataField f;
    f.name = AzString_copyFromBytes((const uint8_t*)name, 0, strlen(name));
    f.field_type = AzComponentFieldType_string();
    f.default_value = AzOptionComponentDefaultValue_some(
        AzComponentDefaultValue_string(AzString_copyFromBytes((const uint8_t*)value, 0, strlen(value))));
    f.required = false;
    f.description = AzString_copyFromBytes((const uint8_t*)description, 0, strlen(description));
    return f;
}
#endif
";

/// A C string literal (`"..."`) for a `const char*` argument.
fn c_lit(s: &str) -> String {
    format!("\"{}\"", super::c_escape(s))
}

/// The C / C++ registration of a component library: per component a
/// default-arguments wrapper, a render function reading the data model, a
/// compile function and its `ComponentDef`; then
/// `register_<library>_library()`. `az_str` spells an `AzString` from a
/// literal in the language. Items the module does not have are skipped.
#[must_use]
pub fn c_family_registration(
    m: &Module,
    lib: &LibrarySpec,
    param_ident: &dyn Fn(&Ident) -> String,
    az_str: &dyn Fn(&str) -> String,
) -> String {
    let lsn = Ident::from_text(&lib.name).snake();
    let mut s = String::new();
    s.push_str("\n/* ── registration ── */\n");
    let any_params = lib
        .components
        .iter()
        .filter_map(|c| m.item(&c.item))
        .any(|i| !i.params.is_empty());
    if any_params {
        s.push_str(C_REGISTRATION_HELPERS);
    }
    let mut defs: Vec<String> = Vec::new();
    for c in &lib.components {
        let Some(item) = m.item(&c.item) else {
            continue;
        };
        let item_fn = item.name.snake();
        let sn = Ident::from_text(&c.name).snake();
        let qn = format!("{}:{}", lib.name, c.name);
        let defaults: Vec<String> = item.params.iter().map(|p| c_lit(p.default_text())).collect();
        let _ = write!(
            s,
            "\n/* `{}` with its default arguments. */\nstatic AzDom {item_fn}_default(void) {{\n    \
             return {item_fn}({});\n}}\n",
            block_comment_safe(&qn),
            defaults.join(", ")
        );
        let _ = writeln!(
            s,
            "\nstatic AzResultStyledDomRenderDomError {sn}_render_fn(const AzComponentDef* def, \
             const AzComponentDataModel* model, const AzComponentMap* map) {{"
        );
        s.push_str("    (void)def;\n    (void)map;\n");
        let mut args: Vec<String> = Vec::new();
        if item.params.is_empty() {
            s.push_str("    (void)model;\n");
        }
        for p in &item.params {
            let local = format!("arg_{}", param_ident(&p.name));
            let _ = writeln!(
                s,
                "    char* {local} = az_model_string(model, {}, {});",
                c_lit(&p.name.snake()),
                c_lit(p.default_text())
            );
            args.push(local);
        }
        let _ = writeln!(s, "    AzDom dom = {item_fn}({});", args.join(", "));
        for a in &args {
            let _ = writeln!(s, "    free({a});");
        }
        s.push_str(
            "    return AzResultStyledDomRenderDomError_ok(AzStyledDom_createFromDom(dom));\n}\n",
        );
        let _ = writeln!(
            s,
            "\nstatic AzResultStringCompileError {sn}_compile_fn(const AzComponentDef* def, const \
             AzCompileTarget* target, const AzComponentDataModel* model, size_t indent) {{"
        );
        s.push_str("    (void)def;\n    (void)target;\n    (void)model;\n    (void)indent;\n");
        let _ = writeln!(
            s,
            "    return AzResultStringCompileError_ok({});\n}}",
            az_str(&format!("{item_fn}_default()"))
        );
        let _ = writeln!(s, "\nstatic AzComponentDef {sn}_def(void) {{");
        s.push_str("    AzComponentDef def;\n");
        let _ = writeln!(
            s,
            "    def.id = AzComponentId_create({}, {});",
            az_str(&lib.name),
            az_str(&c.name)
        );
        let _ = writeln!(s, "    def.display_name = {};", az_str(&c.display_name));
        let _ = writeln!(s, "    def.description = {};", az_str(&c.description));
        let _ = writeln!(s, "    /* The CSS is applied per node by {item_fn}. */");
        let _ = writeln!(s, "    def.css = {};", az_str(""));
        s.push_str("    def.source = AzComponentSource_UserDefined;\n");
        let _ = writeln!(s, "    def.data_model.name = {};", az_str(&c.data_model));
        let _ = writeln!(
            s,
            "    def.data_model.description = {};",
            az_str(&c.data_model_description)
        );
        if item.params.is_empty() {
            s.push_str("    def.data_model.fields = AzComponentDataFieldVec_create();\n");
        } else {
            let n = item.params.len();
            let _ = writeln!(s, "    AzComponentDataField fields[{n}];");
            for (i, p) in item.params.iter().enumerate() {
                let desc = c.field_descriptions.get(i).map_or("", String::as_str);
                let _ = writeln!(
                    s,
                    "    fields[{i}] = az_string_field({}, {}, {});",
                    c_lit(&p.name.snake()),
                    c_lit(p.default_text()),
                    c_lit(desc)
                );
            }
            let _ = writeln!(
                s,
                "    def.data_model.fields = AzComponentDataFieldVec_copyFromPtr(fields, {n});"
            );
            let _ = writeln!(
                s,
                "    /* copyFromPtr cloned them. */\n    for (size_t i = 0; i < {n}; i++) \
                 AzComponentDataField_delete(&fields[i]);"
            );
        }
        let _ = writeln!(s, "    def.render_fn = {sn}_render_fn;");
        let _ = writeln!(s, "    def.compile_fn = {sn}_compile_fn;");
        s.push_str("    def.render_fn_source = AzOptionString_none();\n");
        s.push_str("    def.compile_fn_source = AzOptionString_none();\n    return def;\n}\n");
        defs.push(format!("{sn}_def()"));
    }
    let n = defs.len();
    let _ = write!(
        s,
        "\n/* The component library `{}`:\n *     AzAppConfig_addComponentLibrary(&config, {}, \
         register_{lsn}_library); */\nAzComponentLibrary register_{lsn}_library(void) {{\n",
        block_comment_safe(&lib.name),
        block_comment_safe(&az_str(&lib.name))
    );
    if n == 0 {
        s.push_str("    AzComponentLibrary lib;\n");
    } else {
        let _ = writeln!(s, "    AzComponentDef defs[{n}];");
        for (i, d) in defs.iter().enumerate() {
            let _ = writeln!(s, "    defs[{i}] = {d};");
        }
        s.push_str("    AzComponentLibrary lib;\n");
    }
    let _ = writeln!(s, "    lib.name = {};", az_str(&lib.name));
    let _ = writeln!(s, "    lib.version = {};", az_str(&lib.version));
    let _ = writeln!(s, "    lib.description = {};", az_str("Exported from AzBuilder"));
    if n == 0 {
        s.push_str("    lib.components = AzComponentDefVec_create();\n");
    } else {
        let _ = writeln!(
            s,
            "    lib.components = AzComponentDefVec_copyFromPtr(defs, {n});\n    /* copyFromPtr \
             cloned them. */\n    for (size_t i = 0; i < {n}; i++) AzComponentDef_delete(&defs[i]);"
        );
    }
    s.push_str("    lib.exportable = true;\n    lib.modifiable = false;\n");
    s.push_str("    lib.data_models = AzComponentDataModelVec_create();\n");
    s.push_str("    lib.enum_models = AzComponentEnumModelVec_create();\n    return lib;\n}\n");
    s
}

// ── DOM items through a binding's wrapper layer ──

/// `true` if `item` is DOM construction (it takes parameters or builds a
/// `Dom`) rather than a CSS value. A printer whose binding builds a DOM
/// through another layer than CSS values picks its DOM syntax by it.
#[must_use]
pub fn is_dom_item(item: &Item) -> bool {
    !item.params.is_empty() || item.ty == "Dom"
}

/// The api.json classes the DOM lowering (`azul_core::xml`) constructs.
pub const DOM_CLASSES: &[&str] = &["Dom", "SmallAriaInfo"];

/// How a printer spells DOM items through its binding's WRAPPER layer (the
/// managed classes: native strings, `dom.withChild(..)` chains) when its
/// CSS values go through the raw C layer (`AzulNativeCss.Az..`, `azul.C`,
/// ...). [`WrapperDom`] makes it the [`ExprSyntax`] of the printer's DOM
/// items.
pub trait WrapperDomSyntax {
    /// The printer's own syntax (number and boolean literals).
    fn base(&self) -> &dyn ExprSyntax;
    /// A native string literal (what the wrapper methods take).
    fn native_string(&self, s: &str) -> Doc;
    /// The wrapper's static factory `class::method(args)`.
    fn factory(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc;
    /// The wrapper's by-value `self` method `recv.method(args)`.
    fn method(
        &self,
        recv: Doc,
        class: &str,
        method: &str,
        args: Vec<Doc>,
        layout: MethodLayout,
    ) -> Doc;
    /// Item parameter `name` (a native string).
    fn param(&self, name: &Ident) -> Doc;
    /// A native string joined from `parts`.
    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc;
}

/// The [`ExprSyntax`] of a [`WrapperDomSyntax`]: literals come from its
/// base syntax, and a node the wrapper classes do not take (see
/// [`wrapper_dom_limitation`]) is a limitation instead of wrong code.
#[derive(Debug, Copy, Clone, Default)]
pub struct WrapperDom<W>(pub W);

impl<W: WrapperDomSyntax> ExprSyntax for WrapperDom<W> {
    fn int(&self, value: i128, ty: Prim) -> String {
        self.0.base().int(value, ty)
    }

    fn float(&self, text: &str, ty: Prim) -> String {
        self.0.base().float(text, ty)
    }

    fn boolean(&self, b: bool) -> String {
        self.0.base().boolean(b)
    }

    fn string(&self, s: &str) -> Doc {
        self.0.native_string(s)
    }

    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc {
        self.0.factory(class, method, args, broken)
    }

    // The next three are never reached: `limitation` rejects their nodes.
    fn variant(&self, ty: &str, shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc {
        self.0.base().variant(ty, shape, variant, args, broken)
    }

    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc {
        self.0.base().strukt(ty, fields, broken)
    }

    fn vec(&self, ty: &str, elem: &str, items: Vec<Doc>, broken: bool) -> Doc {
        self.0.base().vec(ty, elem, items, broken)
    }

    fn unsupported(&self, what: &str) -> Doc {
        self.0.base().unsupported(what)
    }

    fn limitation(&self, e: &Expr) -> Option<String> {
        wrapper_dom_limitation(e)
    }

    fn dom_limitation(&self) -> Option<&'static str> {
        None
    }

    fn method(&self, recv: Doc, class: &str, method: &str, args: Vec<Doc>, layout: MethodLayout) -> Doc {
        self.0.method(recv, class, method, args, layout)
    }

    fn param(&self, name: &Ident) -> Doc {
        self.0.param(name)
    }

    fn concat(&self, parts: &[ConcatPart<'_>]) -> Doc {
        self.0.concat(parts)
    }
}

/// Why node `e` has no spelling in a DOM built through wrapper classes: a
/// raw struct, variant or Vec value, or a class the DOM lowering does not
/// construct (see [`DOM_CLASSES`]).
#[must_use]
pub fn wrapper_dom_limitation(e: &Expr) -> Option<String> {
    match e {
        Expr::Call { class, method, .. } | Expr::Method { class, method, .. }
            if !DOM_CLASSES.contains(&class.as_str()) =>
        {
            Some(format!(
                "{class}.{method}: this DOM export builds through the wrapper classes of {}",
                DOM_CLASSES.join(" / ")
            ))
        }
        Expr::Variant { ty, .. } | Expr::Struct { ty, .. } | Expr::Vec { ty, .. } => Some(format!(
            "a raw {ty} value: this DOM export builds through the wrapper classes, which take \
             native values"
        )),
        _ => None,
    }
}

/// `recv.link` in a method chain whose links break with the dot at the END
/// of a line (`AzDom_createDiv().\n    withCss(..)`), for languages where a
/// line may not START with `.`: Go ends a statement at a line that ends in
/// `)`, and Nim continues an expression only after the `.` (`'.' optInd`).
/// Flat, it reads like [`Doc::chained`]'s chain.
#[must_use]
pub fn chained_dot_at_line_end(recv: Doc, link: Doc) -> Doc {
    match recv {
        Doc::Chain {
            head, mut links, ..
        } => {
            if let Some(last) = links.pop() {
                links.push(Doc::cat(vec![last, Doc::text(".")]));
            }
            links.push(link);
            let broken = links.len() >= 2;
            Doc::Chain {
                head,
                links,
                broken,
            }
        }
        other => Doc::Chain {
            head: alloc::boxed::Box::new(Doc::cat(vec![other, Doc::text(".")])),
            links: vec![link],
            broken: false,
        },
    }
}

/// `recv op link` as a chain of infix links (`|> ..` in OCaml, `>>= ..` in
/// Haskell): flat with one link (the link carries its leading space), one
/// link per line from two on.
#[must_use]
pub fn chained_infix(recv: Doc, link: Doc) -> Doc {
    match recv {
        Doc::Chain {
            head, mut links, ..
        } => {
            // The first link of a flat chain carries its leading space.
            if let Some(Doc::Cat(parts)) = links.first_mut() {
                if matches!(parts.first(), Some(Doc::Text(t)) if t == " ") {
                    let _space = parts.remove(0);
                }
            }
            links.push(link);
            Doc::Chain {
                head,
                links,
                broken: true,
            }
        }
        other => Doc::Chain {
            head: alloc::boxed::Box::new(other),
            links: vec![Doc::cat(vec![Doc::text(" "), link])],
            broken: false,
        },
    }
}

/// `s` on one line (a window title or a library name in a comment).
#[must_use]
pub fn one_line(s: &str) -> String {
    s.replace(|c: char| c == '\n' || c == '\r', " ")
}

/// The comment lines (without comment markers) a printer writes instead of
/// the registration of `lib` when its binding cannot build one; `reason`
/// (one clause, no final period) says why.
#[must_use]
pub fn registration_note(lib: &LibrarySpec, reason: &str) -> Vec<String> {
    vec![
        format!(
            "The component library `{}` is not registered here: AppConfig.add_component_library",
            one_line(&lib.name)
        ),
        "needs a ComponentDef whose render_fn is a C function pointer, and".to_string(),
        format!("{reason}."),
        "Register the library from an export that can (Rust, C, C++, Zig, Pascal); the".to_string(),
        "functions above build its components.".to_string(),
    ]
}
