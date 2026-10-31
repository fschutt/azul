//! The rice loader (`azul_css::rice`): the end user's stylesheets, found per THEME DIRECTORY
//! along the theme chain (`~/.azul/css/<theme>/*.css`, `:` as a path segment), each file wrapped
//! in `@theme(<theme>)` and stamped with the priority its header asks for.
//!
//! Design: scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md, sections 4.1, 4.4, 7,
//! 9 and 9.2, pitfalls 7, 8 and 12.
//!
//! Every test uses its own temporary directory as the rice root (`RiceEnv::with_root`), so no
//! test ever reads the real home directory.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_css::{
    css::{rule_priority, CssRuleBlock},
    dynamic_selector::{DynamicSelector, DynamicSelectorContext, OsCondition, ThemeCondition},
    rice::{
        azul_version_matches, chain_directories, fallback_of, load_rice, parse_rice_header,
        rice_chain, ricing_mode_from, sanitize_rice_source, theme_dir_segments,
        version_satisfies, RiceEnv, RiceFileState, RicePriority, MAX_RICE_FILE_BYTES,
    },
    system::{Platform, RicingMode, SystemStyle},
    AzString,
};

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

/// A rice root in the system temp directory, removed on drop.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "azul-rice-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create the temp rice root");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Write `contents` to `<root>/<rel>`, creating the directories.
    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().expect("a parent")).expect("create the rice dirs");
        fs::write(&p, contents).expect("write a rice file");
        p
    }

    fn env(&self, app: &str) -> RiceEnv {
        RiceEnv::with_root(self.0.clone(), app)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn chain(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

fn selector(rule: &CssRuleBlock) -> String {
    rule.path.to_string()
}

/// The rule for `sel` in the loaded rice. Panics when there is none.
fn rule<'a>(rules: &'a [CssRuleBlock], sel: &str) -> &'a CssRuleBlock {
    rules
        .iter()
        .find(|r| selector(r) == sel)
        .unwrap_or_else(|| panic!("no rule {sel} in {:?}", rules.iter().map(selector).collect::<Vec<_>>()))
}

fn theme(name: &str) -> DynamicSelector {
    DynamicSelector::Theme(ThemeCondition::Custom(AzString::from(name.to_string())))
}

// ---------------------------------------------------------------------------------------------
// discovery over the chain
// ---------------------------------------------------------------------------------------------

#[test]
fn a_colon_in_a_theme_name_is_a_path_segment_on_disk() {
    assert_eq!(theme_dir_segments("xyz:pink"), Some(vec!["xyz", "pink"]));
    assert_eq!(theme_dir_segments("flat"), Some(vec!["flat"]));
    let root = Path::new("/r");
    assert_eq!(
        chain_directories(root, "css", &chain(&["xyz:pink", "xyz"])),
        vec![
            ("xyz:pink".to_string(), root.join("css").join("xyz").join("pink")),
            ("xyz".to_string(), root.join("css").join("xyz")),
        ],
    );
}

#[test]
fn the_directory_helper_walks_the_icon_tree_the_same_way() {
    // I1's remap loader reads `~/.azul/icons/<theme>/remap.json` over the same chain.
    let root = Path::new("/r");
    assert_eq!(
        chain_directories(root, "icons", &chain(&["xyz:pink", "flat"])),
        vec![
            ("xyz:pink".to_string(), root.join("icons").join("xyz").join("pink")),
            ("flat".to_string(), root.join("icons").join("flat")),
        ],
    );
}

#[test]
fn theme_directories_load_in_chain_order_the_most_specific_last_in_source() {
    let root = TempRoot::new("chain");
    root.write("css/xyz/pink/a.css", ".pink { color: #ff00ff; }");
    root.write("css/xyz/a.css", ".xyz { color: #00ff00; }");
    root.write("css/flat/a.css", ".flat { color: #0000ff; }");
    root.write("css/unrelated/a.css", ".unrelated { color: #123456; }");

    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz:pink", "xyz", "flat"]));
    let order: Vec<String> = loaded.css.rules().map(selector).collect();
    assert_eq!(order, vec![".flat", ".xyz", ".pink"], "least specific first, head last");

    let chain_seen: Vec<String> = loaded
        .status
        .chain
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    assert_eq!(chain_seen, chain(&["xyz:pink", "xyz", "flat"]));
}

