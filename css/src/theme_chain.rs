//! The THEME CHAIN: which app themes are live, most specific first.
//!
//! "Theme" is the app theme (`flat`, `flora`, later `native` and user
//! themes, and their spin-offs such as `xyz:pink`); "mode" is light / dark /
//! system. The two are separate axes: a theme switch rebuilds the DOM, a mode
//! switch only repaints.
//!
//! A chain is built like a locale fallback list (`fr-CA -> fr -> en`,
//! `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` §7.1):
//!
//! 1. the head expands by its `:` prefixes: `xyz:pink` -> `[xyz:pink, xyz]`;
//! 2. every entry's `fallback:` list (the rice file header, §4.1) is appended,
//!    transitively, in chain order, keeping each name's FIRST (highest)
//!    rank. Where the head's header and a lower theme's header disagree, the
//!    head's list comes first, so the head wins (§9.1 pitfall 4). A fallback
//!    that leads back to a theme it builds on is a cycle: cut, with one
//!    warning;
//! 3. the app's default theme is always the last entry, the floor every
//!    chain ends in, so an unknown theme degrades to the default look.
//!
//! The mode's words (`light`, `dark`, `system`, `auto`) are never theme
//! names: in a chain they are errors and are dropped (§9.1 pitfall 5).
//!
//! Two environment variables choose them for every app a user runs
//! ([`theme_env`]):
//!
//! - `AZ_THEME=<theme>` names the chain's HEAD, and outranks the app's own
//!   choice: `AZ_THEME` > `AppConfig::with_theme` / `CallbackInfo::set_theme`
//!   > the default theme ([`resolve_theme_head`]).
//! - `AZ_MODE=light|dark|system` pins the MODE (screenshots, reftests, CI):
//!   `AZ_MODE` > the app's mode > the window's > the desktop's. `system`
//!   pins nothing.
//!
//! `AZ_THEME=light|dark` was the mode pin before `AZ_THEME` named the theme;
//! for one release it still pins the mode, with a deprecation line pointing
//! at `AZ_MODE`.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    dynamic_selector::{ThemeCondition, DEFAULT_APP_THEME},
    AzString,
};

/// What the environment asks of the theme and the mode: `AZ_THEME` and
/// `AZ_MODE`, read once per process ([`theme_env`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThemeEnv {
    /// The theme chain's head `AZ_THEME` names (`xyz:pink`); `None` leaves
    /// the app's choice. Never a mode word: those are the deprecated alias.
    pub theme: Option<String>,
    /// The mode `AZ_MODE` pins (or, deprecated, `AZ_THEME=light|dark`):
    /// [`ThemeCondition::Light`] or [`ThemeCondition::Dark`]; `None` pins
    /// nothing.
    pub mode: Option<ThemeCondition>,
    /// One line per value that is not taken as written (the deprecated
    /// alias, an unknown `AZ_MODE`), for the app to log once.
    pub warnings: Vec<String>,
}

impl ThemeEnv {
    /// The request made by the VALUES of `AZ_THEME` and `AZ_MODE` (`None`:
    /// unset) - the testable core of [`theme_env`], which reads them from the
    /// process environment. Values are trimmed; a blank one is unset.
    #[must_use]
    pub fn from_values(az_theme: Option<&str>, az_mode: Option<&str>) -> Self {
        let mut env = Self::default();
        let az_theme = az_theme.map(str::trim).filter(|value| !value.is_empty());
        let az_mode = az_mode.map(str::trim).filter(|value| !value.is_empty());

        // `AZ_MODE`, when it reads as a mode, decides the mode - `system`
        // included, which decides "no pin".
        let mut mode_decided = false;
        if let Some(value) = az_mode {
            let word = ModeWord::of(value);
            if word == ModeWord::NotAMode {
                env.warnings.push(format!(
                    "AZ_MODE={value} is not light, dark or system - ignored."
                ));
            } else {
                env.mode = word.pin();
                mode_decided = true;
            }
        }

        // `AZ_THEME` names a theme - unless it is a mode word, the pin it
        // used to be (the one-release alias).
        if let Some(value) = az_theme {
            let word = ModeWord::of(value);
            let instead = value.to_ascii_lowercase();
            if word == ModeWord::NotAMode {
                env.theme = Some(value.to_string());
            } else if mode_decided {
                env.warnings.push(format!(
                    "AZ_THEME={value} is the deprecated spelling of AZ_MODE={instead}, and \
                     AZ_MODE is set - ignored. AZ_THEME names the app theme (flat, flora, \
                     xyz:pink)."
                ));
            } else {
                env.mode = word.pin();
                env.warnings.push(format!(
                    "AZ_THEME={value} as the light / dark pin is deprecated: AZ_THEME names \
                     the app theme (flat, flora, xyz:pink), and from the next release only \
                     that. Set AZ_MODE={instead} instead."
                ));
            }
        }
        env
    }
}

