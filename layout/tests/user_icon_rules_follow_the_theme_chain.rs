//! The user's icon rules on disk (design 8): `<root>/remap.json` (global)
//! and `<root>/<theme>/remap.json` (per theme; `xyz/pink/` is `xyz:pink`),
//! each rule naming a file next to its table, conditioned by `apply-if`.
//!
//! Every test builds its own tree under the system temp dir and hands the
//! loader that ROOT - never the real `$HOME`.

#![cfg(all(feature = "json", feature = "cpurender", feature = "std"))]

use std::path::{Path, PathBuf};

use azul_core::icon::{IconDesignedFor, IconModeColors, IconProviderHandle, IconRecolor};
use azul_css::{
    dynamic_selector::{DynamicSelectorContext, ThemeCondition},
    props::basic::color::{ColorU, SystemColorRef},
};
use azul_layout::{
    icon::{create_default_icon_provider, SvgIconData},
    icon_remap::{load_user_icon_rules, walk_theme_dirs, ThemeDir},
};

const LIGHT_SVG: &str = r#"<svg width="16" height="16"><rect width="16" height="16" fill="currentColor"/></svg>"#;
const DARK_SVG: &str = r#"<svg width="24" height="24"><rect width="24" height="24" fill="currentColor"/></svg>"#;
const BLACK_SVG: &str = r#"<svg width="8" height="8"><rect width="8" height="8"/></svg>"#;

/// A fresh, empty directory for one test.
fn temp_root(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "azul-icon-rules-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test root");
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the parent");
    }
    std::fs::write(path, contents).expect("write the file");
}

fn ctx(dark: bool, chain: &[&str]) -> DynamicSelectorContext {
    let mut ctx = DynamicSelectorContext::default();
    ctx.theme = if dark {
        ThemeCondition::Dark
    } else {
        ThemeCondition::Light
    };
    ctx.theme_chain = azul_css::StringVec::from_vec(
        chain
            .iter()
            .map(|t| azul_css::AzString::from((*t).to_string()))
            .collect(),
    );
    ctx
}

/// The SVG a spec is drawn as under `ctx`, if a rule (or a registration)
/// makes it one.
fn svg_for(provider: &IconProviderHandle, spec: &str, ctx: &DynamicSelectorContext) -> Option<SvgIconData> {
    let mut data = provider.inner.lookup_spec_in_context(spec, ctx)?;
    let svg = data.downcast_ref::<SvgIconData>().map(|s| (*s).clone());
    svg
}