#[test]
fn every_rule_of_a_rice_file_is_wrapped_in_its_theme_and_keeps_its_own_conditions() {
    let root = TempRoot::new("wrap");
    root.write(
        "css/xyz/pink/a.css",
        ".plain { color: #ff00ff; } @os linux { .kde { color: #00ffff; } }",
    );
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz:pink", "xyz"]));
    let rules = loaded.css.rules.as_ref();

    let plain = rule(rules, ".plain");
    assert_eq!(plain.conditions.as_ref(), &[theme("xyz:pink")][..]);

    let kde = rule(rules, ".kde");
    assert_eq!(
        kde.conditions.as_ref(),
        &[theme("xyz:pink"), DynamicSelector::Os(OsCondition::Linux)][..],
        "the theme goes first, the file's own conditions stay (they conjoin)",
    );
}

#[test]
fn a_theme_files_fallback_header_answers_for_that_theme() {
    let root = TempRoot::new("fallback");
    root.write("css/abc/a.css", "// fallback: native, flora\n.a { color: red; }");
    root.write("css/abc/b.css", "// fallback: flora\n.b { color: red; }");
    root.write("css/foo.css", "// theme: abc-base; priority: widgets; fallback: native\n");
    let env = root.env("myapp");
    assert_eq!(fallback_of(&env, "abc"), chain(&["native", "flora"]), "in file order, deduplicated");
    assert_eq!(fallback_of(&env, "abc-base"), chain(&["native"]), "a global file names its theme");
    assert!(fallback_of(&env, "unknown").is_empty());
}

/// Needs R3 (`css::theme_chain::expand_chain`): the rice's chain is R3's expansion, fed by the
/// `fallback:` headers this loader parsed.
#[test]
fn the_rice_chain_is_the_theme_chain_fed_by_the_fallback_headers() {
    let root = TempRoot::new("ricechain");
    root.write("css/abc/a.css", "// fallback: native\n.a { color: red; }");
    let (names, _warnings) = rice_chain(&root.env("myapp"), "abc", "flat");
    assert_eq!(names.first().map(String::as_str), Some("abc"), "{names:?}");
    assert!(names.iter().any(|n| n == "native"), "{names:?}");
    assert_eq!(names.last().map(String::as_str), Some("flat"), "the default is the floor: {names:?}");
}

// ---------------------------------------------------------------------------------------------
// the header
// ---------------------------------------------------------------------------------------------

