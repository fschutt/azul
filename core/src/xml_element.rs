//! One element of markup -> one DOM node: the BUILTIN RENDERERS.
//!
//! An element's tag names its component in the [`ComponentMap`] (`div` is
//! `builtin:div`, `svg:rect` is `builtin:rect`). A builtin component's
//! renderer is a Rust function here: it creates the node and lands the
//! element's attributes on it - the ones every node carries (`id`, `class`,
//! `href`, `style` ...) and the ones its kind reads (an `<svg>`'s `viewBox`, a
//! shape's geometry and paint, an `<img>`'s `src`).
//!
//! Every XML -> DOM builder - core's tree walker ([`super::str_to_dom_unstyled`]),
//! core's arena walker ([`super::str_to_dom`]) and layout's streaming document
//! loader - instantiates its elements with [`render_element`] and nowhere else,
//! and asks [`child_role`] what a child element contributes. The builders only
//! walk the markup.

use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::Write as _;

use azul_css::{
    dynamic_selector::CssPropertyWithConditions, props::property::CssKeyMap, AzString,
};

use super::{ComponentMap, ComponentSource};
use crate::dom::{NodeData, NodeType};

/// Where an element sits: what some tags mean depends on it (a `<rect>`
/// inside an `<svg>` is a shape, outside one a `<div>`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementScope {
    /// Inside an `<svg>`.
    pub inside_svg: bool,
}

impl ElementScope {
    /// The scope of the children of an element `tag` (lowercase) in this one.
    #[must_use]
    pub fn for_children_of(self, tag: &str) -> Self {
        Self {
            inside_svg: self.inside_svg || tag == "svg",
        }
    }
}

/// An element as a renderer reads it.
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    /// The tag, lowercase, its namespace prefix (`svg:`) resolved.
    pub tag: &'a str,
    /// The attributes in document order, names as written.
    pub attributes: &'a [(&'a str, &'a str)],
    /// Where it sits.
    pub scope: ElementScope,
    /// The `@font-face`s in scope (an enclosing `<svg>`'s stylesheet), the
    /// fonts a text's `font-family` names first.
    pub font_faces: &'a [FontFace],
}

/// An `@font-face` of a stylesheet in markup: the family name it declares and
/// its font, made from its `src` by the loader ([`FontSourceFn`]).
///
/// SCOPED to the element whose stylesheet declares it (an `<svg>`): two pages'
/// `F1`s are two fonts.
#[derive(Debug, Clone)]
pub struct FontFace {
    pub family: String,
    pub font: azul_css::props::basic::FontRef,
}

/// Makes the font an `@font-face`'s `src` names (`data:font/otf;base64,...`):
/// supplied by the loader that can decode and parse one (layout's). Without
/// one, markup's `@font-face`s are not loaded.
pub type FontSourceFn = fn(&str) -> Option<azul_css::props::basic::FontRef>;

/// The `@font-face` fonts of a walk of the markup: the loader's
/// [`FontSourceFn`] (none: no font is loaded) and the faces in scope.
#[derive(Debug, Clone, Copy, Default)]
pub struct FontScope<'a> {
    pub source: Option<FontSourceFn>,
    pub faces: &'a [FontFace],
}

impl FontScope<'_> {
    /// The fonts the `@font-face`s of `css` declare, made by the source.
    #[must_use]
    pub fn load(&self, css: &str) -> Vec<FontFace> {
        let Some(source) = self.source else {
            return Vec::new();
        };
        font_face_rules(css)
            .into_iter()
            .filter_map(|(family, src)| {
                Some(FontFace {
                    family,
                    font: source(&src)?,
                })
            })
            .collect()
    }
}

