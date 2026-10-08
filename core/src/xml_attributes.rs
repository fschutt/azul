//! ONE table: which XML attribute sets what on a node.
//!
//! Every XML → DOM builder (`apply_xml_node_attributes` in this crate, and
//! azul-layout's streaming loader) reads an element's attributes through
//! [`setting_of`] / [`node_settings`] and lands them with [`apply_settings`];
//! the code generator (`azul_core::codegen::dom`, `codegen` feature) writes
//! the builder call that sets the same [`NodeSetting`]. One entry, both
//! directions: the loaders and the generated code cannot disagree.
//!
//! **Extensible.** An app, a component library or a widget adds an entry with
//! [`register_xml_attribute`]; a registered entry is looked up before the
//! builtin ones, so it may also change what a builtin attribute sets. An
//! attribute no entry takes keeps today's behaviour: it is ignored.
//!
//! Not in the table: what an attribute does to an element's TYPE or geometry
//! (`<img src width height>`, the SVG presentation and geometry attributes,
//! `<transient-window>`'s configuration). Those are the element's own
//! reading, not a setting any element can carry.

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use azul_css::{
    css::{CssDeclaration, CssPropertyValue},
    dynamic_selector::CssPropertyWithConditions,
    props::{
        property::{CssKeyMap, CssProperty},
        style::StyleDirection,
    },
    AzString,
};

use super::{parse_bool, XmlNode};
use crate::dom::{AttributeNameValue, AttributeType, IdOrClass, NodeData, TabIndex};

/// What an XML attribute sets on a node: the one vocabulary the XML → DOM
/// builders apply ([`apply_settings`]) and the code generator writes as a
/// builder call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeSetting {
    /// `id`: every whitespace-separated id (`.with_id(..)`).
    Ids(Vec<AzString>),
    /// `class`: every whitespace-separated class (`.with_class(..)`).
    Classes(Vec<AzString>),
    /// The keyboard focus (`tabindex`, `focusable`): `.with_tab_index(..)`.
    TabIndex(TabIndex),
    /// `contenteditable="true"`: the node is editable,
    /// `.with_contenteditable(true)`. (`false` is
    /// `Attribute(ContentEditable(false))`: it walls the subtree off inside an
    /// editable host.)
    Editable,
    /// A typed attribute the node carries: `.with_attribute(AttributeType::..)`.
    Attribute(AttributeType),
    /// The writing direction (`dir`): a `direction` declaration, BEFORE the
    /// `style` attribute's.
    Direction(StyleDirection),
    /// The `style` attribute: its declarations, last (an author's inline
    /// style wins).
    Style(AzString),
    /// Something the XML → DOM builders do themselves, or nothing, and that
    /// generated code cannot say yet (a localized text, a callback named in
    /// markup): why. The code generator writes it into the item's doc.
    NotExported(AzString),
}

/// Which elements an entry applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeScope {
    /// Every element.
    AnyElement,
    /// The form controls ([`is_form_control_tag`]).
    FormControls,
    /// The elements named (lowercase), e.g. the table elements a
    /// presentational attribute applies to ([`presentational_css`]).
    Tags(&'static [&'static str]),
}

impl AttributeScope {
    /// `true` if an element `tag` takes an attribute of this scope.
    #[must_use]
    pub fn admits(self, tag: &str) -> bool {
        match self {
            Self::AnyElement => true,
            Self::FormControls => is_form_control_tag(tag),
            Self::Tags(tags) => tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
        }
    }
}

/// The form controls: the elements whose HTML attributes (`type`, `value`,
/// `min`, `checked`, `data-*` ...) land as the typed attributes a
/// `Dom::create_input(..).with_attribute(..)` would carry. `<button>` is here
/// for its `type` / `name` / `value` / `disabled`: a `type="reset"` button is
/// recognised by its `InputType` attribute alone.
#[must_use]
pub fn is_form_control_tag(tag: &str) -> bool {
    [
        "input", "select", "option", "optgroup", "textarea", "datalist", "button",
    ]
    .iter()
    .any(|t| t.eq_ignore_ascii_case(tag))
}

/// One entry of the table.
#[derive(Clone, Copy)]
pub struct XmlAttribute {
    /// The attribute's name, lowercase (`tabindex`), or a prefix ending in
    /// `*` (`data-*`): every attribute that starts with it.
    pub name: &'static str,
    pub scope: AttributeScope,
    /// Where its setting lands among an element's others: the builders apply
    /// them in this order (a later tab index wins; within one order the
    /// document's order, e.g. a form control's attributes).
    pub order: u8,
    /// What `value` sets. `name` is the attribute's own lowercase name (for
    /// a prefix entry the full name). `None`: nothing (an unparsable number,
    /// `required="false"`).
    pub setting: fn(name: &str, value: &str) -> Option<NodeSetting>,
}

impl fmt::Debug for XmlAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XmlAttribute")
            .field("name", &self.name)
            .field("scope", &self.scope)
            .field("order", &self.order)
            .finish_non_exhaustive()
    }
}