#[test]
fn theme_directories_are_named_by_their_path_segments() {
    let root = temp_root("walk");
    for dir in ["xyz/pink", "abc", ".hidden", "xyz/pink/deeper"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    write(&root.join("not-a-dir.svg"), LIGHT_SVG);
    let themes: Vec<String> = walk_theme_dirs(&root)
        .into_iter()
        .map(|ThemeDir { theme, .. }| theme)
        .collect();
    assert_eq!(
        themes,
        vec!["abc", "xyz", "xyz:pink", "xyz:pink:deeper"],
        "parents before their spin-offs, siblings by name, dot-directories skipped"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_global_rule_draws_the_name_as_its_file() {
    let root = temp_root("global");
    write(&root.join("home.svg"), LIGHT_SVG);
    write(
        &root.join("remap.json"),
        r#"{ "material/home": [ { "file": "home.svg" } ] }"#,
    );
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "azwriter");
    assert_eq!(report.rules, 1, "{report:?}");

    let svg = svg_for(&provider, "material/home", &ctx(false, &["flat"])).expect("remapped");
    assert_eq!(svg.svg, LIGHT_SVG.as_bytes());
    assert_eq!(
        svg.meta.recolor,
        IconRecolor::CurrentColor,
        "a currentColor file follows the text colour by default"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_theme_table_applies_while_its_theme_is_live_and_follows_the_mode() {
    let root = temp_root("theme");
    write(&root.join("monokai/home-dark.svg"), DARK_SVG);
    write(&root.join("monokai/home.svg"), LIGHT_SVG);
    write(
        &root.join("monokai/remap.json"),
        r#"{ "home": [
            { "file": "home-dark.svg", "apply-if": "mode=dark" },
            { "file": "home.svg" }
        ] }"#,
    );
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "azwriter");
    assert_eq!(report.rules, 2, "{report:?}");

    let dark = svg_for(&provider, "home", &ctx(true, &["monokai"])).expect("dark rule");
    assert_eq!(dark.svg, DARK_SVG.as_bytes());
    // The same provider, the mode flipped: evaluated at lookup, not at load.
    let light = svg_for(&provider, "home", &ctx(false, &["monokai"])).expect("light rule");
    assert_eq!(light.svg, LIGHT_SVG.as_bytes());
    assert!(
        svg_for(&provider, "home", &ctx(true, &["flat"])).is_none(),
        "monokai's rules are inert while monokai is not in the chain"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_spin_off_directorys_rule_beats_its_base_theme() {
    let root = temp_root("spinoff");
    write(&root.join("xyz/base.svg"), LIGHT_SVG);
    write(&root.join("xyz/pink/pink.svg"), DARK_SVG);
    write(&root.join("xyz/remap.json"), r#"{ "home": [ { "file": "base.svg" } ] }"#);
    write(&root.join("xyz/pink/remap.json"), r#"{ "home": [ { "file": "pink.svg" } ] }"#);
    let mut provider = create_default_icon_provider();
    load_user_icon_rules(&mut provider, &root, "");

    let pink = svg_for(&provider, "home", &ctx(false, &["xyz:pink", "xyz"])).expect("pink");
    assert_eq!(pink.svg, DARK_SVG.as_bytes());
    let base = svg_for(&provider, "home", &ctx(false, &["xyz"])).expect("base");
    assert_eq!(base.svg, LIGHT_SVG.as_bytes());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_path_that_leaves_its_directory_is_refused() {
    let root = temp_root("traversal");
    write(&root.join("secret.svg"), LIGHT_SVG);
    write(&root.join("theme/inside.svg"), LIGHT_SVG);
    write(
        &root.join("theme/remap.json"),
        r#"{
            "a": [ { "file": "../secret.svg" } ],
            "b": [ { "file": "/etc/passwd" } ],
            "c": [ { "file": "sub/../../secret.svg" } ],
            "d": [ { "file": "" } ],
            "e": [ { "file": "inside.svg" } ]
        }"#,
    );
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "");
    assert_eq!(report.rules, 1, "only the file inside its directory: {report:?}");
    assert!(
        report.warnings.len() >= 4,
        "every refused path is reported: {report:?}"
    );
    assert!(svg_for(&provider, "e", &ctx(false, &["theme"])).is_some());
    for name in ["a", "b", "c", "d"] {
        assert!(svg_for(&provider, name, &ctx(false, &["theme"])).is_none());
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_directory_is_refused() {
    let root = temp_root("symlink");
    write(&root.join("outside/secret.svg"), LIGHT_SVG);
    std::fs::create_dir_all(root.join("theme")).unwrap();
    std::os::unix::fs::symlink(root.join("outside/secret.svg"), root.join("theme/link.svg"))
        .unwrap();
    write(&root.join("theme/remap.json"), r#"{ "a": [ { "file": "link.svg" } ] }"#);
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "");
    assert_eq!(report.rules, 0, "{report:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Pitfall 10: an explicit JSON recolour colour beats the CSS colour -
/// `recolor: "#e6e6e6"` is a Fixed colour; `currentColor` takes the CSS one.
#[test]
fn the_recolor_forms_of_the_remap_format() {
    let root = temp_root("recolor");
    write(&root.join("glyph.svg"), BLACK_SVG);
    write(&root.join("art.svg"), BLACK_SVG);
    write(
        &root.join("remap.json"),
        r##"{
            "fixed":   [ { "file": "glyph.svg", "recolor": "#e6e6e6" } ],
            "by-mode": [ { "file": "glyph.svg", "recolor": { "light": "system:text", "dark": "#e6e6e6" } } ],
            "current": [ { "file": "glyph.svg", "recolor": "currentColor", "designed_for": "light" } ],
            "palette": [ { "file": "art.svg", "recolor": { "#000000": "system:text" } } ],
            "plain":   [ { "file": "art.svg" } ]
        }"##,
    );
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "");
    assert_eq!(report.rules, 5, "{report:?}");
    let c = ctx(false, &["flat"]);
    let e6 = ColorU {
        r: 0xe6,
        g: 0xe6,
        b: 0xe6,
        a: 255,
    };

    let fixed = svg_for(&provider, "fixed", &c).unwrap().meta;
    assert_eq!(fixed.recolor, IconRecolor::Fixed(IconModeColors::same(e6)));
    assert!(
        fixed.monochrome,
        "a rule that recolours a file states it is recolourable ink"
    );

    let by_mode = svg_for(&provider, "by-mode", &c).unwrap().meta;
    assert_eq!(
        by_mode.recolor,
        IconRecolor::Fixed(IconModeColors {
            light: SystemColorRef::Text.to_color_token(),
            dark: e6,
        })
    );

    let current = svg_for(&provider, "current", &c).unwrap().meta;
    assert_eq!(current.recolor, IconRecolor::CurrentColor);
    assert_eq!(current.designed_for, IconDesignedFor::Light);

    let palette = svg_for(&provider, "palette", &c).unwrap().meta;
    let IconRecolor::Palette(map) = &palette.recolor else {
        panic!("a colour-keyed object is a palette, got {palette:?}");
    };
    assert_eq!(map.as_ref().len(), 1);
    assert_eq!(map.as_ref()[0].to, SystemColorRef::Text.to_color_token());

    let plain = svg_for(&provider, "plain", &c).unwrap().meta;
    assert_eq!(plain.recolor, IconRecolor::None, "no recolor: never recoloured");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_icon_rule_redirects_to_a_registered_spec() {
    let root = temp_root("redirect");
    write(
        &root.join("remap.json"),
        r#"{ "kde:three-lines": [ { "icon": "user-menu", "apply-if": "app=azwriter" } ] }"#,
    );
    let mut provider = create_default_icon_provider();
    azul_layout::icon::register_svg_icon(
        &mut provider,
        "app",
        "user-menu",
        LIGHT_SVG.as_bytes(),
        azul_core::icon::IconMeta::for_image(),
    );
    load_user_icon_rules(&mut provider, &root, "azwriter");
    assert!(svg_for(&provider, "kde:three-lines", &ctx(false, &["flat"])).is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_broken_table_is_reported_and_the_others_still_load() {
    let root = temp_root("broken");
    write(&root.join("broken/remap.json"), "{ this is not json");
    write(&root.join("fine/ok.svg"), LIGHT_SVG);
    write(&root.join("fine/remap.json"), r#"{ "home": [ { "file": "ok.svg" } ] }"#);
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "");
    assert_eq!(report.rules, 1, "{report:?}");
    assert!(report.warnings.iter().any(|w| w.contains("broken")), "{report:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_theme_directory_named_like_a_mode_is_refused() {
    let root = temp_root("reserved");
    write(&root.join("dark/ok.svg"), LIGHT_SVG);
    write(&root.join("dark/remap.json"), r#"{ "home": [ { "file": "ok.svg" } ] }"#);
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(&mut provider, &root, "");
    assert_eq!(report.rules, 0, "`dark` is the mode, not a theme: {report:?}");
    assert!(!report.warnings.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_root_loads_nothing_quietly() {
    let mut provider = create_default_icon_provider();
    let report = load_user_icon_rules(
        &mut provider,
        Path::new("/definitely/not/an/azul/icons/root"),
        "",
    );
    assert_eq!(report.rules, 0);
    assert!(report.warnings.is_empty(), "no user rules is the normal case");
}