#[test]
fn the_header_reads_every_key_of_the_meta_comment() {
    let h = parse_rice_header(
        "// theme: abc-base@1.4.0; priority: widgets; fallback: native; app: azwriter; \
         azul: 0.2.*, 0.3.*; requires: abc-base@^1.2\n.a { color: red; }",
    );
    assert_eq!(h.theme.as_deref(), Some("abc-base"));
    assert_eq!(h.version.as_deref(), Some("1.4.0"));
    assert_eq!(h.priority, Some(RicePriority::Widgets));
    assert_eq!(h.fallback, chain(&["native"]));
    assert_eq!(h.apps, chain(&["azwriter"]));
    assert_eq!(h.azul, chain(&["0.2.*", "0.3.*"]));
    assert_eq!(h.requires.len(), 1);
    assert_eq!(h.requires[0].theme, "abc-base");
    assert_eq!(h.requires[0].range, "^1.2");
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn the_header_may_be_a_block_comment_spanning_lines() {
    let h = parse_rice_header(
        "/*\n * theme: xyz:pink\n * priority: force\n */\n.a { color: red; }",
    );
    assert_eq!(h.theme.as_deref(), Some("xyz:pink"));
    assert_eq!(h.version, None);
    assert_eq!(h.priority, Some(RicePriority::Force));
}

#[test]
fn only_the_first_comment_is_the_header() {
    let h = parse_rice_header(".a { color: red; }\n/* priority: force */");
    assert_eq!(h.priority, None, "a comment after the first rule is not a header");
}

#[test]
fn a_typo_in_a_header_key_is_a_warning_not_silence() {
    let h = parse_rice_header("// theme: abc; prority: widgets\n");
    assert_eq!(h.priority, None);
    assert!(
        h.warnings.iter().any(|w| w.contains("prority")),
        "{:?}",
        h.warnings
    );
}

#[test]
fn the_priority_names_map_onto_the_rule_priority_slots() {
    assert_eq!(RicePriority::from_name("base"), Some(RicePriority::Base));
    assert_eq!(RicePriority::from_name("PALETTE"), Some(RicePriority::Palette));
    assert_eq!(RicePriority::from_name("app"), Some(RicePriority::App));
    assert_eq!(RicePriority::from_name("widgets"), Some(RicePriority::Widgets));
    assert_eq!(RicePriority::from_name("force"), Some(RicePriority::Force));
    assert_eq!(RicePriority::from_name("off"), Some(RicePriority::Off));
    assert_eq!(RicePriority::from_name("5"), None);

    assert_eq!(RicePriority::Base.rule_priority(), Some(rule_priority::SYSTEM));
    assert_eq!(RicePriority::App.rule_priority(), Some(rule_priority::APP));
    assert_eq!(RicePriority::Widgets.rule_priority(), Some(rule_priority::WIDGETS));
    assert_eq!(RicePriority::Force.rule_priority(), Some(rule_priority::FORCE));
    assert_eq!(RicePriority::Palette.rule_priority(), Some(rule_priority::PALETTE));
    assert_eq!(RicePriority::Off.rule_priority(), None);

    assert_eq!(rule_priority::APP, 25);
    assert_eq!(rule_priority::WIDGETS, 35);
    assert_eq!(rule_priority::FORCE, 60);
    // Above the widgets' own inline declarations, below the app's runtime overrides...
    const _: () = assert!(rule_priority::INLINE < rule_priority::WIDGETS);
    const _: () = assert!(rule_priority::WIDGETS < rule_priority::RUNTIME);
    // ... and force above everything.
    const _: () = assert!(rule_priority::RUNTIME < rule_priority::FORCE);
}

#[test]
fn a_file_without_a_header_is_stamped_base() {
    let root = TempRoot::new("nohdr");
    root.write("css/xyz/a.css", ".a { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    assert_eq!(rule(loaded.css.rules.as_ref(), ".a").priority, rule_priority::SYSTEM);
    assert_eq!(loaded.status.files.as_ref()[0].priority, RicePriority::Base);
}

#[test]
fn a_widgets_header_stamps_every_rule_at_the_widgets_slot_in_either_comment_form() {
    let root = TempRoot::new("widgets");
    root.write("css/xyz/a.css", "// priority: widgets\n.a { color: red; }\n.b { color: blue; }");
    root.write("css/xyz/b.css", "/* priority: force */\n.c { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    let rules = loaded.css.rules.as_ref();
    assert_eq!(rule(rules, ".a").priority, rule_priority::WIDGETS);
    assert_eq!(rule(rules, ".b").priority, rule_priority::WIDGETS);
    assert_eq!(rule(rules, ".c").priority, rule_priority::FORCE);
}

#[test]
fn a_file_of_only_custom_properties_is_a_palette_rice_by_inspection() {
    let root = TempRoot::new("palette");
    root.write("css/xyz/pink/colors.css", ":root { --accent: #ff00ff; --bg: #202020; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz:pink", "xyz"]));
    let file = &loaded.status.files.as_ref()[0];
    assert_eq!(file.priority, RicePriority::Palette);
    for r in loaded.css.rules() {
        assert_eq!(r.priority, rule_priority::PALETTE);
    }
}

#[test]
fn a_declared_palette_that_sets_other_properties_is_demoted_to_base_with_a_warning() {
    let root = TempRoot::new("badpalette");
    root.write(
        "css/xyz/colors.css",
        "// priority: palette\n:root { --accent: #ff00ff; }\n.a { width: 10px; }",
    );
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    let file = &loaded.status.files.as_ref()[0];
    assert_eq!(file.priority, RicePriority::Base);
    assert_eq!(rule(loaded.css.rules.as_ref(), ".a").priority, rule_priority::SYSTEM);
    assert!(
        loaded.status.warnings.iter().any(|w| w.as_str().contains("palette")),
        "{:?}",
        loaded.status.warnings
    );
}

#[test]
fn a_priority_above_base_without_a_theme_is_clamped_to_base() {
    let root = TempRoot::new("clamp");
    // A GLOBAL file (directly under css/) with no `theme:` is live under every theme.
    root.write("css/everywhere.css", "// priority: force\n.a { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["flat"]));
    let a = rule(loaded.css.rules.as_ref(), ".a");
    assert_eq!(a.priority, rule_priority::SYSTEM, "clamped to base");
    assert!(a.conditions.as_ref().is_empty(), "an unthemed file is unconditional");
    assert!(
        loaded.status.warnings.iter().any(|w| w.as_str().contains("clamped")),
        "{:?}",
        loaded.status.warnings
    );
}

#[test]
fn a_global_file_with_a_theme_header_is_that_theme() {
    // "Base themes need no Rust": `css/foo.css` with `theme: abc-base` IS the theme abc-base.
    let root = TempRoot::new("globaltheme");
    root.write("css/foo.css", "// theme: abc-base; priority: widgets\n.a { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["abc-base", "flat"]));
    let a = rule(loaded.css.rules.as_ref(), ".a");
    assert_eq!(a.priority, rule_priority::WIDGETS, "a themed file is not clamped");
    assert_eq!(a.conditions.as_ref(), &[theme("abc-base")][..]);
}

// ---------------------------------------------------------------------------------------------
// versions: the `azul:` gate and `requires:`
// ---------------------------------------------------------------------------------------------

#[test]
fn azul_version_patterns_are_an_or_list_with_wildcards() {
    let p = |s: &[&str]| s.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
    assert!(azul_version_matches(&[], "0.2.0"), "absent means everywhere");
    assert!(azul_version_matches(&p(&["*"]), "0.2.0"));
    assert!(azul_version_matches(&p(&["0.2.*"]), "0.2.0"));
    assert!(azul_version_matches(&p(&["0.1.*", "0.2.*"]), "0.2.5"));
    assert!(azul_version_matches(&p(&["0.2"]), "0.2.7"), "a shorter version is a prefix");
    assert!(azul_version_matches(&p(&["0.2.0"]), "0.2.0"));
    assert!(!azul_version_matches(&p(&["0.2.1"]), "0.2.0"));
    assert!(!azul_version_matches(&p(&["0.1.*", "0.3.*"]), "0.2.0"));
    assert!(!azul_version_matches(&p(&["1"]), "0.2.0"));
}

#[test]
fn requires_ranges_follow_cargo_caret_semantics() {
    assert!(version_satisfies("^1.2", "1.4.0"));
    assert!(version_satisfies("^1.2", "1.2.0"));
    assert!(!version_satisfies("^1.2", "1.1.9"));
    assert!(!version_satisfies("^1.2", "2.0.0"));
    assert!(version_satisfies("1.2", "1.9.0"), "a bare version is a caret range");
    assert!(version_satisfies("^0.3", "0.3.9"));
    assert!(!version_satisfies("^0.3", "0.4.0"));
    assert!(version_satisfies("^0.0.3", "0.0.3"));
    assert!(!version_satisfies("^0.0.3", "0.0.4"));
    assert!(version_satisfies("~1.2", "1.2.9"));
    assert!(!version_satisfies("~1.2", "1.3.0"));
    assert!(version_satisfies("=1.2.3", "1.2.3"));
    assert!(!version_satisfies("=1.2.3", "1.2.4"));
    assert!(version_satisfies("*", "7.0.0"));
    assert!(version_satisfies("1.*", "1.5.0"));
    assert!(!version_satisfies("1.*", "2.0.0"));
}

#[test]
fn the_azul_key_is_the_one_gate_a_file_for_another_engine_is_ignored_with_one_warning() {
    let root = TempRoot::new("azulgate");
    let mut env = root.env("myapp");
    env.azul_version = "0.2.0".to_string();
    root.write("css/xyz/old.css", "// azul: 0.1.*\n.old { color: red; }");
    root.write("css/xyz/now.css", "// azul: 0.2.*, 0.3.*\n.now { color: red; }");

    let loaded = load_rice(&env, &chain(&["xyz"]));
    let order: Vec<String> = loaded.css.rules().map(selector).collect();
    assert_eq!(order, vec![".now"], "the file bounded to 0.1 contributes nothing");

    let old = loaded
        .status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("old.css"))
        .expect("the ignored file is still listed");
    assert_eq!(old.state, RiceFileState::AzulVersionMismatch);
    let lines = loaded
        .status
        .warnings
        .iter()
        .filter(|w| w.as_str().contains("old.css"))
        .count();
    assert_eq!(lines, 1, "exactly one log line: {:?}", loaded.status.warnings);
}

#[test]
fn requires_never_gates_it_applies_and_attributes_a_mismatch() {
    let root = TempRoot::new("requires");
    root.write("css/abc-base/base.css", "// theme: abc-base@2.0.0; priority: widgets\n.base { color: red; }");
    root.write(
        "css/abc-base/pink/pink.css",
        "// theme: abc-base:pink@0.3.1; requires: abc-base@^1.2\n.pink { color: pink; }",
    );
    let loaded = load_rice(&root.env("myapp"), &chain(&["abc-base:pink", "abc-base"]));
    let order: Vec<String> = loaded.css.rules().map(selector).collect();
    assert_eq!(order, vec![".base", ".pink"], "both apply, never clamped");

    let pink = loaded
        .status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("pink.css"))
        .expect("listed");
    assert_eq!(pink.state, RiceFileState::Applied);
    assert!(!pink.requires_ok);
    assert!(pink.requires_check.as_str().contains("2.0.0"), "{}", pink.requires_check.as_str());
    assert_eq!(pink.version.as_str(), "0.3.1");
    assert!(loaded.status.warnings.iter().any(|w| w.as_str().contains("^1.2")));
}

#[test]
fn a_satisfied_requirement_reads_ok() {
    let root = TempRoot::new("requiresok");
    root.write("css/abc-base/base.css", "// theme: abc-base@1.4.0\n.base { color: red; }");
    root.write("css/abc-base/pink/pink.css", "// requires: abc-base@^1.2\n.pink { color: pink; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["abc-base:pink", "abc-base"]));
    let pink = loaded
        .status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("pink.css"))
        .expect("listed");
    assert!(pink.requires_ok, "{}", pink.requires_check.as_str());
}

// ---------------------------------------------------------------------------------------------
// `app:` and per-app versus global precedence (pitfall 7)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_file_whose_app_key_names_another_application_is_inert() {
    let root = TempRoot::new("app");
    root.write("css/xyz/mine.css", "// app: myapp\n.mine { color: red; }");
    root.write("css/xyz/theirs.css", "// app: otherapp, thirdapp\n.theirs { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    let order: Vec<String> = loaded.css.rules().map(selector).collect();
    assert_eq!(order, vec![".mine"]);
    let theirs = loaded
        .status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("theirs.css"))
        .expect("listed");
    assert_eq!(theirs.state, RiceFileState::OtherApp);
}