impl XmlAttribute {
    /// `true` if this entry takes attribute `name` (lowercase) on `tag`.
    fn takes(&self, name: &str, tag: &str) -> bool {
        let named = match self.name.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == self.name,
        };
        named && self.scope.admits(tag)
    }
}

// ── the builtin entries ──

fn words(value: &str) -> Vec<AzString> {
    value.split_whitespace().map(AzString::from).collect()
}

fn ids(_: &str, value: &str) -> Option<NodeSetting> {
    let w = words(value);
    (!w.is_empty()).then_some(NodeSetting::Ids(w))
}

fn classes(_: &str, value: &str) -> Option<NodeSetting> {
    let w = words(value);
    (!w.is_empty()).then_some(NodeSetting::Classes(w))
}

fn focusable(_: &str, value: &str) -> Option<NodeSetting> {
    parse_bool(value).map(|f| {
        NodeSetting::TabIndex(if f {
            TabIndex::Auto
        } else {
            TabIndex::NoKeyboardFocus
        })
    })
}

fn tabindex(_: &str, value: &str) -> Option<NodeSetting> {
    let i = value.parse::<isize>().ok()?;
    Some(NodeSetting::TabIndex(match i {
        0 => TabIndex::Auto,
        i if i > 0 => TabIndex::OverrideInParent(u32::try_from(i).unwrap_or(u32::MAX)),
        _ => TabIndex::NoKeyboardFocus,
    }))
}

fn contenteditable(_: &str, value: &str) -> Option<NodeSetting> {
    // An explicit `false` is NOT "no attribute": inside an editable host it
    // walls its subtree off (HTML's inheritance rule) and keeps that subtree
    // out of the host's edit buffer. Both loaders used to disagree here.
    parse_bool(value).map(|b| {
        if b {
            NodeSetting::Editable
        } else {
            NodeSetting::Attribute(AttributeType::ContentEditable(false))
        }
    })
}

fn autofocus(_: &str, _: &str) -> Option<NodeSetting> {
    // Boolean attribute: presence is the value, as in HTML.
    Some(NodeSetting::Attribute(AttributeType::Autofocus))
}

fn placeholder(_: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Attribute(AttributeType::Placeholder(value.into())))
}

fn span(name: &str, value: &str) -> Option<NodeSetting> {
    let n = value.trim().parse::<i32>().ok()?;
    Some(NodeSetting::Attribute(if name == "colspan" {
        AttributeType::ColSpan(n)
    } else {
        AttributeType::RowSpan(n)
    }))
}

fn dir(_: &str, value: &str) -> Option<NodeSetting> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("rtl") {
        Some(NodeSetting::Direction(StyleDirection::Rtl))
    } else if v.eq_ignore_ascii_case("ltr") {
        Some(NodeSetting::Direction(StyleDirection::Ltr))
    } else {
        None
    }
}

/// `lang` / `xml:lang`: the element's content language (a BCP 47 tag), for
/// its subtree - `hyphens: auto` picks its hyphenation resource by it (CSS
/// Text 3 5.4), the accessibility tree reports it. Kept as written, an empty
/// value too: `lang=""` says "unknown" and hides an ancestor's language.
fn lang(_: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Attribute(AttributeType::Lang(value.trim().into())))
}

fn style(_: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Style(value.into()))
}

/// A presentational attribute (`<table width cellpadding bgcolor ..>`): kept
/// on the node verbatim, as HTML keeps it in the DOM. What it means for
/// rendering is the cascade's to say ([`presentational_css`], applied by
/// [`apply_presentational_hints`] when the DOM is styled), so an app that
/// builds `Dom::create_td().with_attribute(..)` gets the same result as
/// markup, and an app reads the attribute like any other.
fn presentational(name: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Attribute(AttributeType::Custom(
        AttributeNameValue {
            attr_name: name.into(),
            value: value.into(),
        },
    )))
}

/// The elements each presentational attribute applies to (HTML's
/// rendering section, "tables" and "flow content").
const HINT_WIDTH_TAGS: &[&str] = &["table", "td", "th", "col", "colgroup"];
const HINT_HEIGHT_TAGS: &[&str] = &["table", "td", "th", "tr"];
const HINT_BGCOLOR_TAGS: &[&str] = &["body", "table", "thead", "tbody", "tfoot", "tr", "td", "th"];
const HINT_TABLE_TAGS: &[&str] = &["table"];
const HINT_CELL_TAGS: &[&str] = &["td", "th"];
const HINT_ALIGN_TAGS: &[&str] = &[
    "table", "caption", "thead", "tbody", "tfoot", "tr", "td", "th", "col", "colgroup", "div",
    "p", "h1", "h2", "h3", "h4", "h5", "h6",
];
const HINT_VALIGN_TAGS: &[&str] = &["thead", "tbody", "tfoot", "tr", "td", "th", "col", "colgroup"];

