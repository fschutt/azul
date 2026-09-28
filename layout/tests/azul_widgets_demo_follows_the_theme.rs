//! The AzWidgets demo follows the desktop's theme.
//!
//! On a dark desktop the demo painted its page `#f2f4f7`, its section cards
//! white and its headings `#1d2939` - light colours with no dark counterpart -
//! around widgets that DO follow the theme, so a text field came out as a dark
//! box on a white card. The demo is the first thing anyone runs, and the page
//! an application copies its styles from.
//!
//! The demo is an example crate this one cannot link, so this reads its
//! SOURCE: every string literal that is an inline style is parsed the way
//! `Dom::with_css` parses it, and every colour it paints must either be a
//! `system:` keyword (which resolves in the theme it is rendered in) or come
//! with a counterpart under `@media (prefers-color-scheme: dark)` in the same
//! style.

use azul_css::{
    css::{Css, CssDeclaration},
    dynamic_selector::{DynamicSelector, ThemeCondition},
    props::{
        basic::color::{ColorU, SystemColorRef},
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
};

/// `examples/azul-widgets/src/lib.rs`, verbatim, at compile time.
const DEMO: &str = include_str!("../../examples/azul-widgets/src/lib.rs");

/// The Video card, a module of its own (`examples/azul-widgets/src/video.rs`):
/// its styles are the demo's styles too.
const VIDEO_CARD: &str = include_str!("../../examples/azul-widgets/src/video.rs");

/// Every inline style the demo writes, over all of its source files. The
/// page frame (`page_frame`) is still read from `lib.rs` alone.
fn demo_styles() -> Vec<String> {
    [DEMO, VIDEO_CARD]
        .iter()
        .flat_map(|src| string_literals(src))
        .filter(|l| l.contains(':') && l.contains(';'))
        .collect()
}

/// Every `"..."` literal in `src`, escapes decoded (`\` line continuations
/// included). Line comments are skipped; the demo writes no raw strings.
fn string_literals(src: &str) -> Vec<String> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // The one char literal that could open a phantom string.
        if b[i] == '\'' && b.get(i + 1) == Some(&'"') && b.get(i + 2) == Some(&'\'') {
            i += 3;
            continue;
        }
        if b[i] != '"' {
            i += 1;
            continue;
        }
        i += 1;
        let mut s = String::new();
        while i < b.len() && b[i] != '"' {
            if b[i] == '\\' {
                match b.get(i + 1) {
                    Some('\n') => {
                        i += 2;
                        while i < b.len() && b[i].is_whitespace() {
                            i += 1;
                        }
                        continue;
                    }
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(c) => s.push(*c),
                    None => {}
                }
                i += 2;
                continue;
            }
            s.push(b[i]);
            i += 1;
        }
        i += 1;
        out.push(s);
    }
    out
}

fn paints_a_colour(ty: CssPropertyType) -> bool {
    matches!(
        ty,
        CssPropertyType::TextColor
            | CssPropertyType::BackgroundContent
            | CssPropertyType::BorderTopColor
            | CssPropertyType::BorderRightColor
            | CssPropertyType::BorderBottomColor
            | CssPropertyType::BorderLeftColor
    )
}

/// A `system:` keyword resolves in the theme it is painted in, so it needs
/// no twin.
fn follows_the_theme(p: &CssProperty) -> bool {
    let token = |c: azul_css::props::basic::color::ColorU| {
        SystemColorRef::from_color_token(c).is_some()
    };
    match p {
        CssProperty::BackgroundContent(v) => v.get_property().is_some_and(|layers| {
            layers
                .as_ref()
                .iter()
                .all(|l| matches!(l, StyleBackgroundContent::SystemColor(_)))
        }),
        CssProperty::TextColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderTopColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderRightColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderBottomColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderLeftColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        _ => false,
    }
}

