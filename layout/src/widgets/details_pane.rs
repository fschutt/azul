//! Details pane widget - the strip at the bottom of a file manager that
//! describes the selected item: a big icon, the item's name and kind, and
//! its properties as "key: value" rows ("Space used: 132 GB", "Total size:
//! 456 GB", "File system: NTFS").
//!
//! The pane shows what it is given: the app decides what the selected item
//! is and rebuilds. It takes no focus and reports nothing; for assistive
//! technology it is a group named after the item.
//!
//! Key types: [`DetailsPane`].

use alloc::vec::Vec;

use azul_core::{
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    window::StringPairVec,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth,
        },
        property::CssProperty,
        style::{StyleTextAlign, StyleUserSelect},
    },
    AzString,
};

static PANE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-details-pane"))];
static PANE_ICON_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-icon",
))];
static PANE_HEADING_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-heading",
))];
static PANE_TITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-title",
))];
static PANE_SUBTITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-subtitle",
))];
static PANE_PROPERTIES_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-properties",
))];
static PANE_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-row",
))];
static PANE_KEY_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-key",
))];
static PANE_VALUE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-details-pane-value",
))];

/// The details of the selected item: an icon, a title, a subtitle and
/// key / value rows.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DetailsPane {
    /// The big icon (a `Dom::create_icon` name), or empty for none.
    pub icon: AzString,
    /// The item's name.
    pub title: AzString,
    /// What the item is ("Local Disk", "S3 bucket", "Folder"); empty for
    /// none.
    pub subtitle: AzString,
    /// The properties, in order: each key is written with a colon after it.
    pub properties: StringPairVec,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about a details pane: the SKIN of each part, laid
/// over the part's base (the pane's structure, the same in every theme:
/// `PANE_*_BASE`) by [`build`].
pub(crate) struct DetailsPaneLook {
    /// The pane.
    pub pane: Vec<CssPropertyWithConditions>,
    /// The big icon.
    pub icon: Vec<CssPropertyWithConditions>,
    /// The column of title and subtitle.
    pub heading: Vec<CssPropertyWithConditions>,
    /// The title.
    pub title: Vec<CssPropertyWithConditions>,
    /// The subtitle.
    pub subtitle: Vec<CssPropertyWithConditions>,
    /// The column of property rows.
    pub properties: Vec<CssPropertyWithConditions>,
    /// One property row.
    pub row: Vec<CssPropertyWithConditions>,
    /// A property's key.
    pub key: Vec<CssPropertyWithConditions>,
    /// A property's value.
    pub value: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the pane, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the pane's structure, in every theme ----

/// The pane: a row of the icon, the heading and the properties, centred on
/// one midline, whose text a drag never selects.
pub(crate) static PANE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The icon keeps its size.
pub(crate) static PANE_ICON_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A column of lines that may shrink below its text.
pub(crate) static PANE_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A property row: the key, then the value, on one line.
pub(crate) static PANE_ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// A key: set from the right, so the values line up.
pub(crate) static PANE_KEY_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // the key keeps its room; the value is the part that gives way
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Right)),
];

/// A value: the rest of the row.
pub(crate) static PANE_VALUE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

impl DetailsPane {
    /// A pane for `title`, with no icon, subtitle or properties.
    #[must_use]
    pub const fn create(title: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            title,
            subtitle: AzString::from_const_str(""),
            properties: StringPairVec::from_const_slice(&[]),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The big icon (a `Dom::create_icon` name).
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// What the item is.
    pub fn set_subtitle(&mut self, subtitle: AzString) {
        self.subtitle = subtitle;
    }

    /// [`Self::set_subtitle`] for the builder chain.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: AzString) -> Self {
        self.set_subtitle(subtitle);
        self
    }

    /// The property rows, in order.
    pub fn set_properties(&mut self, properties: StringPairVec) {
        self.properties = properties;
    }

    /// [`Self::set_properties`] for the builder chain.
    #[must_use]
    pub fn with_properties(mut self, properties: StringPairVec) -> Self {
        self.set_properties(properties);
        self
    }

    /// Adds one property row.
    pub fn add_property(&mut self, key: AzString, value: AzString) {
        self.properties
            .push(azul_core::window::AzStringPair::create(key, value));
    }

    /// [`Self::add_property`] for the builder chain.
    #[must_use]
    pub fn with_property(mut self, key: AzString, value: AzString) -> Self {
        self.add_property(key, value);
        self
    }

    /// Pin the widget theme; unset, the pane follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty pane and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The pane's DOM. The look comes from the theme module
    /// (`themes::flat::details_pane` / `themes::flora::details_pane`);
    /// `None` carries both looks, each in its `@theme(<name>)` block, and the
    /// app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::details_pane(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::details_pane(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::details_pane,
                crate::widgets::themes::flora::details_pane,
            ),
        }
    }
}