/// A form control's HTML attribute as its typed `AttributeType`; one with no
/// typed variant as `Custom`. Boolean attributes follow HTML - PRESENT means
/// on, whatever the value - except that an explicit `"false"` means off.
fn form_control(name: &str, value: &str) -> Option<NodeSetting> {
    use AttributeType as A;
    let on = !value.trim().eq_ignore_ascii_case("false");
    let attr = match name {
        "type" => A::InputType(value.trim().into()),
        "name" => A::Name(value.into()),
        "value" => A::Value(value.into()),
        "min" => A::Min(value.into()),
        "max" => A::Max(value.into()),
        "step" => A::Step(value.into()),
        "pattern" => A::Pattern(value.into()),
        "autocomplete" => A::Autocomplete(value.into()),
        "aria-label" => A::AriaLabel(value.into()),
        "title" => A::Title(value.into()),
        "alt" => A::Alt(value.into()),
        "src" => A::Src(value.into()),
        "minlength" => A::MinLength(value.trim().parse::<i32>().ok()?),
        "maxlength" => A::MaxLength(value.trim().parse::<i32>().ok()?),
        "required" if on => A::Required,
        "disabled" if on => A::Disabled,
        "readonly" if on => A::Readonly,
        "selected" if on => A::Selected,
        "checked" => {
            if on {
                A::CheckedTrue
            } else {
                A::CheckedFalse
            }
        }
        "required" | "disabled" | "readonly" | "selected" => return None,
        _ => A::Custom(AttributeNameValue {
            attr_name: name.into(),
            value: value.into(),
        }),
    };
    Some(NodeSetting::Attribute(attr))
}

fn data(name: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Attribute(AttributeType::Data(AttributeNameValue {
        attr_name: name.into(),
        value: value.into(),
    })))
}

fn l10n(name: &str, _: &str) -> Option<NodeSetting> {
    // The builders give a `data-l10n="key"` element the key as its first,
    // translatable text child and the `data-l10n-*` arguments as its fluent
    // arguments (structure, not a node setting).
    Some(NodeSetting::NotExported(AzString::from(
        format!(
            "`{name}`: a localized text (the XML loaders translate it); generated code does not \
             write it yet"
        )
        .as_str(),
    )))
}

fn callback(name: &str, _: &str) -> Option<NodeSetting> {
    Some(NodeSetting::NotExported(AzString::from(
        format!(
            "`{name}` names a callback: generated code cannot bind a function by its name (wire \
             it with `with_callback`)"
        )
        .as_str(),
    )))
}

const fn entry(
    name: &'static str,
    scope: AttributeScope,
    order: u8,
    setting: fn(&str, &str) -> Option<NodeSetting>,
) -> XmlAttribute {
    XmlAttribute {
        name,
        scope,
        order,
        setting,
    }
}

use self::AttributeScope::{AnyElement, FormControls, Tags};

/// The builtin entries, first match wins (`data-l10n*` before `data-*`).
static BUILTIN: &[XmlAttribute] = &[
    entry("id", AnyElement, 0, ids),
    entry("class", AnyElement, 0, classes),
    entry("focusable", AnyElement, 1, focusable),
    entry("contenteditable", AnyElement, 2, contenteditable),
    entry("autofocus", AnyElement, 3, autofocus),
    entry("placeholder", AnyElement, 4, placeholder),
    entry("type", FormControls, 5, form_control),
    entry("name", FormControls, 5, form_control),
    entry("value", FormControls, 5, form_control),
    entry("min", FormControls, 5, form_control),
    entry("max", FormControls, 5, form_control),
    entry("step", FormControls, 5, form_control),
    entry("pattern", FormControls, 5, form_control),
    entry("autocomplete", FormControls, 5, form_control),
    entry("aria-label", FormControls, 5, form_control),
    entry("title", FormControls, 5, form_control),
    entry("alt", FormControls, 5, form_control),
    entry("src", FormControls, 5, form_control),
    entry("minlength", FormControls, 5, form_control),
    entry("maxlength", FormControls, 5, form_control),
    entry("required", FormControls, 5, form_control),
    entry("disabled", FormControls, 5, form_control),
    entry("readonly", FormControls, 5, form_control),
    entry("selected", FormControls, 5, form_control),
    entry("checked", FormControls, 5, form_control),
    entry("size", FormControls, 5, form_control),
    entry("rows", FormControls, 5, form_control),
    entry("cols", FormControls, 5, form_control),
    entry("multiple", FormControls, 5, form_control),
    entry("accept", FormControls, 5, form_control),
    entry("list", FormControls, 5, form_control),
    entry("label", FormControls, 5, form_control),
    entry("wrap", FormControls, 5, form_control),
    entry("form", FormControls, 5, form_control),
    entry("inputmode", FormControls, 5, form_control),
    entry("dirname", FormControls, 5, form_control),
    entry("capture", FormControls, 5, form_control),
    entry("tabindex", AnyElement, 6, tabindex),
    entry("colspan", AnyElement, 7, span),
    entry("rowspan", AnyElement, 8, span),
    entry("width", Tags(HINT_WIDTH_TAGS), 8, presentational),
    entry("height", Tags(HINT_HEIGHT_TAGS), 8, presentational),
    entry("bgcolor", Tags(HINT_BGCOLOR_TAGS), 8, presentational),
    entry("border", Tags(HINT_TABLE_TAGS), 8, presentational),
    entry("bordercolor", Tags(HINT_TABLE_TAGS), 8, presentational),
    entry("cellpadding", Tags(HINT_TABLE_TAGS), 8, presentational),
    entry("cellspacing", Tags(HINT_TABLE_TAGS), 8, presentational),
    entry("align", Tags(HINT_ALIGN_TAGS), 8, presentational),
    entry("valign", Tags(HINT_VALIGN_TAGS), 8, presentational),
    entry("nowrap", Tags(HINT_CELL_TAGS), 8, presentational),
    entry("dir", AnyElement, 9, dir),
    // `xml:lang` after `lang`: given both, it is the node's LAST `Lang`, the
    // one its readers take (HTML: the XML-namespace attribute wins).
    entry("lang", AnyElement, 9, lang),
    entry("xml:lang", AnyElement, 10, lang),
    entry("style", AnyElement, 10, style),
    entry("data-l10n*", AnyElement, 11, l10n),
    entry("data-*", FormControls, 5, data),
    entry("on*", AnyElement, 12, callback),
];

