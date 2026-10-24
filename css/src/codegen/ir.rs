//! The language-neutral IR every printer consumes.
//!
//! An [`Expr`] describes *how to construct a value through the azul bindings*,
//! in api.json vocabulary (class names without the `Az` prefix, snake_case
//! method names). It is produced exactly once, by [`super::lower`], and each
//! language printer only decides how to *spell* the six node kinds:
//!
//! | node | api.json concept | C spelling (for orientation) |
//! |---|---|---|
//! | [`Expr::Call`] | a static constructor (`PixelValue::px`) | `AzPixelValue_px(10.0f)` |
//! | [`Expr::Variant`] | an enum variant (`LayoutWidth::Px(..)`) | `AzLayoutWidth_px(..)` |
//! | [`Expr::Struct`] | a struct literal (`ColorU { r, g, b, a }`) | `(AzColorU){ .r = .. }` |
//! | [`Expr::Vec`] | an `XxxVec` built from items | `AzXxxVec_copyFromPtr(..)` |
//! | [`Expr::Str`] | an api.json `String` | `AzString_copyFromBytes(..)` |
//! | [`Expr::Int`] / [`Expr::Float`] / [`Expr::Bool`] | primitives | `255`, `10.0f`, `true` |
//!
//! [`Expr::Unsupported`] marks a value the bindings cannot express (a
//! `FontRef`, a `BoxOrStatic` payload without a constructor); printers emit it
//! as a comment and the containing list skips the element.
//!
//! A [`Module`] is a list of named [`Item`]s (one per style / stylesheet); the
//! printers turn each into a function (or the language's closest equivalent)
//! returning the value.
//!
//! **DOM construction** (AzBuilder's "Subtree → code" / "Component → code",
//! lowered in `azul_core::codegen::dom`) adds four node kinds:
//!
//! | node | api.json concept | C spelling |
//! |---|---|---|
//! | [`Expr::Method`] | a by-value `self` method (`Dom.with_child`) | `AzDom_withChild(dom, child)` |
//! | [`Expr::Param`] | a parameter of the item's function (a native string) | `AzString_copyFromBytes((const uint8_t*)text, 0, strlen(text))` |
//! | [`Expr::Concat`] | a `String` joined from literals and parameters | a helper call |
//! | [`Expr::ItemCall`] | a call of another item of the module (a component's render function) | `render_card("Hi", title)` |
//!
//! and an [`Item`] may take typed [`ItemParam`]s; a [`Module`] may describe an
//! app ([`AppSpec`]: `emit_project_files` then writes a program that opens a
//! window) or a component library ([`LibrarySpec`]: the printers that can
//! write the registration append it). A printer that cannot spell these
//! reports it through `ExprSyntax::dom_limitation` instead of printing wrong
//! code.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

/// A primitive scalar type of the C ABI, spelled like api.json spells it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prim {
    U8,
    U16,
    U32,
    U64,
    Usize,
    I8,
    I16,
    I32,
    I64,
    Isize,
    F32,
    F64,
    Bool,
}

impl Prim {
    /// The api.json / Rust spelling (`"u8"`, `"f32"`, ...).
    #[must_use]
    pub const fn api_name(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::Usize => "usize",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::Isize => "isize",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
        }
    }

    /// The C spelling used by `azul.h` (`uint8_t`, `float`, ...).
    #[must_use]
    pub const fn c_name(self) -> &'static str {
        match self {
            Self::U8 => "uint8_t",
            Self::U16 => "uint16_t",
            Self::U32 => "uint32_t",
            Self::U64 => "uint64_t",
            Self::Usize => "size_t",
            Self::I8 => "int8_t",
            Self::I16 => "int16_t",
            Self::I32 => "int32_t",
            Self::I64 => "int64_t",
            Self::Isize => "ssize_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Bool => "bool",
        }
    }

    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Isize | Self::F32 | Self::F64
        )
    }
}