/// How a value of `AZ_MODE` - or of `AZ_THEME` as the deprecated alias -
/// reads. The mode's words are exactly the names no theme may take
/// ([`is_reserved_theme_name`]).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModeWord {
    Light,
    Dark,
    /// `system` / `auto`: follow the app and the desktop, no pin.
    System,
    NotAMode,
}

impl ModeWord {
    fn of(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "system" | "auto" => Self::System,
            _ => Self::NotAMode,
        }
    }

    /// The pin this word asks for; `system` asks for none.
    const fn pin(self) -> Option<ThemeCondition> {
        match self {
            Self::Light => Some(ThemeCondition::Light),
            Self::Dark => Some(ThemeCondition::Dark),
            Self::System | Self::NotAMode => None,
        }
    }
}

/// The environment's request ([`ThemeEnv`]), read from `AZ_THEME` and
/// `AZ_MODE` on first use and kept for the life of the process: the theme
/// chain is a startup decision, and a pin that changed mid-run would make a
/// screenshot disagree with itself.
#[must_use]
pub fn theme_env() -> &'static ThemeEnv {
    static ENV: std::sync::OnceLock<ThemeEnv> = std::sync::OnceLock::new();
    ENV.get_or_init(|| {
        let theme = std::env::var("AZ_THEME").ok();
        let mode = std::env::var("AZ_MODE").ok();
        ThemeEnv::from_values(theme.as_deref(), mode.as_deref())
    })
}

/// THE app-theme decision: the head of the theme chain.
///
/// ```text
/// AZ_THEME  >  the app's choice (AppConfig::with_theme, CallbackInfo::set_theme)  >  DEFAULT_APP_THEME
/// ```
///
/// The environment outranks the app, as the old `AZ_THEME` mode pin did: it
/// is the user's choice for every app they run, and a screenshot run's.
#[must_use]
pub const fn resolve_theme_head<'a>(env: Option<&'a str>, app: Option<&'a str>) -> &'a str {
    match (env, app) {
        (Some(name), _) | (None, Some(name)) => name,
        (None, None) => DEFAULT_APP_THEME,
    }
}

/// A theme chain, most specific first, and what building it had to drop or
/// cut (a reserved or malformed name, a `fallback:` cycle), as log lines.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThemeChain {
    /// The live themes, most specific first; the app's default theme last.
    pub names: Vec<AzString>,
    /// One line per name dropped or cycle cut, for the caller to log.
    pub warnings: Vec<String>,
}

/// Is `name` one of the mode's words - `light`, `dark`, `system`, `auto`, in
/// any case? No theme may take one: `@theme(dark)` is the mode, and a theme
/// called `dark` would shadow it and the deprecated `AZ_THEME=dark` alias
/// (§9.1 pitfall 5).
#[must_use]
pub fn is_reserved_theme_name(name: &str) -> bool {
    ModeWord::of(name) != ModeWord::NotAMode
}

/// Can `name` name an app theme: non-empty, and only ASCII letters, digits,
/// `-`, `_` and `:` (the spin-off separator)?
///
/// The one character rule of the `@theme(<name>)` parser
/// (`ThemeCondition::from_block_name`) and the chain, so a chain never holds
/// a name no block can carry.
#[must_use]
pub fn is_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