/// The builtin entries (see the module docs).
#[must_use]
pub fn builtin_attributes() -> &'static [XmlAttribute] {
    BUILTIN
}

#[cfg(feature = "std")]
static REGISTERED: std::sync::RwLock<Vec<XmlAttribute>> = std::sync::RwLock::new(Vec::new());

/// Add an entry to the table: every XML → DOM builder and the code generator
/// take it from now on. It is looked up before the builtin ones (a later
/// registration of the same name replaces an earlier one), so it may also
/// change what a builtin attribute sets.
#[cfg(feature = "std")]
pub fn register_xml_attribute(entry: XmlAttribute) {
    let mut r = REGISTERED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    r.retain(|e| e.name != entry.name);
    r.push(entry);
}

/// What `name="value"` sets on a `tag` element, with its entry's order;
/// `None` if no entry takes the attribute (it is ignored) or it sets nothing.
#[must_use]
pub fn setting_of(tag: &str, name: &str, value: &str) -> Option<(u8, NodeSetting)> {
    let key = name.trim().to_ascii_lowercase();
    #[cfg(feature = "std")]
    {
        let registered = REGISTERED
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(e) = registered.iter().rev().find(|e| e.takes(&key, tag)) {
            return (e.setting)(&key, value).map(|s| (e.order, s));
        }
    }
    let e = BUILTIN.iter().find(|e| e.takes(&key, tag))?;
    (e.setting)(&key, value).map(|s| (e.order, s))
}

/// Settings with their orders, in the order the builders apply them (by the
/// entries' orders; a stable sort keeps the document's order within one).
#[must_use]
pub fn ordered(settings: impl Iterator<Item = (u8, NodeSetting)>) -> Vec<NodeSetting> {
    let mut v: Vec<(u8, NodeSetting)> = settings.collect();
    v.sort_by_key(|(order, _)| *order);
    v.into_iter().map(|(_, s)| s).collect()
}

/// Everything the attributes of `xml_node`, a `tag` element (lowercase), set
/// on its node, in the order the builders apply them.
#[must_use]
pub fn node_settings(xml_node: &XmlNode, tag: &str) -> Vec<NodeSetting> {
    ordered(
        xml_node
            .attributes
            .as_slice()
            .iter()
            .filter_map(|pair| setting_of(tag, pair.key.as_str(), pair.value.as_str())),
    )
}

/// Land an element's `settings` ([`node_settings`] / [`setting_of`] +
/// [`ordered`]) on `node`, the way every XML → DOM builder does: ids and
/// classes together, the typed attributes after the node's own, a later tab
/// index over an earlier one, and ONE inline style - `intrinsic` (the
/// element's own sizing, e.g. an `<svg>`'s), then the writing direction, then
/// the `style` attribute (the author's inline style wins). `css_key_map`: the
/// parser's key map if the caller has one (else it is built when a `style`
/// needs it). `intern` makes an id / class string (a loader may share them).
pub fn apply_settings(
    node: &mut NodeData,
    settings: Vec<NodeSetting>,
    intrinsic: Vec<CssPropertyWithConditions>,
    css_key_map: Option<&CssKeyMap>,
    intern: &mut dyn FnMut(&str) -> AzString,
) {
    let mut ids_and_classes: Vec<IdOrClass> = Vec::new();
    let mut attributes: Vec<AttributeType> = Vec::new();
    let mut direction: Option<StyleDirection> = None;
    let mut style_text: Option<AzString> = None;
    for s in settings {
        match s {
            NodeSetting::Ids(v) => {
                ids_and_classes.extend(v.iter().map(|i| IdOrClass::Id(intern(i.as_str()))));
            }
            NodeSetting::Classes(v) => {
                ids_and_classes.extend(v.iter().map(|c| IdOrClass::Class(intern(c.as_str()))));
            }
            NodeSetting::TabIndex(t) => node.set_tab_index(t),
            NodeSetting::Editable => node.set_contenteditable(true),
            NodeSetting::Attribute(a) => attributes.push(a),
            NodeSetting::Direction(d) => direction = Some(d),
            NodeSetting::Style(s) => style_text = Some(s),
            NodeSetting::NotExported(_) => {}
        }
    }
    if !ids_and_classes.is_empty() {
        node.set_ids_and_classes(ids_and_classes.into());
    }
    if !attributes.is_empty() {
        let mut all = node.attributes().clone().into_library_owned_vec();
        all.extend(attributes);
        node.set_attributes(all.into());
    }
    // The element's presentational hints (what its attributes say about its
    // style - `width`, `font-size`, `dir`) are the weakest author rules: any
    // stylesheet rule and the `style` attribute beat them. They used to be
    // stored as inline style, which beats every stylesheet.
    let mut hints = intrinsic;
    if let Some(d) = direction {
        hints.push(CssPropertyWithConditions::simple(CssProperty::Direction(
            CssPropertyValue::Exact(d),
        )));
    }
    let mut inline = Vec::new();
    if let Some(s) = style_text {
        if let Some(map) = css_key_map { inline.extend(style_declarations(s.as_str(), map)) } else {
            let map = azul_css::props::property::get_css_key_map();
            inline.extend(style_declarations(s.as_str(), &map));
        }
    }
    if hints.is_empty() && inline.is_empty() {
        return;
    }
    let mut rules = azul_css::css::Css::from(
        azul_css::dynamic_selector::CssPropertyWithConditionsVec::from(hints),
    )
    .rules
    .into_library_owned_vec();
    for rule in &mut rules {
        rule.priority = azul_css::css::rule_priority::PRESENTATIONAL;
    }
    rules.extend(
        azul_css::css::Css::from(azul_css::dynamic_selector::CssPropertyWithConditionsVec::from(
            inline,
        ))
        .rules
        .into_library_owned_vec(),
    );
    node.style = azul_css::css::Css::new(rules);
}