/// How an enum is represented in the C ABI, which decides how the bindings
/// let you construct a variant.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum EnumShape {
    /// Only unit variants: a plain C enum (`AzLayoutDisplay_Flex`). The
    /// bindings also generate a nullary variant constructor
    /// (`AzLayoutDisplay_flex()`), but every binding exposes the constants.
    CLike,
    /// A `repr(C, u8)` tagged union. The bindings generate one constructor
    /// function per variant: `Az<Enum>_<lowerCamelVariant>(payload)`
    /// (`AzLayoutWidth_px(AzPixelValue)`).
    Tagged,
    /// A tagged union whose variant constructor is SHADOWED by a hand-written
    /// api.json constructor of the same C name (only `CssProperty` has these:
    /// `AzCssProperty_width` takes a `LayoutWidth`, not a `LayoutWidthValue`).
    /// The lowering avoids producing these where it can.
    TaggedShadowed,
    /// A monomorphized generic, e.g. `LayoutWidthValue` =
    /// `CssPropertyValue<LayoutWidth>`: a tagged union with NO constructor
    /// functions in the C ABI (C needs a compound literal). `base` is the
    /// generic's api.json name, `arg` the type argument.
    Generic {
        base: &'static str,
        arg: &'static str,
    },
}

/// One node of the construction IR. See the module docs for the mapping.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// An integer literal of the given primitive type.
    Int { value: i128, ty: Prim },
    /// A float literal. `text` is the canonical decimal spelling (always
    /// contains a `.`, e.g. `"10.0"`, `"0.7"`, `"-1.25"`) or one of `"nan"`,
    /// `"inf"`, `"-inf"`; printers only add the language's suffix/cast.
    Float { text: String, ty: Prim },
    /// A boolean literal.
    Bool(bool),
    /// An api.json `String` (C `AzString`) built from a literal.
    Str(String),
    /// A static api.json constructor / function: `class::method(args)`.
    /// `method` is the api.json snake_case name (`"text_color"`).
    Call {
        class: String,
        method: String,
        args: Vec<Expr>,
    },
    /// An enum variant. `args` is empty for a unit variant, one element for
    /// a tuple variant (api.json variants carry at most one payload).
    Variant {
        ty: String,
        shape: EnumShape,
        variant: String,
        args: Vec<Expr>,
    },
    /// A struct literal, fields in declaration order (api.json order ==
    /// Rust order, checked by the generator).
    Struct {
        ty: String,
        fields: Vec<(String, Expr)>,
    },
    /// An api.json `XxxVec` built from its items.
    Vec {
        ty: String,
        elem: String,
        items: Vec<Expr>,
    },
    /// A value the bindings cannot express; `what` explains it (printed as
    /// a comment). A `Vec` item that is `Unsupported` is dropped by printers.
    Unsupported { what: String },
    /// A by-value `self` method of api.json class `class` on `recv` (the
    /// builder methods: `Dom.with_child`, `Dom.with_css`, ...). `method` is
    /// the api.json snake_case name. A chain of them nests: the innermost
    /// `recv` is the constructor.
    Method {
        recv: alloc::boxed::Box<Expr>,
        class: String,
        method: String,
        args: Vec<Expr>,
    },
    /// The value of parameter `name` of the enclosing [`Item`] (declared in
    /// [`Item::params`]), as the api.json type the parameter declares
    /// (`String`: printers convert the language's native string).
    Param(Ident),
    /// An api.json `String` joined from its parts, each an [`Expr::Str`] or
    /// an [`Expr::Param`] (a template text like `"by {author}"`).
    Concat(Vec<Expr>),
    /// A call of another item of the SAME module by its name, one argument
    /// per parameter of that item, in order: a component instance calling
    /// the component's render function (component boundaries stay calls,
    /// nothing is inlined). Every argument is a `String` parameter's value,
    /// passed as the language's NATIVE string (what the callee's parameter
    /// takes): an [`Expr::Str`], a caller's [`Expr::Param`] or an
    /// [`Expr::Concat`] of both. The callee returns the item's type.
    ItemCall {
        item: Ident,
        /// The callee's parameter names, one per argument (the languages
        /// with labelled / named arguments pass them by name).
        params: Vec<Ident>,
        args: Vec<Expr>,
    },
}