/// The theme chain `head` activates (see the module docs): `head` and its
/// `:` prefixes, then every entry's `fallback_of` list appended transitively
/// and de-duplicated, then `app_default`.
///
/// `fallback_of(name)` is the `fallback:` key of the theme `name`'s rice file
/// header, empty for a theme without one. THE chain builder: the cascade's
/// context (`dynamic_selector::app_theme_chain`) and the rice loader both
/// call it, so the two can never disagree about which themes are live.
///
/// Never fails: a reserved or malformed name is dropped and a `fallback:`
/// cycle is cut, each with a line in [`ThemeChain::warnings`]. `app_default`
/// is always the last entry and only there - a header that names it does not
/// move the floor up - and its own `fallback:` is not followed.
#[must_use]
pub fn expand_chain(
    head: &str,
    fallback_of: &dyn Fn(&str) -> Vec<String>,
    app_default: &str,
) -> ThemeChain {
    let app_default = app_default.trim();
    let mut chain = ChainBuilder {
        names: Vec::new(),
        brought_in_by: Vec::new(),
        warnings: Vec::new(),
        app_default,
    };
    let head = head.trim();
    if !head.is_empty() {
        chain.add_with_prefixes(head, None);
    }
    // Every entry's `fallback:` list, in chain order: the entries a list
    // appends are visited in turn (transitive), and an earlier - more
    // specific - entry's list lands above a later one's, which is how the
    // head's header wins where a base's header disagrees.
    let mut next = 0;
    while next < chain.names.len() {
        let name = chain.names[next].clone();
        for fallback in fallback_of(name.as_str()) {
            chain.add_fallback(fallback.trim(), next);
        }
        next += 1;
    }
    let ChainBuilder {
        mut names,
        warnings,
        ..
    } = chain;
    if !app_default.is_empty() {
        names.push(app_default.to_string());
    }
    ThemeChain {
        names: names.into_iter().map(AzString::from).collect(),
        warnings,
    }
}

/// A chain under construction: each entry with the entry that brought it in
/// (by the prefix rule or by a `fallback:`), which is what tells a cycle from
/// a diamond.
struct ChainBuilder<'a> {
    names: Vec<String>,
    brought_in_by: Vec<Option<usize>>,
    warnings: Vec<String>,
    app_default: &'a str,
}