#[test]
fn a_per_app_base_turns_a_global_widgets_rice_down_for_that_app_only() {
    let root = TempRoot::new("perappdown");
    root.write("css/monokai/theme.css", "// priority: widgets\n.btn { color: red; }");
    root.write("css/monokai/fix-myapp.css", "// app: myapp; priority: base\n");

    let mine = load_rice(&root.env("myapp"), &chain(&["monokai"]));
    assert_eq!(rule(mine.css.rules.as_ref(), ".btn").priority, rule_priority::SYSTEM);

    let other = load_rice(&root.env("otherapp"), &chain(&["monokai"]));
    assert_eq!(rule(other.css.rules.as_ref(), ".btn").priority, rule_priority::WIDGETS);
}

#[test]
fn a_per_app_off_turns_the_theme_off_for_that_app() {
    let root = TempRoot::new("perappoff");
    root.write("css/monokai/theme.css", "// priority: widgets\n.btn { color: red; }");
    root.write("css/monokai/fix-myapp.css", "// app: myapp; priority: off\n");
    let loaded = load_rice(&root.env("myapp"), &chain(&["monokai"]));
    assert_eq!(loaded.css.rules().count(), 0);
    let theme_file = loaded
        .status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("theme.css"))
        .expect("listed");
    assert_eq!(theme_file.state, RiceFileState::Off);
}