impl Expr {
    #[must_use]
    pub const fn int(value: i128, ty: Prim) -> Self {
        Self::Int { value, ty }
    }

    #[must_use]
    pub fn f32(v: f32) -> Self {
        Self::Float {
            text: float_text_f32(v),
            ty: Prim::F32,
        }
    }

    #[must_use]
    pub fn f64(v: f64) -> Self {
        Self::Float {
            text: float_text(v),
            ty: Prim::F64,
        }
    }

    #[must_use]
    pub fn f32_text(text: String) -> Self {
        Self::Float { text, ty: Prim::F32 }
    }

    #[must_use]
    pub fn str(s: &str) -> Self {
        Self::Str(s.to_string())
    }

    #[must_use]
    pub fn call(class: &str, method: &str, args: Vec<Self>) -> Self {
        Self::Call {
            class: class.to_string(),
            method: method.to_string(),
            args,
        }
    }

    #[must_use]
    pub fn variant(ty: &str, shape: EnumShape, variant: &str, args: Vec<Self>) -> Self {
        Self::Variant {
            ty: ty.to_string(),
            shape,
            variant: variant.to_string(),
            args,
        }
    }

    #[must_use]
    pub fn unit(ty: &str, shape: EnumShape, variant: &str) -> Self {
        Self::variant(ty, shape, variant, Vec::new())
    }