impl ChainBuilder<'_> {
    fn position(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|live| live == name)
    }

    /// Does the entry `from` build on the entry `at`: is `at` the entry
    /// itself or one of the entries that brought it in?
    fn builds_on(&self, from: usize, at: usize) -> bool {
        let mut cursor = Some(from);
        while let Some(entry) = cursor {
            if entry == at {
                return true;
            }
            cursor = self.brought_in_by.get(entry).copied().flatten();
        }
        false
    }

    /// Can `name` be in a chain? If not, says why in the warnings.
    fn accepts(&mut self, name: &str) -> bool {
        let problem = if is_reserved_theme_name(name) {
            format!(
                "theme chain: `{name}` is a mode (light / dark / system), not a theme - \
                 dropped. The mode is set with AZ_MODE."
            )
        } else if !is_theme_name(name) || name.split(':').any(str::is_empty) {
            format!(
                "theme chain: `{name}` is not a theme name (ASCII letters, digits, `-`, `_`, \
                 and `:` between non-empty parts) - dropped."
            )
        } else {
            return true;
        };
        self.warnings.push(problem);
        false
    }

    /// Add `name` and its `:` prefixes, longest first, brought in by `from`.
    /// A name already in the chain keeps its higher place; the app default
    /// is left for the end.
    fn add_with_prefixes(&mut self, name: &str, from: Option<usize>) {
        if !self.accepts(name) {
            return;
        }
        let mut parent = from;
        let prefixes = name.rmatch_indices(':').map(|(at, _)| &name[..at]);
        for entry in core::iter::once(name).chain(prefixes) {
            // A prefix of a valid name can only fail as a mode word (`dark:pink`).
            if entry != name && !self.accepts(entry) {
                continue;
            }
            if entry == self.app_default {
                continue;
            }
            if let Some(at) = self.position(entry) {
                parent = Some(at);
                continue;
            }
            self.names.push(entry.to_string());
            self.brought_in_by.push(parent);
            parent = Some(self.names.len() - 1);
        }
    }

    /// The entry `from`'s header names `name` in its `fallback:` list.
    fn add_fallback(&mut self, name: &str, from: usize) {
        if name.is_empty() || name == self.app_default {
            return;
        }
        if let Some(at) = self.position(name) {
            // Already live, higher up: a diamond keeps that rank silently; a
            // theme falling back to one it builds on is a cycle.
            if self.builds_on(from, at) {
                let line = format!(
                    "theme chain: `{}` falls back to `{name}`, which it already builds on - \
                     a cycle, cut there.",
                    self.names[from]
                );
                self.warnings.push(line);
            }
            return;
        }
        self.add_with_prefixes(name, Some(from));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(chain: &ThemeChain) -> Vec<&str> {
        chain.names.iter().map(AzString::as_str).collect()
    }

    fn no_headers(_: &str) -> Vec<String> {
        Vec::new()
    }

    /// `fallback_of` for a set of rice file headers: `(theme, its fallback: list)`.
    fn headers(table: &'static [(&'static str, &'static [&'static str])]) -> impl Fn(&str) -> Vec<String> {
        move |name: &str| {
            table
                .iter()
                .find(|(theme, _)| *theme == name)
                .map(|(_, list)| list.iter().map(|s| (*s).to_string()).collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn a_spin_off_expands_to_its_prefixes_and_ends_in_the_app_default() {
        let chain = expand_chain("xyz:pink", &no_headers, "flat");
        assert_eq!(names(&chain), ["xyz:pink", "xyz", "flat"]);
        assert!(chain.warnings.is_empty(), "{:?}", chain.warnings);

        let deep = expand_chain("a:b:c", &no_headers, "flat");
        assert_eq!(names(&deep), ["a:b:c", "a:b", "a", "flat"]);
    }

    #[test]
    fn a_theme_without_a_prefix_or_a_header_is_itself_over_the_default() {
        let chain = expand_chain("abc", &no_headers, "flat");
        assert_eq!(names(&chain), ["abc", "flat"]);
        assert!(chain.warnings.is_empty(), "{:?}", chain.warnings);
    }

    #[test]
    fn the_app_default_alone_is_a_chain_of_one() {
        for head in ["flat", "", "   "] {
            let chain = expand_chain(head, &no_headers, "flat");
            assert_eq!(names(&chain), ["flat"], "head {head:?}");
            assert!(chain.warnings.is_empty(), "head {head:?}: {:?}", chain.warnings);
        }
    }

    #[test]
    fn fallbacks_are_appended_transitively_after_the_prefix_chain() {
        let fallback_of = headers(&[("abc", &["native"]), ("native", &["flora"])]);
        let chain = expand_chain("abc", &fallback_of, "flat");
        assert_eq!(names(&chain), ["abc", "native", "flora", "flat"]);
        assert!(chain.warnings.is_empty(), "{:?}", chain.warnings);

        // A base reached by the prefix rule brings its own header along.
        let fallback_of = headers(&[("xyz", &["nord"])]);
        let chain = expand_chain("xyz:pink", &fallback_of, "flat");
        assert_eq!(names(&chain), ["xyz:pink", "xyz", "nord", "flat"]);

        // A fallback is a theme like any other: a spin-off one brings its base.
        let fallback_of = headers(&[("abc", &["nord:frost"])]);
        let chain = expand_chain("abc", &fallback_of, "flat");
        assert_eq!(names(&chain), ["abc", "nord:frost", "nord", "flat"]);
    }

    /// §9.1 pitfall 4: `xyz:pink` says `fallback: native`, `xyz` says `fallback: flora`.
    #[test]
    fn the_heads_header_wins_where_the_chain_disagrees() {
        let fallback_of = headers(&[("xyz:pink", &["native"]), ("xyz", &["flora"])]);
        let chain = expand_chain("xyz:pink", &fallback_of, "flat");
        assert_eq!(names(&chain), ["xyz:pink", "xyz", "native", "flora", "flat"]);
        assert!(chain.warnings.is_empty(), "a disagreement is not an error: {:?}", chain.warnings);
    }

    #[test]
    fn a_theme_named_twice_keeps_its_first_its_higher_rank() {
        // A diamond, not a cycle: `c` is reached from `a` and from `b`.
        let fallback_of = headers(&[("a", &["b", "c"]), ("b", &["c", "d"])]);
        let chain = expand_chain("a", &fallback_of, "flat");
        assert_eq!(names(&chain), ["a", "b", "c", "d", "flat"]);
        assert!(chain.warnings.is_empty(), "{:?}", chain.warnings);
    }

    #[test]
    fn a_fallback_cycle_is_cut_with_one_warning() {
        let fallback_of = headers(&[("a", &["b"]), ("b", &["a"])]);
        let chain = expand_chain("a", &fallback_of, "flat");
        assert_eq!(names(&chain), ["a", "b", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "{:?}", chain.warnings);
        assert!(chain.warnings[0].contains("cycle"), "{:?}", chain.warnings);

        let fallback_of = headers(&[("a", &["a"])]);
        let chain = expand_chain("a", &fallback_of, "flat");
        assert_eq!(names(&chain), ["a", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "a theme falling back to itself: {:?}", chain.warnings);

        // Back up through the prefix rule: `xyz` builds on nothing `xyz:pink` does not.
        let fallback_of = headers(&[("xyz", &["xyz:pink"])]);
        let chain = expand_chain("xyz:pink", &fallback_of, "flat");
        assert_eq!(names(&chain), ["xyz:pink", "xyz", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "{:?}", chain.warnings);
    }

    #[test]
    fn the_app_default_is_always_last_even_when_a_header_names_it() {
        let fallback_of = headers(&[("abc", &["flat", "native"])]);
        let chain = expand_chain("abc", &fallback_of, "flat");
        assert_eq!(names(&chain), ["abc", "native", "flat"]);

        let chain = expand_chain("flat:pink", &no_headers, "flat");
        assert_eq!(names(&chain), ["flat:pink", "flat"]);
        assert!(chain.warnings.is_empty(), "{:?}", chain.warnings);
    }

    /// §9.1 pitfall 5: a theme called `dark` would shadow the mode.
    #[test]
    fn the_mode_words_are_reserved_and_dropped_with_an_error() {
        for word in ["light", "dark", "system", "auto", "Dark", "SYSTEM"] {
            let chain = expand_chain(word, &no_headers, "flat");
            assert_eq!(names(&chain), ["flat"], "head {word}");
            assert_eq!(chain.warnings.len(), 1, "head {word}: {:?}", chain.warnings);
            assert!(
                chain.warnings[0].contains(word) && chain.warnings[0].contains("AZ_MODE"),
                "the error names the word and points at the mode's variable: {:?}",
                chain.warnings
            );
        }

        let fallback_of = headers(&[("abc", &["dark", "native"])]);
        let chain = expand_chain("abc", &fallback_of, "flat");
        assert_eq!(names(&chain), ["abc", "native", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "{:?}", chain.warnings);

        // A spin-off OF a mode word is a legal name; its prefix is not.
        let chain = expand_chain("dark:pink", &no_headers, "flat");
        assert_eq!(names(&chain), ["dark:pink", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "{:?}", chain.warnings);
    }

    #[test]
    fn a_malformed_name_is_dropped_with_an_error() {
        for head in ["my theme", "xyz::pink", ":pink", "pink:"] {
            let chain = expand_chain(head, &no_headers, "flat");
            assert_eq!(names(&chain), ["flat"], "head {head:?}");
            assert_eq!(chain.warnings.len(), 1, "head {head:?}: {:?}", chain.warnings);
        }

        let fallback_of = headers(&[("abc", &["bad name!"])]);
        let chain = expand_chain("abc", &fallback_of, "flat");
        assert_eq!(names(&chain), ["abc", "flat"]);
        assert_eq!(chain.warnings.len(), 1, "{:?}", chain.warnings);
    }

    // ── AZ_THEME / AZ_MODE ────────────────────────────────────────────────
    //
    // Through `ThemeEnv::from_values`, never `std::env::set_var`: the
    // environment is process-global, and a test that set it would race every
    // other test thread of the binary (the rule `system.rs`'s `AZ_RICING`
    // tests follow). `theme_env()` itself only reads the variables.

    #[test]
    fn az_mode_pins_the_mode() {
        for (value, mode) in [
            ("dark", ThemeCondition::Dark),
            ("light", ThemeCondition::Light),
            (" Dark ", ThemeCondition::Dark),
            ("LIGHT", ThemeCondition::Light),
        ] {
            let env = ThemeEnv::from_values(None, Some(value));
            assert_eq!(env.mode, Some(mode), "AZ_MODE={value:?}");
            assert_eq!(env.theme, None, "AZ_MODE={value:?} names no theme");
            assert!(env.warnings.is_empty(), "AZ_MODE={value:?}: {:?}", env.warnings);
        }
    }

    #[test]
    fn az_mode_system_pins_nothing() {
        for value in ["system", "System", "auto"] {
            let env = ThemeEnv::from_values(None, Some(value));
            assert_eq!(env.mode, None, "AZ_MODE={value}");
            assert!(env.warnings.is_empty(), "AZ_MODE={value}: {:?}", env.warnings);
        }
    }

    #[test]
    fn an_unknown_az_mode_pins_nothing_and_says_so() {
        let env = ThemeEnv::from_values(None, Some("blue"));
        assert_eq!(env.mode, None);
        assert_eq!(env.warnings.len(), 1, "{:?}", env.warnings);
        assert!(env.warnings[0].contains("AZ_MODE=blue"), "{:?}", env.warnings);
    }

    /// The one-release alias: `AZ_THEME=light|dark` was the mode pin.
    #[test]
    fn az_theme_dark_still_pins_the_mode_and_points_at_az_mode() {
        for (value, mode, instead) in [
            ("dark", ThemeCondition::Dark, "AZ_MODE=dark"),
            ("Light", ThemeCondition::Light, "AZ_MODE=light"),
        ] {
            let env = ThemeEnv::from_values(Some(value), None);
            assert_eq!(env.mode, Some(mode), "AZ_THEME={value}");
            assert_eq!(env.theme, None, "AZ_THEME={value} is a mode, never a theme");
            assert_eq!(env.warnings.len(), 1, "AZ_THEME={value}: {:?}", env.warnings);
            assert!(
                env.warnings[0].contains("deprecated") && env.warnings[0].contains(instead),
                "AZ_THEME={value}: {:?}",
                env.warnings
            );
        }
        // `system` / `auto` were never pins; as `AZ_THEME` they are still mode words.
        let env = ThemeEnv::from_values(Some("system"), None);
        assert_eq!((env.mode, env.theme), (None, None));
        assert_eq!(env.warnings.len(), 1, "{:?}", env.warnings);
    }

    #[test]
    fn az_mode_outranks_the_deprecated_az_theme_alias() {
        let env = ThemeEnv::from_values(Some("dark"), Some("light"));
        assert_eq!(env.mode, Some(ThemeCondition::Light));
        assert_eq!(env.theme, None);
        assert_eq!(env.warnings.len(), 1, "the alias is still reported: {:?}", env.warnings);

        let env = ThemeEnv::from_values(Some("dark"), Some("system"));
        assert_eq!(env.mode, None, "AZ_MODE=system is a decision too: no pin");
    }

    #[test]
    fn az_theme_names_the_head_of_the_theme_chain() {
        let env = ThemeEnv::from_values(Some("xyz:pink"), None);
        assert_eq!(env.theme.as_deref(), Some("xyz:pink"));
        assert_eq!(env.mode, None, "a theme is not a mode");
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);

        let both = ThemeEnv::from_values(Some(" flora "), Some("dark"));
        assert_eq!(both.theme.as_deref(), Some("flora"));
        assert_eq!(both.mode, Some(ThemeCondition::Dark));
        assert!(both.warnings.is_empty(), "{:?}", both.warnings);
    }

    #[test]
    fn unset_or_blank_variables_ask_for_nothing() {
        assert_eq!(ThemeEnv::from_values(None, None), ThemeEnv::default());
        assert_eq!(ThemeEnv::from_values(Some(""), Some("  ")), ThemeEnv::default());
    }

    #[test]
    fn the_environment_outranks_the_app_which_outranks_the_default() {
        assert_eq!(resolve_theme_head(Some("xyz:pink"), Some("flora")), "xyz:pink");
        assert_eq!(resolve_theme_head(None, Some("flora")), "flora");
        assert_eq!(resolve_theme_head(None, None), DEFAULT_APP_THEME);
    }
}