/// The static declarations of a `style` attribute (`key: value; ...`).
#[must_use]
pub fn style_declarations(style: &str, css_key_map: &CssKeyMap) -> Vec<CssPropertyWithConditions> {
    let mut parsed = Vec::new();
    // Declarations end at a TOP-LEVEL `;`: one inside parentheses or a quoted
    // string is part of a value (`url(data:image/png;base64,...)`, which a
    // plain `split(';')` cut in two and lost).
    for decl in azul_css::props::basic::parse::split_top_level(style, |b| b == b';') {
        // The key ends at the FIRST colon; the value keeps every later one
        // (`font-family: system:ui`, `url(https://...)`).
        let Some((key, value)) = decl.split_once(':') else {
            continue;
        };
        // Called for its side effect (writes the parsed declarations into
        // `parsed`); the returned value is intentionally discarded.
        drop(azul_css::parser2::parse_css_declaration(
            key.trim(),
            value.trim(),
            azul_css::parser2::ErrorLocationRange::default(),
            css_key_map,
            &mut Vec::new(),
            &mut parsed,
        ));
    }
    parsed
        .into_iter()
        .filter_map(|d| match d {
            CssDeclaration::Static(s) => Some(CssPropertyWithConditions::simple(s)),
            CssDeclaration::Dynamic(_) | CssDeclaration::CustomProperty(_) => None,
        })
        .collect()
}

/// The `direction` declaration of a writing direction, as CSS text
/// (`direction: rtl;`).
#[must_use]
pub fn direction_css(d: StyleDirection) -> String {
    let v = match d {
        StyleDirection::Rtl => "rtl",
        StyleDirection::Ltr => "ltr",
    };
    format!("direction: {v};")
}

// ── presentational hints ──
//
// HTML's rendering section maps the legacy attributes of some elements to
// CSS ("presentational hints"): `<table width="600" cellpadding="0"
// bgcolor="#fff">`, `<td align="center" valign="top" nowrap>`. The attribute
// table keeps them on the node verbatim ([`presentational`]); here is what
// they mean, and the pass that lands them when a DOM is styled.
//
// Precedence: the declarations are put in FRONT of the element's inline
// style, so its `style` attribute still wins. HTML ranks hints below every
// author stylesheet rule as well; azul's inline declarations outrank the
// stylesheet, so a stylesheet rule cannot override a hint yet (a cascade
// layer for hints would; see scripts/TABLE_A_2026_10_01.md).