    #[must_use]
    pub fn strukt(ty: &str, fields: Vec<(&str, Self)>) -> Self {
        Self::Struct {
            ty: ty.to_string(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[must_use]
    pub fn vec(ty: &str, elem: &str, items: Vec<Self>) -> Self {
        Self::Vec {
            ty: ty.to_string(),
            elem: elem.to_string(),
            items,
        }
    }

    #[must_use]
    pub fn unsupported(what: &str) -> Self {
        Self::Unsupported {
            what: what.to_string(),
        }
    }

    /// `recv.method(args)`, a by-value `self` method of `class`.
    #[must_use]
    pub fn method(recv: Self, class: &str, method: &str, args: Vec<Self>) -> Self {
        Self::Method {
            recv: alloc::boxed::Box::new(recv),
            class: class.to_string(),
            method: method.to_string(),
            args,
        }
    }

    /// The item parameter `name` (`"text"`, `"text_2"`).
    #[must_use]
    pub fn param(name: &str) -> Self {
        Self::Param(Ident::from_text(name))
    }

    /// `item(param: arg, ..)`: a call of item `item` of the same module,
    /// each argument with the callee's parameter name (see
    /// [`Expr::ItemCall`]).
    #[must_use]
    pub fn item_call(item: &str, args: Vec<(&str, Self)>) -> Self {
        let (params, args) = args
            .into_iter()
            .map(|(p, a)| (Ident::from_text(p), a))
            .unzip();
        Self::ItemCall {
            item: Ident::from_text(item),
            params,
            args,
        }
    }

    /// A `String` joined from `parts` (literals and parameters). One literal
    /// part is just that literal, one parameter just that parameter.
    #[must_use]
    pub fn concat(parts: Vec<Self>) -> Self {
        match <[Self; 1]>::try_from(parts) {
            Ok([only]) => only,
            Err(parts) => Self::Concat(parts),
        }
    }

    /// `true` for the DOM node kinds ([`Expr::Method`], [`Expr::Param`],
    /// [`Expr::Concat`], [`Expr::ItemCall`]).
    #[must_use]
    pub const fn is_dom_node(&self) -> bool {
        matches!(
            self,
            Self::Method { .. } | Self::Param(_) | Self::Concat(_) | Self::ItemCall { .. }
        )
    }

    /// `true` for literals (no nested construction).
    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        matches!(
            self,
            Self::Int { .. } | Self::Float { .. } | Self::Bool(_) | Self::Str(_)
        )
    }

    /// `true` if this node cannot be built: it or a child is
    /// [`Expr::Unsupported`]. Does not look into a *droppable* Vec (see
    /// [`is_droppable_vec`]): that one drops its own bad items. Any other Vec
    /// (gradient stops, grid tracks, transforms, ...) is a value, and a
    /// value with a hole is wrong, so its bad item makes the whole value bad.
    #[must_use]
    pub fn contains_unsupported(&self) -> bool {
        match self {
            Self::Unsupported { .. } => true,
            Self::Call { args, .. } | Self::Variant { args, .. } => {
                args.iter().any(Self::contains_unsupported)
            }
            Self::Struct { fields, .. } => fields.iter().any(|(_, e)| e.contains_unsupported()),
            Self::Vec { ty, items, .. } => {
                !is_droppable_vec(ty) && items.iter().any(Self::contains_unsupported)
            }
            Self::Method { recv, args, .. } => {
                recv.contains_unsupported() || args.iter().any(Self::contains_unsupported)
            }
            Self::Concat(parts) | Self::ItemCall { args: parts, .. } => {
                parts.iter().any(Self::contains_unsupported)
            }
            Self::Int { .. } | Self::Float { .. } | Self::Bool(_) | Self::Str(_) | Self::Param(_) => {
                false
            }
        }
    }

    /// Every `Unsupported` reason inside this expression (depth-first).
    pub fn unsupported_reasons(&self, out: &mut Vec<String>) {
        match self {
            Self::Unsupported { what } => out.push(what.clone()),
            Self::Call { args, .. } | Self::Variant { args, .. } => {
                for a in args {
                    a.unsupported_reasons(out);
                }
            }
            Self::Struct { fields, .. } => {
                for (_, e) in fields {
                    e.unsupported_reasons(out);
                }
            }
            Self::Vec { items, .. } | Self::Concat(items) | Self::ItemCall { args: items, .. } => {
                for e in items {
                    e.unsupported_reasons(out);
                }
            }
            Self::Method { recv, args, .. } => {
                recv.unsupported_reasons(out);
                for a in args {
                    a.unsupported_reasons(out);
                }
            }
            Self::Int { .. } | Self::Float { .. } | Self::Bool(_) | Self::Str(_) | Self::Param(_) => {}
        }
    }

    /// Visit every node, parents before children.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Self)) {
        f(self);
        match self {
            Self::Call { args, .. } | Self::Variant { args, .. } => {
                for a in args {
                    a.walk(f);
                }
            }
            Self::Struct { fields, .. } => {
                for (_, e) in fields {
                    e.walk(f);
                }
            }
            Self::Vec { items, .. } | Self::Concat(items) | Self::ItemCall { args: items, .. } => {
                for e in items {
                    e.walk(f);
                }
            }
            Self::Method { recv, args, .. } => {
                recv.walk(f);
                for a in args {
                    a.walk(f);
                }
            }
            Self::Int { .. }
            | Self::Float { .. }
            | Self::Bool(_)
            | Self::Str(_)
            | Self::Param(_)
            | Self::Unsupported { .. } => {}
        }
    }

    /// Every api.json type name this expression constructs (sorted, deduped).
    #[must_use]
    pub fn used_types(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        self.walk(&mut |e| match e {
            Self::Call { class, .. } => out.push(class.clone()),
            Self::Variant { ty, shape, .. } => {
                out.push(ty.clone());
                if let EnumShape::Generic { base, .. } = shape {
                    out.push((*base).to_string());
                }
            }
            Self::Struct { ty, .. } => out.push(ty.clone()),
            Self::Vec { ty, elem, .. } => {
                out.push(ty.clone());
                out.push(elem.clone());
            }
            Self::Method { class, .. } => out.push(class.clone()),
            Self::Str(_) | Self::Concat(_) => out.push("String".to_string()),
            _ => {}
        });
        out.sort();
        out.dedup();
        out
    }
}

/// Lists whose items are independent (a style's properties, a rule's
/// declarations, a stylesheet's rules): an item the bindings cannot build is
/// dropped from them (with a note) instead of poisoning the whole list.
pub const DROPPABLE_VECS: &[&str] = &[
    "CssPropertyWithConditionsVec",
    "CssDeclarationVec",
    "CssPropertyVec",
    "CssRuleBlockVec",
];

/// See [`DROPPABLE_VECS`].
#[must_use]
pub fn is_droppable_vec(ty: &str) -> bool {
    DROPPABLE_VECS.contains(&ty)
}

/// Canonical decimal text of an `f64`: the shortest representation that
/// round-trips, always containing a `.` (`"10.0"`, `"0.5"`), or
/// `"nan"` / `"inf"` / `"-inf"`.
#[must_use]
pub fn float_text(v: f64) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return String::from(if v > 0.0 { "inf" } else { "-inf" });
    }
    // `Display` on floats prints the shortest round-trip form and never an
    // exponent; make sure there is a decimal point.
    normalize_float_text(format!("{v}"))
}

