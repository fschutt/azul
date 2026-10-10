//! The end user's rice (`azul_css::rice`) reaches the window's cascade as a USER-origin sheet
//! (`StyledDom::create_from_dom_with_user_sheets`): unscoped, after the DOM's own sheets, at the
//! priority its header asked for.
//!
//! - `base` (the default) fills what nobody declared: the app's own stylesheet beats it.
//! - `widgets` beats the app's stylesheet and a `with_css` declaration (INLINE, 30).
//! - A rice for a theme that is not live is inert.
//!
//! Each test builds its own rice root in the temp directory; nothing reads the real home.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::StyledDom,
};
use azul_css::{
    dynamic_selector::DynamicSelectorContext,
    parser2::new_from_str,
    props::basic::{ColorU, PhysicalSize},
    rice::{load_rice, RiceEnv},
};
use azul_layout::solver3::getters;

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };

/// A rice root holding one file, `css/<theme dir>/rice.css`, removed on drop.
struct Rice(PathBuf);

impl Rice {
    fn new(theme_dir: &str, contents: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "azul-rice-window-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("css").join(theme_dir);
        fs::create_dir_all(&dir).expect("rice dir");
        fs::write(dir.join("rice.css"), contents).expect("rice file");
        Self(root)
    }
}

impl Drop for Rice {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The colour of `div.x` under the app theme `live_theme`, with the app's own stylesheet
/// `.x { color: red }`, an optional `with_css` declaration on the div, and the rice.
fn ink(rice: &Rice, live_theme: &str, inline: Option<&str>) -> ColorU {
    let env = RiceEnv::with_root(rice.0.clone(), "rice-window-test");
    let loaded = load_rice(&env, &[live_theme.to_string()]);

    let mut div = Dom::create_div().with_class("x".into());
    if let Some(css) = inline {
        div = div.with_css(css);
    }
    let app_sheet = new_from_str(".x { color: #ff0000; }").0;
    let dom = Dom::create_body().with_component_css(app_sheet).with_child(div);

    let style = azul_css::system::defaults::macos_modern_light();
    let ctx = DynamicSelectorContext::from_system_style(&style)
        .with_viewport(800.0, 600.0)
        .with_app_theme(live_theme);
    let sd = StyledDom::create_from_dom_with_user_sheets(
        dom,
        Some(ctx),
        core::slice::from_ref(&loaded.css),
    );
    getters::get_style_properties(&sd, NodeId::new(1), None, PhysicalSize::new(800.0, 600.0)).color
}

#[test]
fn a_base_rice_fills_in_but_the_apps_stylesheet_beats_it() {
    let rice = Rice::new("r4base", ".x { color: #0000ff; }");
    assert_eq!(ink(&rice, "r4base", None), RED);
}

#[test]
fn a_widgets_rice_beats_the_apps_stylesheet() {
    let rice = Rice::new("r4widgets", "// priority: widgets\n.x { color: #0000ff; }");
    assert_eq!(ink(&rice, "r4widgets", None), BLUE);
}

#[test]
fn a_widgets_rice_beats_a_with_css_declaration_and_an_app_rice_does_not() {
    let widgets = Rice::new("r4inline", "// priority: widgets\n.x { color: #0000ff; }");
    assert_eq!(ink(&widgets, "r4inline", Some("color: #00ff00;")), BLUE);

    let app = Rice::new("r4app", "// priority: app\n.x { color: #0000ff; }");
    assert_eq!(ink(&app, "r4app", Some("color: #00ff00;")), GREEN, "app (25) < inline (30)");
    assert_eq!(ink(&app, "r4app", None), BLUE, "app (25) > the app's author sheet (20)");
}

#[test]
fn a_rice_for_a_theme_that_is_not_live_is_inert() {
    let rice = Rice::new("r4elsewhere", "// priority: force\n.x { color: #0000ff; }");
    // Loaded for `r4elsewhere`, but the window runs another app theme.
    let env = RiceEnv::with_root(rice.0.clone(), "rice-window-test");
    let loaded = load_rice(&env, &["r4elsewhere".to_string()]);
    let dom = Dom::create_body()
        .with_component_css(new_from_str(".x { color: #ff0000; }").0)
        .with_child(Dom::create_div().with_class("x".into()));
    let style = azul_css::system::defaults::macos_modern_light();
    let ctx = DynamicSelectorContext::from_system_style(&style)
        .with_viewport(800.0, 600.0)
        .with_app_theme("flat");
    let sd = StyledDom::create_from_dom_with_user_sheets(
        dom,
        Some(ctx),
        core::slice::from_ref(&loaded.css),
    );
    let got =
        getters::get_style_properties(&sd, NodeId::new(1), None, PhysicalSize::new(800.0, 600.0))
            .color;
    assert_eq!(got, RED);
}

/// A `*` rule of a rice reaches every node, not only the root: the rice is appended AFTER the
/// DOM's sheets are scoped (a `* {}` at or above INLINE hung on the root would be node-only).
#[test]
fn a_universal_rice_rule_reaches_every_node() {
    // `background-color` does not inherit: only a rule that MATCHES node 2 paints it.
    let rice = Rice::new("r4star", "// priority: widgets\n* { background-color: #0000ff; }");
    let env = RiceEnv::with_root(rice.0.clone(), "rice-window-test");
    let loaded = load_rice(&env, &["r4star".to_string()]);
    let dom = Dom::create_body().with_child(Dom::create_div().with_child(Dom::create_div()));
    let style = azul_css::system::defaults::macos_modern_light();
    let ctx = DynamicSelectorContext::from_system_style(&style)
        .with_viewport(800.0, 600.0)
        .with_app_theme("r4star");
    let sd = StyledDom::create_from_dom_with_user_sheets(
        dom,
        Some(ctx),
        core::slice::from_ref(&loaded.css),
    );
    let inner = NodeId::new(2);
    let state = sd.styled_nodes.as_container()[inner].styled_node_state;
    assert_eq!(getters::get_background_color(&sd, inner, &state), BLUE);
}

/// The design's `widgets` slot "beats widget inline": a widget's static inline properties
/// (`with_css_props`, `NodeData::style`) are looked up BEFORE every stylesheet rule in
/// `CssPropertyCache` (its "PRIORITY 1"), whatever the rule's priority. Needs the
/// inline-vs-component unification that puts inline declarations at `rule_priority::INLINE`.
#[test]
#[ignore = "needs inline properties ordered by rule_priority::INLINE (inline-vs-component unification)"]
fn a_widgets_rice_beats_a_widgets_static_inline_property() {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{property::CssProperty, style::StyleTextColor},
    };
    let rice = Rice::new("r4static", "// priority: widgets\n.x { color: #0000ff; }");
    let env = RiceEnv::with_root(rice.0.clone(), "rice-window-test");
    let loaded = load_rice(&env, &["r4static".to_string()]);
    let div = Dom::create_div().with_class("x".into()).with_css_props(
        vec![CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: GREEN,
        }))]
        .into(),
    );
    let dom = Dom::create_body().with_child(div);
    let style = azul_css::system::defaults::macos_modern_light();
    let ctx = DynamicSelectorContext::from_system_style(&style)
        .with_viewport(800.0, 600.0)
        .with_app_theme("r4static");
    let sd = StyledDom::create_from_dom_with_user_sheets(
        dom,
        Some(ctx),
        core::slice::from_ref(&loaded.css),
    );
    let got =
        getters::get_style_properties(&sd, NodeId::new(1), None, PhysicalSize::new(800.0, 600.0))
            .color;
    assert_eq!(got, BLUE);
}