/// The colour properties `style` paints in the light theme with a fixed
/// colour and never re-states for the dark one.
fn colours_without_a_dark_twin(style: &str) -> Vec<CssPropertyType> {
    let css = Css::parse_inline(style);
    let mut light = Vec::new();
    let mut dark = Vec::new();
    for rule in css.rules.as_ref() {
        let conditions = rule.conditions.as_ref();
        let is_dark = conditions.contains(&DynamicSelector::Theme(ThemeCondition::Dark));
        for d in rule.declarations.as_ref() {
            let CssDeclaration::Static(p) = d else {
                continue;
            };
            let ty = p.get_type();
            if !paints_a_colour(ty) {
                continue;
            }
            if is_dark {
                dark.push(ty);
            } else if conditions.is_empty() && !follows_the_theme(p) {
                light.push(ty);
            }
        }
    }
    light.retain(|t| !dark.contains(t));
    light
}

#[test]
fn every_colour_the_demo_paints_follows_the_theme() {
    let styles = demo_styles();
    assert!(
        styles.iter().any(|s| s.contains("flex-direction")),
        "premise: the scan found the demo's inline styles"
    );
    assert!(
        styles.iter().any(|s| s.contains("system:accent-text")),
        "premise: the scan found the video card's styles"
    );

    let bad: Vec<String> = styles
        .iter()
        .filter_map(|s| {
            let missing = colours_without_a_dark_twin(s);
            (!missing.is_empty()).then(|| {
                format!(
                    "{missing:?} in {:?}",
                    s.chars().take(72).collect::<String>()
                )
            })
        })
        .collect();
    assert!(
        bad.is_empty(),
        "{} demo style(s) paint a light-theme colour with no dark counterpart:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

/// A guard on the guard: the check must see a bare colour, and must accept
/// both ways a style can follow the theme.
#[test]
fn the_check_tells_a_themed_style_from_a_fixed_one() {
    assert_eq!(
        colours_without_a_dark_twin("color: #123456;"),
        vec![CssPropertyType::TextColor]
    );
    assert!(colours_without_a_dark_twin(
        "color: #123456; @media (prefers-color-scheme: dark) { color: system:text; }"
    )
    .is_empty());
    assert!(colours_without_a_dark_twin("color: system:text; width: 4px;").is_empty());
    assert_eq!(
        colours_without_a_dark_twin(
            "background-color: #fff; color: #000; @media (prefers-color-scheme: dark) { color: \
             system:text; }"
        ),
        vec![CssPropertyType::BackgroundContent],
        "a twin for one property does not excuse another"
    );
}

// ---------------------------------------------------------------------------
// What the user READS: the page title and the titlebar title, in both themes.
// ---------------------------------------------------------------------------

/// The demo's own page frame, rebuilt from its source: `body > [titlebar >
/// [title, label], scroll > heading]`, each node carrying the inline style
/// the demo gives it. Located by the literals around it: the first
/// `"Azul Widget Showcase"` is the heading's text (its style follows), the
/// second the titlebar title's (the titlebar's style precedes it), and the
/// label, body and scroll styles follow the `"custom titlebar"` label's text.
///
/// Shared with `the_macos_titlebar_lines_up_with_its_traffic_lights`, which
/// lays the demo's titlebar out.
pub(crate) struct PageFrame {
    pub(crate) body: String,
    pub(crate) titlebar: String,
    pub(crate) title: String,
    pub(crate) label: String,
    pub(crate) scroll: String,
    pub(crate) heading: String,
}

pub(crate) fn page_frame() -> PageFrame {
    let lits = string_literals(DEMO);
    let at = |text: &str| -> Vec<usize> {
        lits.iter()
            .enumerate()
            .filter(|(_, l)| l.as_str() == text)
            .map(|(i, _)| i)
            .collect()
    };
    let titles = at("Azul Widget Showcase");
    assert!(titles.len() >= 2, "premise: the heading and the titlebar title, got {titles:?}");
    let label = *at("custom titlebar").first().expect("premise: the titlebar's label");
    let frame = PageFrame {
        heading: lits[titles[0] + 1].clone(),
        titlebar: lits[titles[1] - 1].clone(),
        title: lits[titles[1] + 1].clone(),
        label: lits[label + 1].clone(),
        body: lits[label + 2].clone(),
        scroll: lits[label + 3].clone(),
    };
    for (what, style, marker) in [
        ("heading", &frame.heading, "font-size"),
        ("titlebar", &frame.titlebar, "app-region"),
        ("title", &frame.title, "font-size"),
        ("label", &frame.label, "no-drag"),
        ("body", &frame.body, "margin"),
        ("scroll", &frame.scroll, "overflow"),
    ] {
        assert!(style.contains(marker), "premise: found the {what}'s style, got {style:?}");
    }
    frame
}

type Rgb = [f32; 3];

fn rgb(c: ColorU) -> Rgb {
    [f32::from(c.r), f32::from(c.g), f32::from(c.b)]
}

/// `top` (straight alpha) over an opaque `base`.
fn over(top: ColorU, base: Rgb) -> Rgb {
    let a = f32::from(top.a) / 255.0;
    let t = rgb(top);
    [
        t[0] * a + base[0] * (1.0 - a),
        t[1] * a + base[1] * (1.0 - a),
        t[2] * a + base[2] * (1.0 - a),
    ]
}

/// WCAG 2 contrast ratio of two opaque sRGB colours.
fn contrast(a: Rgb, b: Rgb) -> f32 {
    let lum = |c: Rgb| {
        let lin = |v: f32| {
            let v = v / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    };
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `(ink, background)` of the text node `text` as the display list paints
/// it: the ink is the cascade's `color` for the text node itself (the live
/// re-resolve the display list makes, `system:` keywords resolved), the
/// background every solid layer on the path from the root, over the
/// window's background.
fn seen(
    sd: &azul_core::styled_dom::StyledDom,
    text: azul_core::dom::NodeId,
    ctx: &azul_css::dynamic_selector::DynamicSelectorContext,
) -> (Rgb, Rgb) {
    use azul_layout::solver3::getters;

    let hierarchy = sd.node_hierarchy.as_container();
    let states = sd.styled_nodes.as_container();
    let mut path = vec![text];
    let mut cur = hierarchy[text].parent_id();
    while let Some(n) = cur {
        path.push(n);
        cur = hierarchy[n].parent_id();
    }
    path.reverse();

    let mut bg = rgb(ctx.system_color(SystemColorRef::WindowBackground));
    for &n in &path {
        for layer in getters::get_background_contents(sd, n, &states[n].styled_node_state) {
            if let StyleBackgroundContent::Color(c) = layer {
                bg = over(c, bg);
            }
        }
    }
    let nd = &sd.node_data.as_container()[text];
    let ink = sd
        .get_css_property_cache()
        .get_text_color(nd, &text, &states[text].styled_node_state)
        .and_then(|v| v.get_property().copied())
        .map(|c| c.inner)
        .expect("a cascaded DOM gives every text node a `color`");
    let ink = getters::system_colors_resolved(sd, ink);
    (over(ink, bg), bg)
}

/// The page heading and the titlebar title read at WCAG AA (4.5:1) in the
/// macOS light AND dark presets. On a dark desktop both came out `#101828`
/// on the dark page - their `@media (prefers-color-scheme: dark)` twin won
/// on the div but the text inside inherited the light value.
#[test]
fn the_page_and_titlebar_titles_are_legible_in_both_themes() {
    use azul_core::{
        dom::{Dom, NodeId},
        styled_dom::StyledDom,
    };
    use azul_css::{
        dynamic_selector::DynamicSelectorContext,
        system::{defaults, Theme},
    };

    let f = page_frame();
    let text = || Dom::create_text_do_not_use_without_block_level_wrapper("Azul Widget Showcase");
    // body(0) > titlebar(1) > title(2) > text(3); body > scroll(4) >
    // heading(5) > text(6)
    let dom = Dom::create_body()
        .with_css(&f.body)
        .with_child(
            Dom::create_div()
                .with_css(&f.titlebar)
                .with_child(Dom::create_div().with_css(&f.title).with_child(text())),
        )
        .with_child(
            Dom::create_div()
                .with_css(&f.scroll)
                .with_child(Dom::create_div().with_css(&f.heading).with_child(text())),
        );

    let mut bad = Vec::new();
    for theme in [Theme::Light, Theme::Dark] {
        let style = std::sync::Arc::new(match theme {
            Theme::Light => defaults::macos_modern_light(),
            Theme::Dark => defaults::macos_modern_dark(),
        });
        let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(1024.0, 768.0);
        let sd = StyledDom::create_from_dom_with_context(dom.clone(), Some(ctx.clone()));
        for (what, node) in [("titlebar title", 3), ("page heading", 6)] {
            let (ink, bg) = seen(&sd, NodeId::new(node), &ctx);
            let ratio = contrast(ink, bg);
            if ratio < 4.5 {
                bad.push(format!(
                    "{theme:?} {what}: {ratio:.2}:1 - ink {ink:?} on {bg:?}"
                ));
            }
        }
    }
    assert!(bad.is_empty(), "illegible demo titles:\n  {}", bad.join("\n  "));
}

/// The demo paints from the desktop palette DIRECTLY: every colour it names
/// is a `system:` keyword in BOTH themes - no light value with a
/// `@media (prefers-color-scheme: dark)` patch over it - and its page text
/// is the platform's UI face (`font-family: system:ui`). A light palette
/// with dark twins follows the theme, but not the platform: on a desktop
/// with its own light palette (or accent) the page kept the demo's greys.
#[test]
fn the_demo_paints_from_the_system_palette_directly() {
    let styles = demo_styles();

    let mut fixed = Vec::new();
    let mut twins = Vec::new();
    for s in &styles {
        let head = || s.chars().take(60).collect::<String>();
        for rule in Css::parse_inline(s).rules.as_ref() {
            if rule
                .conditions
                .as_ref()
                .contains(&DynamicSelector::Theme(ThemeCondition::Dark))
            {
                twins.push(head());
            }
            for d in rule.declarations.as_ref() {
                let CssDeclaration::Static(p) = d else {
                    continue;
                };
                if paints_a_colour(p.get_type()) && !follows_the_theme(p) {
                    fixed.push(format!("{:?} in {:?}", p.get_type(), head()));
                }
            }
        }
    }
    assert!(
        fixed.is_empty(),
        "{} fixed colour(s) where the demo should name a `system:` colour:\n  {}",
        fixed.len(),
        fixed.join("\n  ")
    );
    assert!(
        twins.is_empty(),
        "{} dark-theme patch(es) - a `system:` colour needs none:\n  {}",
        twins.len(),
        twins.join("\n  ")
    );

    let body = page_frame().body;
    assert!(
        body.split(';').any(|d| d.trim() == "font-family: system:ui"),
        "the page text is the platform's UI face, got {body:?}"
    );
}

/// The Notifications section (which replaced the demo's Toast) lives in its
/// own module, `notifications.rs`; the scans above read `lib.rs` only. It
/// paints from the same palette, with no dark-theme patches.
const DEMO_NOTIFICATIONS: &str =
    include_str!("../../examples/azul-widgets/src/notifications.rs");

#[test]
fn the_notifications_section_paints_from_the_system_palette_too() {
    let styles: Vec<String> = string_literals(DEMO_NOTIFICATIONS)
        .into_iter()
        .filter(|l| l.contains(':') && l.contains(';'))
        .collect();
    assert!(
        styles.iter().any(|s| s.contains("color: system:")),
        "premise: the scan found the section's inline styles, got {styles:?}"
    );

    let mut bad = Vec::new();
    for s in &styles {
        for rule in Css::parse_inline(s).rules.as_ref() {
            if rule
                .conditions
                .as_ref()
                .contains(&DynamicSelector::Theme(ThemeCondition::Dark))
            {
                bad.push(format!("a dark-theme patch in {s:?}"));
            }
            for d in rule.declarations.as_ref() {
                let CssDeclaration::Static(p) = d else {
                    continue;
                };
                if paints_a_colour(p.get_type()) && !follows_the_theme(p) {
                    bad.push(format!("fixed {:?} in {s:?}", p.get_type()));
                }
            }
        }
    }
    assert!(bad.is_empty(), "the notifications section:\n  {}", bad.join("\n  "));
}