/// The `@font-face` rules of a stylesheet's text, as `(family, src)`: the
/// family name unquoted, `src`'s first `url(...)` (unquoted).
///
/// A rule without either is skipped. Read here because the CSS parser drops
/// at-rules it does not apply; a `data:` URL's `;` and `,` stay inside it.
#[must_use]
pub fn font_face_rules(css: &str) -> Vec<(String, String)> {
    let unquote = |v: &str| String::from(v.trim().trim_matches(|c| c == '"' || c == '\''));
    let mut faces = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("@font-face") {
        rest = &rest[at + "@font-face".len()..];
        let Some(open) = rest.find('{') else {
            break;
        };
        // The block's end: the first `}` outside quotes and parentheses.
        let body = &rest[open + 1..];
        let (mut depth, mut quote, mut end) = (0i32, None::<char>, body.len());
        for (i, c) in body.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '"' | '\'') => quote = Some(c),
                (None, '(') => depth += 1,
                (None, ')') => depth -= 1,
                (None, '}') if depth <= 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        let block = &body[..end];
        rest = body.get(end + 1..).unwrap_or("");
        let family = block.find("font-family").and_then(|at| {
            let value = &block[at + "font-family".len()..];
            let value = value.trim_start().strip_prefix(':')?;
            Some(unquote(value.split(';').next()?))
        });
        let src = block.find("src").and_then(|at| {
            let value = &block[at + 3..];
            let start = value.find("url(")? + 4;
            let value = &value[start..];
            Some(unquote(&value[..value.find(')')?]))
        });
        if let (Some(family), Some(src)) = (family, src) {
            if !family.is_empty() && !src.is_empty() {
                faces.push((family, src));
            }
        }
    }
    faces
}

impl<'a> Element<'a> {
    /// The value of the attribute `name` (the first one of that name).
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&'a str> {
        self.attributes
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    }

    /// The attributes as `(name, value)` pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        self.attributes.iter().copied()
    }
}

/// How a builder lands strings and styles.
///
/// The CSS key map its `style` attributes parse with (`None`: built when one is
/// needed) and the function that makes an id / class / attribute string (a
/// document loader shares them in an arena).
pub struct Landing<'m> {
    pub css_key_map: Option<&'m CssKeyMap>,
    pub intern: &'m mut dyn FnMut(&str) -> AzString,
}

impl core::fmt::Debug for Landing<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Landing")
            .field("css_key_map", &self.css_key_map.is_some())
            .finish_non_exhaustive()
    }
}

/// A builtin component's renderer.
pub type BuiltinRenderFn = fn(&Element<'_>, &mut Landing<'_>) -> NodeData;

/// What a child element contributes to its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildRole {
    /// A node of its own.
    Node,
    /// A stylesheet: `<style>` is no node, its text is CSS for the parent's
    /// subtree (an `<svg>`'s, wherever it sits inside it).
    Stylesheet,
    /// Nothing, subtree and all: an element that draws nothing
    /// ([`super::element_draws_nothing`] - an icon's RDF metadata).
    Nothing,
}

/// What a child element `raw_tag` contributes, in the scope of its parent's
/// children.
#[must_use]
pub fn child_role(children_scope: ElementScope, raw_tag: &str) -> ChildRole {
    if raw_tag.eq_ignore_ascii_case("style") {
        ChildRole::Stylesheet
    } else if children_scope.inside_svg
        && super::element_draws_nothing(raw_tag, &raw_tag.to_ascii_lowercase())
    {
        ChildRole::Nothing
    } else {
        ChildRole::Node
    }
}

/// The message key of a `data-l10n="key"` element (`None` without one, or with
/// an empty one).
///
/// Its translation is the element's text: every builder gives the element the
/// key as its FIRST child, a text node marked localizable (`AzString::tr`).
#[must_use]
pub fn l10n_key<'a>(attributes: &[(&'a str, &'a str)]) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| *key == "data-l10n")
        .map(|(_, value)| *value)
        .filter(|key| !key.is_empty())
}