#[test]
fn a_per_app_priority_wins_upwards_too() {
    let root = TempRoot::new("perappup");
    root.write("css/monokai/theme.css", ".btn { color: red; }");
    root.write("css/monokai/more-myapp.css", "// app: myapp; priority: widgets\n.extra { color: blue; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["monokai"]));
    let rules = loaded.css.rules.as_ref();
    assert_eq!(rule(rules, ".btn").priority, rule_priority::WIDGETS);
    assert_eq!(rule(rules, ".extra").priority, rule_priority::WIDGETS);
}

#[test]
fn per_app_rules_come_after_the_global_rules_of_the_same_theme() {
    let root = TempRoot::new("perapporder");
    // `a-` sorts before `z-`: file order alone would put the per-app file first.
    root.write("css/monokai/a-myapp.css", "// app: myapp\n.perapp { color: blue; }");
    root.write("css/monokai/z-theme.css", ".global { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["monokai"]));
    let order: Vec<String> = loaded.css.rules().map(selector).collect();
    assert_eq!(order, vec![".global", ".perapp"]);
}

#[test]
fn the_legacy_per_app_stylesheet_still_loads_as_a_per_app_file() {
    let root = TempRoot::new("legacy");
    let legacy = root.write("styles/myapp.css", ".legacy { color: red; }");
    let mut env = root.env("myapp");
    env.legacy_file = Some(legacy);
    let loaded = load_rice(&env, &chain(&["flat"]));
    let r = rule(loaded.css.rules.as_ref(), ".legacy");
    assert_eq!(r.priority, rule_priority::SYSTEM, "today's behaviour: base");
    assert!(r.conditions.as_ref().is_empty(), "unthemed: live under every theme");
    let f = &loaded.status.files.as_ref()[0];
    assert!(f.per_app);
    assert_eq!(f.app.as_str(), "myapp");
}

// ---------------------------------------------------------------------------------------------
// hardening (pitfall 8)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_remote_url_is_dropped_with_a_warning_and_the_rest_of_the_rule_stays() {
    let s = sanitize_rice_source(
        ".a { background-image: url(https://evil.example/x.png); color: #ff0000; }\n\
         .b { background-image: url(\"//evil.example/y.png\") }\n\
         .c { background-image: url(local.png); }\n\
         .d { background-image: url(data:image/png;base64,AAAA); }",
    );
    assert!(!s.source.contains("evil.example"), "{}", s.source);
    assert!(s.source.contains("color: #ff0000"), "{}", s.source);
    assert!(s.source.contains("local.png"), "a relative url is not a fetch");
    assert!(s.source.contains("data:image/png"), "a data: url is inline");
    assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
}

#[test]
fn an_import_is_dropped() {
    let s = sanitize_rice_source("@import url(https://evil.example/x.css);\n.a { color: red; }");
    assert!(!s.source.contains("evil.example"), "{}", s.source);
    assert!(s.source.contains(".a { color: red; }"));
    assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
}

#[test]
fn a_remote_url_never_reaches_the_cascade() {
    let root = TempRoot::new("url");
    root.write(
        "css/xyz/a.css",
        ".a { background-image: url(https://evil.example/x.png); color: #ff0000; }",
    );
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    let a = rule(loaded.css.rules.as_ref(), ".a");
    assert_eq!(a.declarations.as_ref().len(), 1, "only the colour survives");
    assert!(loaded.status.warnings.iter().any(|w| w.as_str().contains("evil.example")));
}

#[test]
fn a_file_over_the_size_limit_is_refused() {
    let root = TempRoot::new("size");
    let mut big = String::from(".a { color: red; }\n");
    while (big.len() as u64) <= MAX_RICE_FILE_BYTES {
        big.push_str("/* padding padding padding padding padding padding padding */\n");
    }
    root.write("css/xyz/big.css", &big);
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    assert_eq!(loaded.css.rules().count(), 0);
    assert_eq!(loaded.status.files.as_ref()[0].state, RiceFileState::Refused);
}

#[test]
fn a_theme_name_that_would_leave_the_rice_root_is_refused() {
    assert_eq!(theme_dir_segments("../etc"), None);
    assert_eq!(theme_dir_segments("a/b"), None);
    assert_eq!(theme_dir_segments("a\\b"), None);
    assert_eq!(theme_dir_segments("a:..:b"), None);
    assert_eq!(theme_dir_segments("a::b"), None);
    assert_eq!(theme_dir_segments(""), None);
    assert_eq!(theme_dir_segments("/abs"), None);
    // Reserved for the colour scheme (pitfall 5).
    assert_eq!(theme_dir_segments("dark"), None);
    assert_eq!(theme_dir_segments("light"), None);

    let root = TempRoot::new("traversal");
    root.write("outside.css", ".outside { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["..", "../..", "flat"]));
    assert_eq!(loaded.css.rules().count(), 0);
    assert!(!loaded.status.warnings.is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_rice_root_is_refused() {
    let root = TempRoot::new("symlink");
    let elsewhere = TempRoot::new("symlink-target");
    let target = elsewhere.write("secret.css", ".secret { color: red; }");
    fs::create_dir_all(root.path().join("css/xyz")).expect("dir");
    std::os::unix::fs::symlink(&target, root.path().join("css/xyz/link.css")).expect("symlink");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    assert_eq!(loaded.css.rules().count(), 0);
    assert_eq!(loaded.status.files.as_ref()[0].state, RiceFileState::Refused);
}

// ---------------------------------------------------------------------------------------------
// status / attribution
// ---------------------------------------------------------------------------------------------

#[test]
fn the_status_lists_the_chain_every_file_and_live_versus_inert_rules() {
    let root = TempRoot::new("status");
    root.write(
        "css/xyz/a.css",
        "// theme: xyz@1.0.0; priority: widgets\n\
         .everywhere { color: red; }\n\
         @os windows { .windows-only { color: blue; } }",
    );
    root.write("css/xyz/other.css", "// app: otherapp\n.x { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz", "flat"]));

    let mut style = SystemStyle::default();
    style.platform = Platform::MacOs;
    let ctx = DynamicSelectorContext::from_system_style(&style).with_app_theme("xyz");
    let status = loaded.status_under(&ctx);

    let a = status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("a.css"))
        .expect("listed");
    assert_eq!(a.state, RiceFileState::Applied);
    assert_eq!(a.priority, RicePriority::Widgets);
    assert_eq!(a.version.as_str(), "1.0.0");
    assert_eq!(a.theme.as_str(), "xyz");
    assert_eq!(a.live_rules, 1, "the unconditional rule");
    assert_eq!(a.inert_rules, 1, "@os windows is false on macOS");

    let other = status
        .files
        .iter()
        .find(|f| f.path.as_str().ends_with("other.css"))
        .expect("listed");
    assert_eq!(other.live_rules, 0);
    assert_eq!(other.inert_rules, 1, "every rule of an inert file is inert");

    let report = status.to_report();
    assert!(report.contains("xyz"), "{report}");
    assert!(report.contains("a.css"), "{report}");
    assert!(report.contains("widgets"), "{report}");
    assert!(report.contains("1.0.0"), "{report}");
    assert!(report.contains("AZ_RICING=off"), "the self-check is in every report: {report}");
}

#[test]
fn the_same_rules_are_inert_when_their_theme_is_not_live() {
    let root = TempRoot::new("inerttheme");
    root.write("css/xyz/a.css", ".a { color: red; }");
    let loaded = load_rice(&root.env("myapp"), &chain(&["xyz"]));
    let ctx = DynamicSelectorContext::from_system_style(&SystemStyle::default()).with_app_theme("flora");
    let status = loaded.status_under(&ctx);
    let a = &status.files.as_ref()[0];
    assert_eq!(a.live_rules, 0);
    assert_eq!(a.inert_rules, 1);
}

// ---------------------------------------------------------------------------------------------
// AZ_RICING
// ---------------------------------------------------------------------------------------------

#[test]
fn az_ricing_has_an_off_a_force_and_a_watch_mode() {
    assert_eq!(ricing_mode_from(None), RicingMode::Default);
    assert_eq!(ricing_mode_from(Some("off")), RicingMode::Off);
    assert_eq!(ricing_mode_from(Some("0")), RicingMode::Off);
    assert_eq!(ricing_mode_from(Some("FORCE")), RicingMode::Force);
    assert_eq!(ricing_mode_from(Some("watch")), RicingMode::Watch);
    assert_eq!(ricing_mode_from(Some("live")), RicingMode::Watch);
    assert_eq!(ricing_mode_from(Some("typo")), RicingMode::Default);
}