/// [`float_text`] for an `f32`: shortest text that round-trips through
/// `f32` (`0.7_f32` -> `"0.7"`, not `"0.699999988079071"`).
#[must_use]
pub fn float_text_f32(v: f32) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return String::from(if v > 0.0 { "inf" } else { "-inf" });
    }
    normalize_float_text(format!("{v}"))
}

fn normalize_float_text(mut s: String) -> String {
    if s == "-0" {
        s = "0".to_string();
    }
    if !s.contains('.') {
        s.push_str(".0");
    }
    s
}

/// A multi-word identifier the printers case per language
/// (`["btn", "primary"]` -> `btn_primary` / `btnPrimary` / `BtnPrimary`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    pub words: Vec<String>,
}

impl Ident {
    /// Split any string (a CSS selector, `camelCase`, `snake_case`, ...) into
    /// lowercase ASCII words. Non-alphanumerics separate words; an empty
    /// result becomes `["style"]`; a leading digit gets an `n` prefix.
    #[must_use]
    pub fn from_text(s: &str) -> Self {
        let mut words: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut prev_lower = false;
        for c in s.chars() {
            if c.is_ascii_alphanumeric() {
                if c.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
                    words.push(core::mem::take(&mut cur));
                }
                prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
                cur.push(c.to_ascii_lowercase());
            } else {
                if !cur.is_empty() {
                    words.push(core::mem::take(&mut cur));
                }
                prev_lower = false;
            }
        }
        if !cur.is_empty() {
            words.push(cur);
        }
        if words.is_empty() {
            words.push("style".to_string());
        }
        if words[0].starts_with(|c: char| c.is_ascii_digit()) {
            words[0] = format!("n{}", words[0]);
        }
        Self { words }
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        if self.words.first().map(String::as_str) != Some(prefix) {
            self.words.insert(0, prefix.to_string());
        }
        self
    }

    /// `btn_primary`
    #[must_use]
    pub fn snake(&self) -> String {
        self.words.join("_")
    }

    /// `BTN_PRIMARY`
    #[must_use]
    pub fn screaming(&self) -> String {
        self.snake().to_ascii_uppercase()
    }

    /// `btn-primary`
    #[must_use]
    pub fn kebab(&self) -> String {
        self.words.join("-")
    }

    /// `btnPrimary`
    #[must_use]
    pub fn lower_camel(&self) -> String {
        let mut out = String::new();
        for (i, w) in self.words.iter().enumerate() {
            if i == 0 {
                out.push_str(w);
            } else {
                out.push_str(&capitalize(w));
            }
        }
        out
    }

    /// `BtnPrimary`
    #[must_use]
    pub fn upper_camel(&self) -> String {
        self.words.iter().map(|w| capitalize(w)).collect()
    }
}

/// `"flex"` -> `"Flex"` (ASCII).
#[must_use]
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        let mut out = f.to_ascii_uppercase().to_string();
        out.push_str(c.as_str());
        out
    })
}