/// The tag a builder hands [`render_element`]: lowercase, with an `svg:`,
/// `html:` or `xhtml:` namespace prefix dropped.
///
/// HTML and SVG names are ASCII-case-insensitive: `TABLE` is a table,
/// `linearGradient` a gradient. The namespace prefixes go because their
/// elements ARE the builtins, unless a component library has that name. Any
/// other prefix stays: Outlook's `<o:p>` is a foreign element, not a `<p>`.
#[must_use]
pub fn element_tag(map: &ComponentMap, raw_tag: &str) -> String {
    let tag = raw_tag.to_ascii_lowercase();
    match tag.split_once(':') {
        Some((prefix, name))
            if matches!(prefix, "svg" | "html" | "xhtml")
                && !map.libraries.iter().any(|lib| lib.name.as_str() == prefix) =>
        {
            String::from(name)
        }
        _ => tag,
    }
}

/// THE instantiation: the node `element` is, by the component its tag names in
/// `map` - a builtin component by its renderer ([`builtin_renderer`]).
///
/// A tag no library has is a `<div>` with its attributes (HTML's unknown
/// element), as is a library component for now: the builders do not expand user
/// components.
#[must_use]
pub fn render_element(
    map: &ComponentMap,
    element: &Element<'_>,
    landing: &mut Landing<'_>,
) -> NodeData {
    let builtin = match map.get_by_qualified_name(element.tag) {
        Some(def) if !matches!(def.source, ComponentSource::Builtin) => None,
        Some(def) => Some(def.id.name.as_str()),
        // A map without the builtin library still knows the builtins.
        None => Some(element.tag),
    };
    let render: BuiltinRenderFn =
        builtin.map_or(render_generic, |name| builtin_renderer(name, element.scope));
    render(element, landing)
}

/// The renderer of the builtin component `name` in `scope`.
#[must_use]
pub fn builtin_renderer(name: &str, scope: ElementScope) -> BuiltinRenderFn {
    match name {
        "img" => render_img,
        "svg" => render_svg,
        "path" | "circle" | "rect" | "ellipse" | "line" | "polygon" | "polyline"
            if scope.inside_svg =>
        {
            render_svg_shape
        }
        "g" if scope.inside_svg => render_svg_group,
        "text" if scope.inside_svg => render_svg_text,
        "tspan" if scope.inside_svg => render_svg_tspan,
        "image" if scope.inside_svg => render_svg_image,
        _ => render_generic,
    }
}

// ---- the renderers ----

/// Any element: its node type ([`super::tag_to_node_type`], a `<div>` for a
/// tag it does not know) and the attributes every node carries.
fn render_generic(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let mut node = NodeData::create_node(super::tag_to_node_type(element.tag));
    land_common(&mut node, element, Vec::new(), landing);
    node
}

/// `<img src width height>`: an `Image` whose placeholder carries the `src`
/// (as UTF-8 bytes in its tag). The bytes are not resolved here: a renderer
/// (printpdf, the compositor ...) looks the image up by it. `width` /
/// `height` are its intrinsic size (CSS still overrides).
fn render_img(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let mut node = NodeData::create_node(super::tag_to_node_type(element.tag));
    if let Some(src) = element.attribute("src") {
        let size = |key: &str| {
            element
                .attribute(key)
                .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<usize>().ok())
                .unwrap_or(0)
        };
        let image = crate::resources::ImageRef::null_image(
            size("width"),
            size("height"),
            crate::resources::RawImageFormat::RGBA8,
            src.as_bytes().to_vec(),
        );
        node.set_node_type(NodeType::Image(azul_css::css::BoxOrStatic::heap(image)));
    }
    land_common(&mut node, element, Vec::new(), landing);
    node
}