impl Default for DetailsPane {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<DetailsPane> for Dom {
    fn from(p: DetailsPane) -> Self {
        p.dom()
    }
}

/// The pane's DOM in `look`: pane [icon, heading [title, subtitle],
/// properties [row [key, value], ...]]. Every part is its base (the
/// structure), then the look's skin.
pub(crate) fn build(pane: DetailsPane, look: &DetailsPaneLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let label = |text: AzString, class: &'static [IdOrClass], skin: &[CssPropertyWithConditions]| {
        crate::widgets::widget_p_with_text(text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
            .with_css_props(part(&[], skin))
    };
    let DetailsPane {
        icon,
        title,
        subtitle,
        properties,
        theme: _,
    } = pane;

    let icon_node = if icon.as_str().is_empty() {
        Dom::create_div()
    } else {
        Dom::create_icon(icon)
    }
    .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_ICON_CLASS))
    .with_css_props(part(PANE_ICON_BASE, &look.icon));

    let mut heading = Vec::with_capacity(2);
    heading.push(label(title.clone(), PANE_TITLE_CLASS, &look.title));
    if !subtitle.as_str().is_empty() {
        heading.push(label(subtitle, PANE_SUBTITLE_CLASS, &look.subtitle));
    }
    let heading = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_HEADING_CLASS))
        .with_css_props(part(PANE_COLUMN_BASE, &look.heading))
        .with_children(DomVec::from_vec(heading));

    let rows: Vec<Dom> = properties
        .as_ref()
        .iter()
        .map(|pair| {
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_ROW_CLASS))
                .with_css_props(part(PANE_ROW_BASE, &look.row))
                .with_children(DomVec::from_vec(alloc::vec![
                    crate::widgets::widget_p_with_text(AzString::from(alloc::format!(
                        "{}:",
                        pair.key.as_str()
                    )))
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_KEY_CLASS))
                    .with_css_props(part(PANE_KEY_BASE, &look.key)),
                    crate::widgets::widget_p_with_text(pair.value.clone())
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_VALUE_CLASS))
                        .with_css_props(part(PANE_VALUE_BASE, &look.value)),
                ]))
        })
        .collect();
    let properties = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(PANE_PROPERTIES_CLASS))
        .with_css_props(part(PANE_COLUMN_BASE, &look.properties))
        .with_children(DomVec::from_vec(rows));

    let mut classes: Vec<IdOrClass> = PANE_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(PANE_BASE, &look.pane))
        // A GROUP named after the item it describes.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Grouping,
            accessibility_name: Some(title).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(alloc::vec![icon_node, heading, properties]))
}

#[cfg(test)]
mod details_pane_tests {
    use azul_core::dom::NodeType;

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    fn text_of(node: &Dom) -> Option<&str> {
        match node.children.as_ref() {
            [only] => match only.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_ref().as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    fn home() -> DetailsPane {
        DetailsPane::create(AzString::from("Home"))
            .with_icon(AzString::from("home"))
            .with_subtitle(AzString::from("Local Disk"))
            .with_property(AzString::from("Space used"), AzString::from("132 GB"))
            .with_property(AzString::from("Total size"), AzString::from("456 GB"))
    }

    #[test]
    fn the_pane_is_the_icon_the_heading_and_the_properties() {
        for theme in checks::BOTH {
            let dom = home().with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}", theme.name());
            assert!(has_class(&parts[0], "__azul-native-details-pane-icon"));
            assert!(matches!(parts[0].root.get_node_type(), NodeType::Icon(_)));
            let heading = parts[1].children.as_ref();
            assert_eq!(text_of(&heading[0]), Some("Home"), "{}", theme.name());
            assert_eq!(text_of(&heading[1]), Some("Local Disk"), "{}", theme.name());
            let rows = parts[2].children.as_ref();
            assert_eq!(rows.len(), 2, "{}: one row per property", theme.name());
            let cells = rows[0].children.as_ref();
            assert_eq!(text_of(&cells[0]), Some("Space used:"), "the key ends in a colon");
            assert_eq!(text_of(&cells[1]), Some("132 GB"));
            assert_eq!(text_of(&rows[1].children.as_ref()[1]), Some("456 GB"));
        }
    }

    #[test]
    fn a_pane_without_a_subtitle_or_properties_shows_only_its_title() {
        let dom = DetailsPane::create(AzString::from("notes.txt"))
            .with_theme(UiTheme::Flat)
            .dom();
        let parts = dom.children.as_ref();
        assert!(
            matches!(parts[0].root.get_node_type(), NodeType::Div),
            "no icon: an empty slot"
        );
        assert_eq!(parts[1].children.as_ref().len(), 1, "the title alone");
        assert!(parts[2].children.as_ref().is_empty(), "no rows");
    }

    #[test]
    fn the_pane_is_a_group_named_after_its_item() {
        let dom = home().with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Grouping);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("Home")
        );
        assert!(dom.root.get_tab_index().is_none(), "a pane takes no focus");
    }

    #[test]
    fn a_pane_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "details_pane",
            || home().dom(),
            |t: UiTheme| home().with_theme(t).dom(),
        );
    }
}