/// `"text_color"` -> `"textColor"` (the C ABI method spelling).
#[must_use]
pub fn snake_to_lower_camel(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `"text_color"` -> `"TextColor"`.
#[must_use]
pub fn snake_to_upper_camel(s: &str) -> String {
    capitalize(&snake_to_lower_camel(s))
}

/// `"MinContent"` -> `"min_content"`, `"W500"` -> `"w500"`,
/// `"MacOS"` -> `"mac_os"`, `"IOS"` -> `"ios"`.
#[must_use]
pub fn camel_to_snake(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            let prev_upper = i > 0 && chars[i - 1].is_ascii_uppercase();
            if i > 0 && (prev_lower || (prev_upper && next_lower)) {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `"MinContent"` -> `"minContent"` (the C ABI variant-constructor spelling:
/// only the first character is lowered, like the bindings generator does).
#[must_use]
pub fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        let mut out = f.to_ascii_lowercase().to_string();
        out.push_str(c.as_str());
        out
    })
}

/// One named value of a generated module (a style, the stylesheet, or a
/// DOM render function).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Identifier words, cased by the printer (`style_btn_primary`).
    pub name: Ident,
    /// Comment lines (the CSS selector, dropped/unsupported notes).
    pub doc: Vec<String>,
    /// api.json type of `value` (`"CssPropertyWithConditionsVec"`, `"Css"`,
    /// `"Dom"`).
    pub ty: String,
    /// The function's parameters ([`Expr::Param`] refers to them). Empty for
    /// every CSS item.
    pub params: Vec<ItemParam>,
    pub value: Expr,
}

/// A typed parameter of an [`Item`]'s function.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemParam {
    /// Identifier words, cased by the printer like the item name.
    pub name: Ident,
    /// api.json type of the parameter (`"String"`: printers take the
    /// language's native string and convert it where it is used).
    pub ty: String,
    /// The value the item was made with (a keyword default in languages
    /// that have them; a default-arguments wrapper uses it).
    pub default: Option<Expr>,
}

impl ItemParam {
    /// A `String` parameter with a default.
    #[must_use]
    pub fn string(name: &str, default: &str) -> Self {
        Self {
            name: Ident::from_text(name),
            ty: "String".to_string(),
            default: Some(Expr::str(default)),
        }
    }

    /// The default's text, for a `String` parameter with a literal default.
    #[must_use]
    pub fn default_text(&self) -> &str {
        match &self.default {
            Some(Expr::Str(s)) => s,
            _ => "",
        }
    }
}

/// A module that is an app: `emit_project_files` writes a program that
/// opens a window showing item `root` (instead of printing each item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSpec {
    /// The window title.
    pub title: String,
    /// The item that builds the window's content (a `Dom`, no parameters).
    pub root: Ident,
    /// `root` builds the `<body>` itself; else the app puts it into one.
    pub is_body: bool,
}

/// A module that is a component library: printers that can spell the
/// registration append a `register_<library>_library()` that registers
/// each component (a `ComponentDef` whose render function calls the item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySpec {
    /// The library name (`"user"`).
    pub name: String,
    pub version: String,
    pub components: Vec<ComponentSpec>,
}

/// One component of a [`LibrarySpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSpec {
    /// The item that renders it (its parameters are the String fields of
    /// the data model, in order).
    pub item: Ident,
    /// The component's tag (`"my-card"`).
    pub name: String,
    pub display_name: String,
    pub description: String,
    /// The data model's name (`"MyCardData"`) and description.
    pub data_model: String,
    pub data_model_description: String,
    /// Each field's description, in parameter order.
    pub field_descriptions: Vec<String>,
}

/// A generated module: what one `emit_*` call prints.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Module {
    pub items: Vec<Item>,
    /// `Some`: `emit_project_files` writes an app around the module.
    pub app: Option<AppSpec>,
    /// `Some`: the module is a component library (see [`LibrarySpec`]).
    pub library: Option<LibrarySpec>,
}