/// `<svg>`: its own viewport and the positioning context of its shapes.
///
/// Two things come off the element:
///
/// * the `viewBox`, the element's USER-SPACE coordinate system. An ABSENT one is not "no user
///   space": user units then map straight onto the viewport, the same as `viewBox="0 0 <width>
///   <height>"` (every window-control icon of a GTK theme writes `width="16" height="16"` and no
///   viewBox);
/// * an INTRINSIC size: an `<svg>` is a replaced element, as big as `width` / `height` say and
///   failing that as big as its viewBox - pushed ahead of the `style` attribute, so a call site
///   that says how big it wants it still wins.
fn render_svg(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    use azul_css::props::{
        layout::{LayoutHeight, LayoutPosition, LayoutWidth},
        property::CssProperty,
    };
    let simple = CssPropertyWithConditions::simple;

    let mut node = NodeData::create_node(NodeType::Svg);
    let mut intrinsic = Vec::new();
    let view_box = element
        .attribute("viewBox")
        .or_else(|| element.attribute("viewbox"))
        .and_then(super::parse_svg_view_box);
    let stated = |key: &str| super::parse_svg_length(element.attribute(key));
    let usable = |v: f32| v.is_finite() && v > 0.0;
    let implied = match (stated("width"), stated("height")) {
        (Some(w), Some(h)) if usable(w) && usable(h) => Some((0.0, 0.0, w, h)),
        _ => None,
    };
    if let Some((min_x, min_y, width, height)) = view_box.or(implied) {
        node.set_svg_data(crate::dom::SvgNodeData::ViewBox {
            min_x,
            min_y,
            width,
            height,
        });
    }
    if let Some(w) = stated("width")
        .or_else(|| view_box.map(|(_, _, w, _)| w))
        .filter(|w| usable(*w))
    {
        intrinsic.push(simple(CssProperty::width(LayoutWidth::px(w))));
    }
    if let Some(h) = stated("height")
        .or_else(|| view_box.map(|(_, _, _, h)| h))
        .filter(|h| usable(*h))
    {
        intrinsic.push(simple(CssProperty::height(LayoutHeight::px(h))));
    }
    intrinsic.push(simple(CssProperty::const_position(LayoutPosition::Relative)));
    land_common(&mut node, element, intrinsic, landing);
    keep_svg_attributes(&mut node, element, landing);
    node
}

/// An SVG shape (`path`, `circle`, `rect`, `ellipse`, `line`, `polygon`,
/// `polyline`): painted by filling its own box and clipping that box to its
/// geometry (`SvgNodeData::Path`, pushed as a clip mask by the display list).
///
/// The box is the `<svg>`'s viewport (the clip mask is rasterised into the
/// node's paint rect, so a shape laid out in flow would be clipped against
/// the wrong rectangle); `fill` / `stroke` / `stroke-width` are the box's
/// background / border, which the display list paints as the shape's fill
/// and stroke (`style="fill:..."` and stylesheets need nothing: `fill` is an
/// accepted spelling of `background-color`). `fill="none"` lands NOTHING
/// rather than a transparent background: it must not shadow a stylesheet rule
/// that sets a fill.
fn render_svg_shape(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    use azul_css::props::{
        basic::color::{parse_color_or_system, parse_color_or_system_token, ColorOrSystem},
        property::CssProperty,
        style::{
            LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor,
            StyleBorderTopColor,
        },
    };
    let simple = CssPropertyWithConditions::simple;

    let mut node = NodeData::create_node(super::tag_to_node_type(element.tag));
    let mut intrinsic = viewport_box();
    // Both take a `system:` colour keyword like their CSS spellings: the fill
    // as an unresolved `SystemColor` layer, the stroke as the border colour's
    // token - the getters resolve them against the cascade's theme.
    if let Some(fill) = element.attribute("fill").map(str::trim).filter(|f| *f != "none") {
        if let Ok(color) = parse_color_or_system(fill) {
            let layer = match color {
                ColorOrSystem::Color(c) => StyleBackgroundContent::Color(c),
                ColorOrSystem::System(r) => StyleBackgroundContent::SystemColor(r),
            };
            intrinsic.push(simple(CssProperty::const_background_content(
                StyleBackgroundContentVec::from_vec(alloc::vec![layer]),
            )));
        }
    }
    if let Some(stroke) = element.attribute("stroke").map(str::trim).filter(|s| *s != "none") {
        if let Ok(color) = parse_color_or_system_token(stroke) {
            intrinsic.extend([
                simple(CssProperty::const_border_top_color(StyleBorderTopColor { inner: color })),
                simple(CssProperty::const_border_right_color(StyleBorderRightColor {
                    inner: color,
                })),
                simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
                    inner: color,
                })),
                simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
                    inner: color,
                })),
            ]);
        }
    }
    // `stroke-width` is in USER UNITS, like every geometry attribute.
    if let Some(width) = super::parse_svg_float(element.attribute("stroke-width")) {
        if width.is_finite() && width > 0.0 {
            let px = azul_css::props::basic::PixelValue::px(width);
            intrinsic.extend([
                simple(CssProperty::const_border_top_width(LayoutBorderTopWidth { inner: px })),
                simple(CssProperty::const_border_right_width(LayoutBorderRightWidth {
                    inner: px,
                })),
                simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth {
                    inner: px,
                })),
                simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth {
                    inner: px,
                })),
            ]);
        }
    }
    land_common(&mut node, element, intrinsic, landing);
    keep_svg_attributes(&mut node, element, landing);
    if let Some(geometry) = super::svg_shape_geometry(element) {
        node.set_svg_data(crate::dom::SvgNodeData::Path(geometry));
    }
    node
}

