//! `ShellThemeScope` - the app's root: the widget theme's ground and an ACCENT
//! family, set once (05-widget-backlog.md, `ShellThemeScope`).
//!
//! The app THEME (flat, flora, ...) is the window's: `AppConfig::with_theme`
//! chooses it at start, `CallbackInfo::set_theme` switches it, and every
//! widget that follows the app theme picks its `@theme(<name>)` block by it.
//! What the widgets cannot know is the app's ACCENT - Office paints Word
//! blue, Excel green, `PowerPoint` orange - so the scope carries it: one of
//! five families ([`ShellThemeAccent`]: blue, leaf, plum, clay, slate, flora's
//! accent ramps), published to the app's own CSS as custom properties on the
//! scope's root (`var(--az-accent, #2F4A85)` and friends, each with its
//! night value under the dark mode), and readable in Rust through
//! [`ShellThemeAccent::colors`].
//!
//! The scope's root is a column that fills the window and paints the
//! theme's page ground and ink, so content that sets nothing inherits the
//! theme's - in both modes.
//!
//! Key types: [`ShellThemeScope`], [`ShellThemeAccent`], [`ShellThemeAccentColors`].

use alloc::vec::Vec;

use azul_core::dom::{Dom, IdOrClass, IdOrClassVec};
use azul_css::{
    css::{rule_priority, Css, CssCustomProperty, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector, ModeCondition,
    },
    props::{
        basic::{color::ColorU, pixel::PixelValue},
        layout::{
            LayoutDisplay, LayoutFlexDirection, LayoutHeight, LayoutMarginBottom, LayoutMarginLeft,
            LayoutMarginRight, LayoutMarginTop,
        },
        property::CssProperty,
    },
    AzString,
};

use super::{look_for, part, ShellLook, FILL_COLUMN_BASE};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The scope root's class.
pub const THEME_SCOPE_CLASS: &str = "__azul-native-theme-scope";

/// The accent family of an app: flora's five accent ramps
/// (`doc/templates/flora.css`, "accent"), echoing Office's per-app colours.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ShellThemeAccent {
    /// Writer, Mail, Calendar, Contacts, Files.
    #[default]
    Blue = 0,
    /// Spreadsheet, ERP, `ToDo`, Health.
    Leaf = 1,
    /// Vector, Notes, Chat / Meet.
    Plum = 2,
    /// Slides, Video editor, Music, Photo.
    Clay = 3,
    /// Terminal, Code, System monitor, Passwords.
    Slate = 4,
}

/// One accent family's colours in one mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShellThemeAccentColors {
    /// The accent itself: primary buttons, the selection.
    pub accent: ColorU,
    /// The deep tone: selected text in light mode, pressed faces.
    pub deep: ColorU,
    /// The soft wash: a selection's background, a hover tint.
    pub soft: ColorU,
    /// The glow: the focus ring.
    pub glow: ColorU,
    /// The ink on the accent.
    pub on_accent: ColorU,
}

const fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

/// The paper ink flora lays on every accent stone.
const ON_ACCENT: ColorU = rgb(0xF4, 0xF2, 0xEA);

impl ShellThemeAccent {
    /// Every family, in order.
    pub const ALL: [Self; 5] = [Self::Blue, Self::Leaf, Self::Plum, Self::Clay, Self::Slate];

    /// The family's name: the class suffix (`__azul-accent-<name>`) and
    /// the app's setting value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Leaf => "leaf",
            Self::Plum => "plum",
            Self::Clay => "clay",
            Self::Slate => "slate",
        }
    }

    /// The family a setting names; `None` for a name no family has.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The family's light-mode ramp: accent / deep / soft / glow.
    const fn light(self) -> ShellThemeAccentColors {
        let (accent, deep, soft, glow) = match self {
            Self::Blue => (rgb(0x2F, 0x4A, 0x85), rgb(0x1E, 0x32, 0x60), rgb(0xE0, 0xE4, 0xEE), rgb(0x7A, 0x93, 0xC6)),
            Self::Leaf => (rgb(0x44, 0x68, 0x4F), rgb(0x2F, 0x4C, 0x39), rgb(0xE1, 0xE6, 0xE1), rgb(0x7F, 0xA9, 0x8C)),
            Self::Plum => (rgb(0x57, 0x4A, 0x66), rgb(0x3E, 0x34, 0x4B), rgb(0xE5, 0xE1, 0xEA), rgb(0x8E, 0x80, 0xA2)),
            Self::Clay => (rgb(0x7E, 0x4A, 0x42), rgb(0x5E, 0x33, 0x2D), rgb(0xEA, 0xE0, 0xDD), rgb(0xB3, 0x83, 0x7A)),
            Self::Slate => (rgb(0x4A, 0x5C, 0x6B), rgb(0x35, 0x45, 0x51), rgb(0xDE, 0xE3, 0xE7), rgb(0x8A, 0xA0, 0xB0)),
        };
        ShellThemeAccentColors {
            accent,
            deep,
            soft,
            glow,
            on_accent: ON_ACCENT,
        }
    }

    /// The family's colours in the light or the dark mode. At night the
    /// accent keeps its stone; the deep tone lifts to the glow (ink on a dark
    /// ground), the soft wash sinks to the deep tone, the focus ring is the
    /// glow (flora.css: "only the focus ring lifts to --fl-glow").
    #[must_use]
    pub const fn colors(self, dark: bool) -> ShellThemeAccentColors {
        let light = self.light();
        if dark {
            ShellThemeAccentColors {
                accent: light.accent,
                deep: light.glow,
                soft: light.deep,
                glow: light.glow,
                on_accent: light.on_accent,
            }
        } else {
            light
        }
    }
}

