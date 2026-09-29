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
#[derive(Debug, Clone, PartialEq)]
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
}

impl AttributeScope {
    /// `true` if an element `tag` takes an attribute of this scope.
    #[must_use]
    pub fn admits(self, tag: &str) -> bool {
        match self {
            Self::AnyElement => true,
            Self::FormControls => is_form_control_tag(tag),
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

fn style(_: &str, value: &str) -> Option<NodeSetting> {
    Some(NodeSetting::Style(value.into()))
}

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

use self::AttributeScope::{AnyElement, FormControls};

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
    entry("dir", AnyElement, 9, dir),
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
    let mut props = intrinsic;
    if let Some(d) = direction {
        props.push(CssPropertyWithConditions::simple(CssProperty::Direction(
            CssPropertyValue::Exact(d),
        )));
    }
    if let Some(s) = style_text {
        match css_key_map {
            Some(map) => props.extend(style_declarations(s.as_str(), map)),
            None => {
                let map = azul_css::props::property::get_css_key_map();
                props.extend(style_declarations(s.as_str(), &map));
            }
        }
    }
    if !props.is_empty() {
        node.set_css_props(props.into());
    }
}

/// The static declarations of a `style` attribute (`key: value; ...`).
#[must_use]
pub fn style_declarations(style: &str, css_key_map: &CssKeyMap) -> Vec<CssPropertyWithConditions> {
    let mut parsed = Vec::new();
    for decl in style.split(';') {
        let mut kv = decl.split(':');
        let Some(key) = kv.next() else {
            continue;
        };
        let Some(value) = kv.next() else {
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