/// The box of an SVG element drawn in its `<svg>`'s user space: the `<svg>`'s
/// viewport (`position: absolute` at `inset: 0`). What it draws is placed by
/// its attributes, through the `<svg>`'s viewBox mapping and the `transform`s
/// above it, at layout and paint time.
fn viewport_box() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::{
        layout::{LayoutInsetBottom, LayoutLeft, LayoutPosition, LayoutRight, LayoutTop},
        property::CssProperty,
    };
    let simple = CssPropertyWithConditions::simple;
    alloc::vec![
        simple(CssProperty::const_position(LayoutPosition::Absolute)),
        simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        simple(CssProperty::const_top(LayoutTop::const_px(0))),
        simple(CssProperty::const_right(LayoutRight::const_px(0))),
        simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
    ]
}

/// `<g>`: a group, its `transform` (kept on the node) applying to everything
/// in it. Its box is the viewport, like its shapes'.
fn render_svg_group(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let mut node = NodeData::create_node(NodeType::SvgG);
    land_common(&mut node, element, viewport_box(), landing);
    keep_svg_attributes(&mut node, element, landing);
    node
}

/// `<text>`: an element whose characters are its `Text` children (and its
/// `<tspan>`s'), as every element's are; `x` / `y` (its baseline),
/// `font-*`, `fill`, `transform` stay on the node, where layout and paint
/// read them.
fn render_svg_text(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let mut node = NodeData::create_node(NodeType::SvgText);
    // A box of its own that never wraps, placed by layout from `x` / `y`.
    let font = embedded_font_family(element);
    let css = alloc::format!(
        "position: absolute; left: 0px; top: 0px; margin: 0px; white-space: pre; {}",
        svg_text_hints(element, font.is_none())
    );
    let mut intrinsic = declarations(&css, landing);
    intrinsic.extend(font);
    land_common(&mut node, element, intrinsic, landing);
    keep_svg_attributes(&mut node, element, landing);
    node
}

/// `<tspan>`: a run of an SVG text (`dx` / `dy`, its own `font-*` and `fill`
/// kept on the node).
fn render_svg_tspan(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let mut node = NodeData::create_node(NodeType::SvgTspan);
    let font = embedded_font_family(element);
    let mut intrinsic = declarations(&svg_text_hints(element, font.is_none()), landing);
    intrinsic.extend(font);
    land_common(&mut node, element, intrinsic, landing);
    keep_svg_attributes(&mut node, element, landing);
    node
}