/// The custom properties an accent publishes on the scope's root, in one
/// mode: `--az-accent`, `--az-accent-deep`, `--az-accent-soft`,
/// `--az-accent-glow`, `--az-on-accent`.
fn accent_declarations(colors: ShellThemeAccentColors) -> Vec<CssDeclaration> {
    [
        ("az-accent", colors.accent),
        ("az-accent-deep", colors.deep),
        ("az-accent-soft", colors.soft),
        ("az-accent-glow", colors.glow),
        ("az-on-accent", colors.on_accent),
    ]
    .into_iter()
    .map(|(name, color)| {
        CssDeclaration::CustomProperty(CssCustomProperty {
            name: AzString::from_const_str(name),
            value: AzString::from(color.to_hex()),
        })
    })
    .collect()
}

/// The scope's component sheet: the accent's custom properties, the light
/// values unconditioned and the night values under the dark mode.
#[must_use]
pub(crate) fn accent_sheet(accent: ShellThemeAccent) -> Css {
    let rule = |declarations: Vec<CssDeclaration>, conditions: Vec<DynamicSelector>| CssRuleBlock {
        path: CssPath {
            selectors: alloc::vec![CssPathSelector::Global].into(),
        },
        declarations: declarations.into(),
        conditions: conditions.into(),
        priority: rule_priority::AUTHOR,
    };
    Css {
        rules: alloc::vec![
            rule(accent_declarations(accent.colors(false)), Vec::new()),
            rule(
                accent_declarations(accent.colors(true)),
                alloc::vec![DynamicSelector::Mode(ModeCondition::Dark)]
            ),
        ]
        .into(),
        ..Css::default()
    }
}

/// The app's root: the theme's ground and an accent family around the
/// app's content.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellThemeScope {
    /// The app's content.
    pub content: Dom,
    /// The widget theme this scope is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// The accent family.
    pub accent: ShellThemeAccent,
}

impl ShellThemeScope {
    /// A scope around `content` in the blue accent, following the app theme.
    #[must_use]
    pub const fn create(content: Dom) -> Self {
        Self {
            content,
            theme: OptionUiTheme::None,
            accent: ShellThemeAccent::Blue,
        }
    }

    /// The accent family.
    pub const fn set_accent(&mut self, accent: ShellThemeAccent) {
        self.accent = accent;
    }

    /// [`Self::set_accent`] for the builder chain.
    #[must_use]
    pub const fn with_accent(mut self, accent: ShellThemeAccent) -> Self {
        self.set_accent(accent);
        self
    }

    /// Pin the widget theme; unset, the scope follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty scope and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The scope's DOM: one root around the content, carrying the theme
    /// marker, the accent class and the accent's custom properties.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }

    /// The scope as the window's `<body>`: the root an app's `layout()`
    /// returns. The body fills the window - no UA margin, the full height -
    /// and the scope grows in it, so the shells' panes take the window.
    #[must_use]
    pub fn body(self) -> Dom {
        Dom::create_body()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(WINDOW_BODY_BASE))
            .with_child(self.dom())
    }
}