/// The value of attribute `name` (case-insensitive) in `attributes`, the
/// first one if it repeats (HTML keeps the first).
fn hint_attr<'a>(attributes: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// HTML's "rules for parsing integers": optional ASCII whitespace, an
/// optional sign, then at least one ASCII digit; anything after the digits
/// is ignored (`1foo` is 1, `1%` is 1). `None` on an error.
fn parse_html_integer(value: &str) -> Option<i64> {
    let s = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let end = digits
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    let mut n: i64 = 0;
    for b in digits[..end].bytes() {
        n = n.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    Some(if negative { -n } else { n })
}

/// HTML's "rules for parsing non-negative integers": an integer that is not
/// negative (`-0` is 0).
fn parse_html_non_negative(value: &str) -> Option<i64> {
    parse_html_integer(value).filter(|n| *n >= 0)
}

/// HTML's "rules for parsing dimension values" as a CSS length: digits
/// (optionally with a fraction) then `%` for a percentage, anything else
/// (`px`, nothing, garbage after the digits) a length in pixels. No sign is
/// allowed. With `ignore_zero` a zero is no value ("maps to the dimension
/// property (ignoring zero)").
fn parse_html_dimension(value: &str, ignore_zero: bool) -> Option<String> {
    let s = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let int_end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    if int_end == 0 {
        return None;
    }
    let mut end = int_end;
    let rest = &s[int_end..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let frac_len = fraction
            .bytes()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(fraction.len());
        if frac_len > 0 {
            end = int_end + 1 + frac_len;
        }
    }
    let number: f32 = s[..end].parse().ok()?;
    if !number.is_finite() || (ignore_zero && number == 0.0) {
        return None;
    }
    let percent = s[end..].starts_with('%');
    Some(if percent {
        format!("{number}%")
    } else {
        format!("{number}px")
    })
}

/// HTML's "rules for parsing a legacy colour value": a named colour as
/// itself, `#rgb` expanded, and everything else through the legacy
/// algorithm that makes `ff0000` (no `#`) red and `#1c3d5a` itself. As a CSS
/// colour (`red`, `#rrggbb`); `None` for an empty value or `transparent`.
fn parse_html_legacy_color(value: &str) -> Option<String> {
    let s = value.trim_matches(|c: char| c.is_ascii_whitespace());
    if s.is_empty() || s.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_alphabetic()) {
        let name = s.to_ascii_lowercase();
        if azul_css::props::basic::color::parse_css_color(&name).is_ok() {
            return Some(name);
        }
    }
    let hex3 = s
        .strip_prefix('#')
        .filter(|h| h.len() == 3 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    if let Some(h) = hex3 {
        let mut out = String::from("#");
        for c in h.chars() {
            out.push(c.to_ascii_lowercase());
            out.push(c.to_ascii_lowercase());
        }
        return Some(out);
    }
    // The legacy algorithm: at most 128 characters, no leading `#`, every
    // non-hex digit a `0`, padded to a multiple of three, split in three,
    // each part cut to its last 8 digits, common leading zeros dropped
    // while longer than 2, then the first two digits of each part.
    let mut digits: Vec<char> = s
        .chars()
        .take(128)
        .collect::<Vec<char>>();
    if digits.first() == Some(&'#') {
        digits.remove(0);
    }
    for c in &mut digits {
        if !c.is_ascii_hexdigit() {
            *c = '0';
        }
    }
    while digits.is_empty() || !digits.len().is_multiple_of(3) {
        digits.push('0');
    }
    let len = digits.len() / 3;
    let mut parts: Vec<Vec<char>> = digits.chunks(len).map(<[char]>::to_vec).collect();
    if len > 8 {
        for p in &mut parts {
            let cut = p.len() - 8;
            p.drain(..cut);
        }
    }
    while parts[0].len() > 2 && parts.iter().all(|p| p.first() == Some(&'0')) {
        for p in &mut parts {
            p.remove(0);
        }
    }
    let mut out = String::from("#");
    for p in &parts {
        let two: String = p.iter().take(2).collect();
        let byte = u8::from_str_radix(&two, 16).unwrap_or(0);
        out.push_str(&format!("{byte:02x}"));
    }
    Some(out)
}

/// `text-align` of an `align` attribute on a block or a cell (`middle` is
/// HTML's other spelling of `center`).
fn hint_text_align(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("left") {
        Some("left")
    } else if v.eq_ignore_ascii_case("right") {
        Some("right")
    } else if v.eq_ignore_ascii_case("center") || v.eq_ignore_ascii_case("middle") {
        Some("center")
    } else if v.eq_ignore_ascii_case("justify") {
        Some("justify")
    } else {
        None
    }
}

/// `vertical-align` of a `valign` attribute.
fn hint_vertical_align(value: &str) -> Option<&'static str> {
    let v = value.trim();
    ["top", "middle", "bottom", "baseline"]
        .into_iter()
        .find(|k| v.eq_ignore_ascii_case(k))
}

/// One declaration per side: `{prefix}-{side}{suffix}: {value}`.
fn push_sides(out: &mut Vec<String>, prefix: &str, suffix: &str, value: &str) {
    for side in ["top", "right", "bottom", "left"] {
        out.push(format!("{prefix}-{side}{suffix}: {value}"));
    }
}

/// The `border` attribute of a table as a width in pixels: absent is
/// `None`, unparsable (`""`, `foo`, `-1`) is 1 (HTML's default).
fn table_border_width(table: &[(&str, &str)]) -> Option<i64> {
    hint_attr(table, "border").map(|v| parse_html_non_negative(v).unwrap_or(1))
}