/// `<image href x y width height>`: an `SvgImage` whose placeholder carries the
/// `href` (a URL or a `data:` URI, resolved like an `<img src>`), its
/// `width` / `height` its size in user units.
fn render_svg_image(element: &Element<'_>, landing: &mut Landing<'_>) -> NodeData {
    let href = element
        .attribute("href")
        .or_else(|| element.attribute("xlink:href"))
        .unwrap_or_default();
    // `v` is finite and positive here; `as` saturates a size past usize::MAX.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = |key: &str| {
        super::parse_svg_float(element.attribute(key))
            .filter(|v| v.is_finite() && *v > 0.0)
            .map_or(0, |v| v.round() as usize)
    };
    let image = crate::resources::ImageRef::null_image(
        size("width"),
        size("height"),
        crate::resources::RawImageFormat::RGBA8,
        href.as_bytes().to_vec(),
    );
    let mut node = NodeData::create_node(NodeType::SvgImage(image));
    land_common(&mut node, element, viewport_box(), landing);
    keep_svg_attributes(&mut node, element, landing);
    node
}

// ---- what every element carries ----

/// Lands what every node carries, after the renderer's own `intrinsic` CSS
/// (an `<svg>`'s size, a shape's paint), which the `style` attribute
/// overrides:
///
/// * a `<transient-window>`'s config (it rides INSIDE the node type) and its `tearoff="zone:<sel>"`
///   selector, kept as the `tearoff-zone` attribute the engine's drop handling reads;
/// * the ONE attribute table ([`super::attributes`]: ids and classes, focus, editing, the typed
///   attributes, `dir`, the inline `style`);
/// * HTML's presentational hints (`<font color>`, `<ol type>`, `<center>` ...);
/// * the element's COMPONENT arguments (`<a href target rel>`, `<img src alt>`), filled from the
///   attributes by the filler every component uses;
/// * the `data-l10n-*` arguments of a localised element.
fn land_common(
    node: &mut NodeData,
    element: &Element<'_>,
    mut intrinsic: Vec<CssPropertyWithConditions>,
    landing: &mut Landing<'_>,
) {
    if let NodeType::TransientWindow(cfg) = node.get_node_type() {
        let mut cfg = *cfg;
        let mut zone = None;
        for (key, value) in element.pairs() {
            if cfg.apply_attr(key, value) && key == "tearoff" {
                zone = value.trim().strip_prefix("zone:").map(str::trim);
            }
        }
        node.set_node_type(NodeType::TransientWindow(cfg));
        if let Some(selector) = zone {
            let mut all = node.attributes().clone().into_library_owned_vec();
            all.push(crate::dom::AttributeType::Custom(
                crate::dom::AttributeNameValue {
                    attr_name: (landing.intern)("tearoff-zone"),
                    value: (landing.intern)(selector),
                },
            ));
            node.set_attributes(all.into());
        }
    }

    let settings = super::attributes::ordered(
        element
            .pairs()
            .filter_map(|(key, value)| super::attributes::setting_of(element.tag, key, value)),
    );
    let hints = super::builtin_presentational_hints(element.tag, element.pairs());
    if !hints.is_empty() {
        if let Some(map) = landing.css_key_map { intrinsic.extend(super::attributes::style_declarations(&hints, map)) } else {
            let map = azul_css::props::property::get_css_key_map();
            intrinsic.extend(super::attributes::style_declarations(&hints, &map));
        }
    }
    super::attributes::apply_settings(
        node,
        settings,
        intrinsic,
        landing.css_key_map,
        &mut *landing.intern,
    );
    super::apply_builtin_args_from_attributes(element.tag, element.pairs(), node);

    if l10n_key(element.attributes).is_some() {
        let fluent_args = crate::dom::FluentArgKVVec::from_l10n_attributes(element.pairs());
        if !fluent_args.is_empty() {
            node.fluent_args = Some(Box::new(fluent_args));
        }
    }
}