impl Module {
    /// Every api.json type used by any item (sorted, deduped).
    #[must_use]
    pub fn used_types(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .items
            .iter()
            .flat_map(|i| {
                let mut v = i.value.used_types();
                v.push(i.ty.clone());
                v.extend(i.params.iter().map(|p| p.ty.clone()));
                v
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// `true` if any item takes parameters or builds a DOM.
    #[must_use]
    pub fn is_dom(&self) -> bool {
        self.app.is_some()
            || self.library.is_some()
            || self
                .items
                .iter()
                .any(|i| !i.params.is_empty() || i.ty == "Dom")
    }

    /// The item named `name`.
    #[must_use]
    pub fn item(&self, name: &Ident) -> Option<&Item> {
        self.items.iter().find(|i| &i.name == name)
    }
}

/// Flatten `e` for languages that cannot nest constructor calls: every
/// non-leaf child is hoisted into a temporary, in evaluation order. Returns
/// the bindings (`(temp_index, shallow expr)`) and the final shallow
/// expression; shallow expressions reference temporaries through
/// [`Operand::Temp`] slots.
#[must_use]
pub fn linearize(e: &Expr) -> (Vec<(usize, Shallow)>, Shallow) {
    let mut out = Vec::new();
    let root = linearize_into(e, &mut out);
    (out, root)
}

/// An argument of a [`Shallow`] node: a literal or a temporary.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Lit(Expr),
    Temp(usize),
}

/// A node whose children are all [`Operand`]s (no nesting).
#[derive(Debug, Clone, PartialEq)]
pub enum Shallow {
    Lit(Expr),
    Call {
        class: String,
        method: String,
        args: Vec<Operand>,
    },
    Variant {
        ty: String,
        shape: EnumShape,
        variant: String,
        args: Vec<Operand>,
    },
    Struct {
        ty: String,
        fields: Vec<(String, Operand)>,
    },
    Vec {
        ty: String,
        elem: String,
        items: Vec<Operand>,
    },
    Unsupported { what: String },
}

fn operand(e: &Expr, out: &mut Vec<(usize, Shallow)>) -> Operand {
    if e.is_leaf() {
        return Operand::Lit(e.clone());
    }
    let s = linearize_into(e, out);
    let idx = out.len();
    out.push((idx, s));
    Operand::Temp(idx)
}

fn linearize_into(e: &Expr, out: &mut Vec<(usize, Shallow)>) -> Shallow {
    match e {
        Expr::Int { .. } | Expr::Float { .. } | Expr::Bool(_) | Expr::Str(_) => Shallow::Lit(e.clone()),
        Expr::Unsupported { what } => Shallow::Unsupported { what: what.clone() },
        // The statement-oriented printers do not print DOM construction.
        Expr::Method { .. } | Expr::Param(_) | Expr::Concat(_) | Expr::ItemCall { .. } => {
            Shallow::Unsupported {
                what: "DOM construction (builder methods, parameters) is not linearized"
                    .to_string(),
            }
        }
        Expr::Call {
            class,
            method,
            args,
        } => Shallow::Call {
            class: class.clone(),
            method: method.clone(),
            args: args.iter().map(|a| operand(a, out)).collect(),
        },
        Expr::Variant {
            ty,
            shape,
            variant,
            args,
        } => Shallow::Variant {
            ty: ty.clone(),
            shape: *shape,
            variant: variant.clone(),
            args: args.iter().map(|a| operand(a, out)).collect(),
        },
        Expr::Struct { ty, fields } => Shallow::Struct {
            ty: ty.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), operand(v, out)))
                .collect(),
        },
        Expr::Vec { ty, elem, items } => Shallow::Vec {
            ty: ty.clone(),
            elem: elem.clone(),
            items: items
                .iter()
                .filter(|i| !(is_droppable_vec(ty) && i.contains_unsupported()))
                .map(|a| operand(a, out))
                .collect(),
        },
    }
}