/// The CSS declarations (`prop: value; ...`) HTML's rendering section maps
/// the presentational attributes of a `tag` element to. `attributes` are the
/// element's own, `table` the attributes of the nearest enclosing `table`
/// (what its `cellpadding` / `border` / `bordercolor` give a `td` / `th`;
/// empty for other elements). Empty when nothing applies.
///
/// - `width` / `height` (table, cells, `col`): the dimension properties,
///   ignoring zero (a row's `height` keeps zero).
/// - `bgcolor`: `background-color`, by the legacy colour rules.
/// - `border` (table): `border-*-width` (an unparsable value is 1px) and,
///   when not zero, `outset` borders (`solid` with a `bordercolor`); the
///   table's cells then get a 1px `inset` (`solid`) border.
/// - `bordercolor` (table): `border-*-color`, on the table and its cells.
/// - `cellspacing`: `border-spacing`. `cellpadding`: the cells' padding.
/// - `align`: on a table `float: left|right` or auto margins (`center`); on
///   a caption `caption-side` (`top` / `bottom`) or `text-align`; elsewhere
///   `text-align`. `valign`: `vertical-align`. `nowrap`: `white-space:
///   nowrap`.
#[must_use]
pub fn presentational_css(
    tag: &str,
    attributes: &[(&str, &str)],
    table: &[(&str, &str)],
) -> String {
    let tag = tag.to_ascii_lowercase();
    let tag = tag.as_str();
    let mut out: Vec<String> = Vec::new();
    let attr = |name: &str| hint_attr(attributes, name);
    let is_cell = matches!(tag, "td" | "th");

    if HINT_WIDTH_TAGS.contains(&tag) {
        if let Some(w) = attr("width").and_then(|v| parse_html_dimension(v, true)) {
            out.push(format!("width: {w}"));
        }
    }
    if HINT_HEIGHT_TAGS.contains(&tag) {
        let ignore_zero = tag != "tr";
        if let Some(h) = attr("height").and_then(|v| parse_html_dimension(v, ignore_zero)) {
            out.push(format!("height: {h}"));
        }
    }
    if HINT_BGCOLOR_TAGS.contains(&tag) {
        if let Some(c) = attr("bgcolor").and_then(parse_html_legacy_color) {
            out.push(format!("background-color: {c}"));
        }
    }
    if tag == "table" {
        let color = attr("bordercolor").and_then(parse_html_legacy_color);
        if let Some(n) = table_border_width(attributes) {
            push_sides(&mut out, "border", "-width", &format!("{n}px"));
            if n > 0 {
                let style = if color.is_some() { "solid" } else { "outset" };
                push_sides(&mut out, "border", "-style", style);
            }
        }
        if let Some(c) = &color {
            push_sides(&mut out, "border", "-color", c);
        }
        if let Some(n) = attr("cellspacing").and_then(parse_html_non_negative) {
            out.push(format!("border-spacing: {n}px"));
        }
    }
    if let Some(align) = attr("align").filter(|_| HINT_ALIGN_TAGS.contains(&tag)) {
        let v = align.trim();
        if tag == "table" {
            if v.eq_ignore_ascii_case("left") {
                out.push(String::from("float: left"));
            } else if v.eq_ignore_ascii_case("right") {
                out.push(String::from("float: right"));
            } else if v.eq_ignore_ascii_case("center") || v.eq_ignore_ascii_case("middle") {
                out.push(String::from("margin-left: auto"));
                out.push(String::from("margin-right: auto"));
            }
        } else if tag == "caption"
            && (v.eq_ignore_ascii_case("top") || v.eq_ignore_ascii_case("bottom"))
        {
            out.push(format!("caption-side: {}", v.to_ascii_lowercase()));
        } else if let Some(a) = hint_text_align(v) {
            out.push(format!("text-align: {a}"));
        }
    }
    if HINT_VALIGN_TAGS.contains(&tag) {
        if let Some(a) = attr("valign").and_then(hint_vertical_align) {
            out.push(format!("vertical-align: {a}"));
        }
    }
    if is_cell {
        if attr("nowrap").is_some() {
            out.push(String::from("white-space: nowrap"));
        }
        if let Some(n) = hint_attr(table, "cellpadding").and_then(parse_html_non_negative) {
            push_sides(&mut out, "padding", "", &format!("{n}px"));
        }
        if table_border_width(table).is_some_and(|n| n > 0) {
            let color = hint_attr(table, "bordercolor").and_then(parse_html_legacy_color);
            push_sides(&mut out, "border", "-width", "1px");
            let style = if color.is_some() { "solid" } else { "inset" };
            push_sides(&mut out, "border", "-style", style);
            if let Some(c) = &color {
                push_sides(&mut out, "border", "-color", c);
            }
        }
    }
    out.join("; ")
}

/// The tag [`presentational_css`] knows a node type by (`None` for the
/// elements no presentational attribute applies to).
fn hint_tag(node_type: &crate::dom::NodeType) -> Option<&'static str> {
    use crate::dom::NodeType as N;
    Some(match node_type {
        N::Body => "body",
        N::Table => "table",
        N::Caption => "caption",
        N::THead => "thead",
        N::TBody => "tbody",
        N::TFoot => "tfoot",
        N::Tr => "tr",
        N::Td => "td",
        N::Th => "th",
        N::Col => "col",
        N::ColGroup => "colgroup",
        N::Div => "div",
        N::P => "p",
        N::H1 => "h1",
        N::H2 => "h2",
        N::H3 => "h3",
        N::H4 => "h4",
        N::H5 => "h5",
        N::H6 => "h6",
        _ => return None,
    })
}