/// The window's body around the scope: no UA margin (`body { margin: 8px }`),
/// the window's full height (the body's own height is its content's), a
/// column the scope's root grows in. Structure, the same in every theme.
static WINDOW_BODY_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_margin_top(LayoutMarginTop::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_right(LayoutMarginRight::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(
        0,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::Px(
        PixelValue::const_percent(100),
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
];

impl From<ShellThemeScope> for Dom {
    fn from(s: ShellThemeScope) -> Self {
        s.dom()
    }
}

/// The accent class: `__azul-accent-<name>`.
#[must_use]
pub fn accent_class(accent: ShellThemeAccent) -> AzString {
    AzString::from(alloc::format!("__azul-accent-{}", accent.name()))
}

/// The scope's DOM in `look`.
pub(crate) fn build(scope: ShellThemeScope, look: &ShellLook) -> Dom {
    let ShellThemeScope {
        content,
        theme,
        accent,
    } = scope;
    let mut classes: Vec<IdOrClass> =
        alloc::vec![IdOrClass::Class(AzString::from_const_str(THEME_SCOPE_CLASS))];
    if let Some(marker) = look.marker {
        classes.push(IdOrClass::Class(AzString::from_const_str(marker)));
    }
    classes.push(IdOrClass::Class(accent_class(accent)));
    let mut root = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(FILL_COLUMN_BASE, &look.scope_root));
    // Flora's wool-on-parchment scrollbars for every scroll box in the app:
    // a `@theme(flora)` sheet (inert in every other theme) under a scope that
    // follows the app theme, the bare rules under one pinned to flora, none
    // under one pinned to flat - a pinned scope carries no theme's blocks.
    match theme.into_option() {
        None => {
            root = root.with_component_css(crate::widgets::themes::flora::scrollbar_sheet(false));
        }
        Some(UiTheme::Flora) => {
            root = root.with_component_css(crate::widgets::themes::flora::scrollbar_sheet(true));
        }
        Some(UiTheme::Flat) => {}
    }
    // The accent, the last sheet.
    root.with_component_css(accent_sheet(accent)).with_child(content)
}

#[cfg(test)]
mod theme_scope_tests {
    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    #[test]
    fn every_accent_family_has_a_name_that_round_trips_and_five_colours() {
        for accent in ShellThemeAccent::ALL {
            assert_eq!(ShellThemeAccent::from_name(accent.name()), Some(accent));
            let light = accent.colors(false);
            let dark = accent.colors(true);
            assert_eq!(light.accent, dark.accent, "{accent:?}: the stone stays at night");
            assert_eq!(dark.glow, light.glow);
            assert_ne!(light.deep, dark.deep, "{accent:?}: the deep tone lifts at night");
        }
        assert_eq!(ShellThemeAccent::from_name("teal"), None);
        assert_eq!(ShellThemeAccent::Blue.colors(false).accent, ColorU { r: 0x2F, g: 0x4A, b: 0x85, a: 255 });
    }

    #[test]
    fn the_scope_publishes_the_accent_as_custom_properties_with_night_values() {
        let dom = ShellThemeScope::create(slot())
            .with_accent(ShellThemeAccent::Leaf)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(tc::has_class(&dom, THEME_SCOPE_CLASS));
        assert!(tc::has_class(&dom, "__azul-accent-leaf"));
        let sheet = dom.css.as_ref().last().expect("the accent sheet");
        let rules = sheet.rules.as_ref();
        assert_eq!(rules.len(), 2, "light, then dark");
        assert!(rules[0].conditions.as_ref().is_empty());
        assert_eq!(
            rules[1].conditions.as_ref(),
            &[DynamicSelector::Mode(ModeCondition::Dark)]
        );
        let names: Vec<String> = rules[0]
            .declarations
            .as_ref()
            .iter()
            .filter_map(|d| match d {
                CssDeclaration::CustomProperty(c) => Some(String::from(c.name.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            vec!["az-accent", "az-accent-deep", "az-accent-soft", "az-accent-glow", "az-on-accent"]
        );
        let first = match &rules[0].declarations.as_ref()[0] {
            CssDeclaration::CustomProperty(c) => c.value.as_str().to_string(),
            other => panic!("{other:?}"),
        };
        assert_eq!(first, "#44684f");
        assert_eq!(dom.children.as_ref().len(), 1, "the content, as it is");
    }

    #[test]
    fn the_scope_as_the_windows_body_drops_the_ua_margin_and_takes_the_full_height() {
        use azul_core::dom::NodeType;
        use azul_css::props::{
            basic::pixel::PixelValue,
            layout::{
                LayoutDisplay, LayoutFlexDirection, LayoutHeight, LayoutMarginBottom,
                LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop,
            },
            property::{CssProperty, CssPropertyType},
        };
        let body = ShellThemeScope::create(slot()).with_theme(UiTheme::Flat).body();
        assert!(matches!(body.root.get_node_type(), NodeType::Body));
        for wanted in [
            CssProperty::const_margin_top(LayoutMarginTop::const_px(0)),
            CssProperty::const_margin_right(LayoutMarginRight::const_px(0)),
            CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(0)),
            CssProperty::const_margin_left(LayoutMarginLeft::const_px(0)),
            CssProperty::const_height(LayoutHeight::Px(PixelValue::const_percent(100))),
            CssProperty::const_display(LayoutDisplay::Flex),
            CssProperty::const_flex_direction(LayoutFlexDirection::Column),
        ] {
            let ty: CssPropertyType = wanted.get_type();
            for dark in [false, true] {
                assert_eq!(
                    tc::resolve(&body, ty, dark, None).as_ref(),
                    Some(&wanted),
                    "the body's {ty:?} (dark {dark})"
                );
            }
        }
        let kids = body.children.as_ref();
        assert_eq!(kids.len(), 1, "the scope, alone");
        assert!(tc::has_class(&kids[0], THEME_SCOPE_CLASS));
    }

    #[test]
    fn a_scope_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "theme_scope",
            || ShellThemeScope::create(slot()).dom(),
            |t: UiTheme| ShellThemeScope::create(slot()).with_theme(t).dom(),
        );
    }
}