/// Keeps an SVG element's own attributes on its node - every one the
/// attribute table did not take (`d`, `fill`, `stroke`, `transform`, `x`,
/// `font-size`, `href`, `viewBox` ...), as written: layout and paint read them
/// there (`NodeData::get_attribute`), and the node says what the markup said
/// (the SVG can be written back from the DOM).
fn keep_svg_attributes(node: &mut NodeData, element: &Element<'_>, landing: &mut Landing<'_>) {
    let kept: Vec<_> = element
        .pairs()
        .filter(|(key, value)| super::attributes::setting_of(element.tag, key, value).is_none())
        .map(|(key, value)| {
            crate::dom::AttributeType::Custom(crate::dom::AttributeNameValue {
                attr_name: (landing.intern)(key),
                value: (landing.intern)(value),
            })
        })
        .collect();
    if kept.is_empty() {
        return;
    }
    let mut all = node.attributes().clone().into_library_owned_vec();
    all.extend(kept);
    node.set_attributes(all.into());
}

/// The CSS an SVG text element's PRESENTATION attributes stand for (SVG 2
/// 6.6: `font-family`, `font-size`, `font-weight`, `font-style`,
/// `letter-spacing`, `word-spacing`, and `fill` - the colour its glyphs are
/// painted in), lowest in the cascade like HTML's presentational hints. Sizes
/// are user units: layout scales them through the `<svg>`'s mapping.
fn svg_text_hints(element: &Element<'_>, with_family: bool) -> String {
    let mut css = String::new();
    let family = element.attribute("font-family").map(str::trim);
    if let Some(family) = family.filter(|f| with_family && !f.is_empty()) {
        if family.contains(',') || family.starts_with('"') || family.starts_with('\'') {
            let _ = write!(css, "font-family: {family};");
        } else {
            let _ = write!(css, "font-family: \"{family}\";");
        }
    }
    for (attribute, property) in [
        ("font-size", "font-size"),
        ("letter-spacing", "letter-spacing"),
        ("word-spacing", "word-spacing"),
    ] {
        if let Some(size) = super::parse_svg_float(element.attribute(attribute)) {
            if size.is_finite() {
                let _ = write!(css, "{property}: {size}px;");
            }
        }
    }
    for property in ["font-weight", "font-style"] {
        if let Some(value) = element.attribute(property).map(str::trim).filter(|v| !v.is_empty()) {
            let _ = write!(css, "{property}: {value};");
        }
    }
    match element.attribute("fill").map(str::trim) {
        Some("none") => css.push_str("color: transparent;"),
        Some(fill) if !fill.is_empty() => {
            let _ = write!(css, "color: {fill};");
        }
        _ => {}
    }
    css
}

/// The declarations of a CSS block, with the builder's key map.
fn declarations(css: &str, landing: &Landing<'_>) -> Vec<CssPropertyWithConditions> {
    landing.css_key_map.map_or_else(
        || {
            super::attributes::style_declarations(
                css,
                &azul_css::props::property::get_css_key_map(),
            )
        },
        |map| super::attributes::style_declarations(css, map),
    )
}

/// A text element's `font-family` as the FONT an `@font-face` in its scope
/// declares under that name (`StyleFontFamily::Ref`, in place of the name):
/// the page's own font, not one the system has under that name.
fn embedded_font_family(element: &Element<'_>) -> Option<CssPropertyWithConditions> {
    use azul_css::props::{
        basic::font::{StyleFontFamily, StyleFontFamilyVec},
        property::CssProperty,
    };
    let family = element.attribute("font-family")?.trim();
    let family = family.trim_matches(|c| c == '"' || c == '\'');
    let face = element
        .font_faces
        .iter()
        .rev()
        .find(|face| face.family.eq_ignore_ascii_case(family))?;
    Some(CssPropertyWithConditions::simple(CssProperty::font_family(
        StyleFontFamilyVec::from_vec(alloc::vec![StyleFontFamily::Ref(face.font.clone())]),
    )))
}