/// A node's `Custom` attributes as owned `(name, value)` pairs - where the
/// attribute table keeps the presentational ones ([`presentational`]).
fn custom_attributes(node: &NodeData) -> Vec<(String, String)> {
    node.attributes()
        .as_ref()
        .iter()
        .filter_map(|a| match a {
            AttributeType::Custom(nv) => {
                Some((String::from(nv.attr_name.as_str()), String::from(nv.value.as_str())))
            }
            _ => None,
        })
        .collect()
}

/// Land the presentational hints of every element of a DOM about to be
/// styled: each element's [`presentational_css`] (with its nearest
/// enclosing table's attributes for a cell) as inline declarations IN FRONT
/// of its own inline style. `node_data` and `hierarchy` are the flat arena
/// of the DOM (pre-order: a parent before its children). Called once per
/// styled DOM (`StyledDom` creation); a DOM without presentational
/// attributes costs one scan.
pub fn apply_presentational_hints(
    node_data: &mut [NodeData],
    hierarchy: &[crate::styled_dom::NodeHierarchyItem],
) {
    let n = node_data.len().min(hierarchy.len());
    let carries_hint = |nd: &NodeData| {
        hint_tag(nd.get_node_type()).is_some()
            && nd
                .attributes()
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::Custom(_)))
    };
    if !node_data[..n].iter().any(carries_hint) {
        return;
    }

    // The nearest `table` ancestor of every node (a cell's table).
    let mut nearest_table: Vec<Option<usize>> = alloc::vec![None; n];
    for i in 0..n {
        nearest_table[i] = hierarchy[i]
            .parent_id()
            .map(|p| p.index())
            .filter(|p| *p < i)
            .and_then(|p| {
                if matches!(node_data[p].get_node_type(), crate::dom::NodeType::Table) {
                    Some(p)
                } else {
                    nearest_table[p]
                }
            });
    }

    let mut key_map: Option<CssKeyMap> = None;
    for i in 0..n {
        let Some(tag) = hint_tag(node_data[i].get_node_type()) else {
            continue;
        };
        let own = custom_attributes(&node_data[i]);
        let table = if matches!(tag, "td" | "th") {
            nearest_table[i].map_or_else(Vec::new, |t| custom_attributes(&node_data[t]))
        } else {
            Vec::new()
        };
        if own.is_empty() && table.is_empty() {
            continue;
        }
        let own_ref: Vec<(&str, &str)> = own.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let table_ref: Vec<(&str, &str)> =
            table.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let css = presentational_css(tag, &own_ref, &table_ref);
        if css.is_empty() {
            continue;
        }
        let map = key_map.get_or_insert_with(azul_css::props::property::get_css_key_map);
        let declarations = style_declarations(&css, map);
        if declarations.is_empty() {
            continue;
        }
        let hints = azul_css::css::Css::from(
            azul_css::dynamic_selector::CssPropertyWithConditionsVec::from(declarations),
        );
        let node = &mut node_data[i];
        let mut rules = hints.rules.into_library_owned_vec();
        rules.extend(core::mem::take(&mut node.style.rules).into_library_owned_vec());
        node.style.rules = rules.into();
    }
}

#[cfg(test)]
mod tests {
    use azul_css::props::style::background::StyleBackgroundContent;

    use super::*;

    #[test]
    fn a_lang_attribute_lands_on_its_node_as_the_content_language() {
        // CSS Text 3 5.4: `hyphens: auto` hyphenates in the CONTENT
        // LANGUAGE, which markup states with `lang` (`xml:lang` in XML). The
        // table had no entry for either, so every loader dropped `lang="en"`
        // and nothing downstream could read it (pdfocr engine issue 3).
        for name in ["lang", "LANG", "xml:lang"] {
            assert_eq!(
                setting_of("div", name, "en-GB").map(|(_, s)| s),
                Some(NodeSetting::Attribute(AttributeType::Lang(AzString::from(
                    "en-GB"
                )))),
                "`{name}=\"en-GB\"` on a div"
            );
        }
    }

    #[test]
    fn a_data_url_in_a_style_attribute_keeps_its_base64_payload() {
        // `style.split(';')` cut the declaration at the `;` inside
        // `url(data:image/png;base64,...)`: the background was dropped (SYSUI8).
        // A `;` inside parentheses or a quoted string separates nothing.
        let map = azul_css::props::property::get_css_key_map();
        let declarations = style_declarations(
            "background-image: url(data:image/png;base64,iVBORw0KGgo=); color: red",
            &map,
        );
        let image = declarations.iter().find_map(|d| match &d.property {
            CssProperty::BackgroundContent(CssPropertyValue::Exact(layers)) => {
                layers.as_ref().iter().find_map(|layer| match layer {
                    StyleBackgroundContent::Image(url) => Some(url.as_str().to_string()),
                    _ => None,
                })
            }
            _ => None,
        });
        assert_eq!(
            image.as_deref(),
            Some("data:image/png;base64,iVBORw0KGgo="),
            "the whole data URL: {declarations:?}"
        );
        assert!(
            declarations
                .iter()
                .any(|d| matches!(d.property, CssProperty::TextColor(_))),
            "the declaration after it is read too: {declarations:?}"
        );
    }
}
