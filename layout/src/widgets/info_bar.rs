//! Info bar widget - the notice strip across the top of a mail's reading
//! pane: a glyph, one line of text and an ACTION at its end. Outlook's blue
//! "i" bar ("Click here to download pictures. To help protect your privacy,
//! Outlook prevented automatic download of some pictures in this message."),
//! its yellow "sent with high importance" strip, its red "the sender could
//! not be verified" strip.
//!
//! The strip borrows the [`Alert`](crate::widgets::alert::Alert) widget's
//! kind palette ([`AlertKind`]: info, success, warning, danger) but it is
//! not an alert that closes: it stays until the app rebuilds without it, and
//! its one affordance is the action - a link button that reports through
//! [`InfoBar::on_action`] (the same click shape as a [`Button`]'s, so an app
//! hands it the one handler it already has). For assistive technology the
//! strip is an alert named by its text; the action is a button named by its
//! label, and the only keyboard stop.
//!
//! Key types: [`InfoBar`], [`AlertKind`].

use alloc::vec::Vec;

use azul_core::{
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString,
};

use crate::widgets::{
    alert::AlertKind,
    button::{Button, ButtonOnClick, ButtonOnClickCallback, ButtonType, OptionButtonOnClick},
};

static BAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-info-bar"))];
static BAR_ICON_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-info-bar-icon"))];
static BAR_TEXT_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-info-bar-text"))];
static BAR_ACTION_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-info-bar-action",
))];

/// A notice strip: a glyph, a line of text and an action link.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InfoBar {
    /// The glyph at the start (a `Dom::create_icon` name: "info",
    /// "warning"), or empty for none.
    pub icon: AzString,
    /// The notice.
    pub text: AzString,
    /// The action link's label ("Download pictures"), or empty for no
    /// action.
    pub action: AzString,
    /// Fires when the action is clicked. A [`Button`]'s click, so the app
    /// can hand over any button handler it has.
    pub on_action: OptionButtonOnClick,
    /// The strip's colour: the [`Alert`](crate::widgets::alert::Alert)
    /// palette (info by default).
    pub kind: AlertKind,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about an info bar: the SKIN of each part, laid over
/// the part's base (the strip's structure, the same in every theme:
/// `INFO_BAR_*_BASE`) by [`build`].
pub(crate) struct InfoBarLook {
    /// The strip for a kind (its face, rule and ink, dark twins included).
    pub strip: fn(AlertKind) -> Vec<CssPropertyWithConditions>,
    /// The glyph.
    pub icon: Vec<CssPropertyWithConditions>,
    /// The text line.
    pub text: Vec<CssPropertyWithConditions>,
    /// The box around the action link.
    pub action: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the strip, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the strip's structure, in every theme ----

/// The strip: one row across its column, its parts on one midline, whose
/// text a drag never selects.
pub(crate) static INFO_BAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The glyph and the action keep their size.
pub(crate) static INFO_BAR_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The text takes the rest of the strip and may wrap.
pub(crate) static INFO_BAR_TEXT_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

impl InfoBar {
    /// An info strip saying `text`, with no glyph and no action.
    #[must_use]
    pub fn create(text: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            text,
            action: AzString::from_const_str(""),
            on_action: None.into(),
            kind: AlertKind::Info,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The glyph at the start (a `Dom::create_icon` name).
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// The action link's label.
    pub fn set_action(&mut self, action: AzString) {
        self.action = action;
    }

    /// [`Self::set_action`] for the builder chain.
    #[must_use]
    pub fn with_action(mut self, action: AzString) -> Self {
        self.set_action(action);
        self
    }

    /// The strip's colour (the alert palette).
    pub const fn set_kind(&mut self, kind: AlertKind) {
        self.kind = kind;
    }

    /// [`Self::set_kind`] for the builder chain.
    #[must_use]
    pub const fn with_kind(mut self, kind: AlertKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// The action's click.
    pub fn set_on_action<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_action = Some(ButtonOnClick {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_action`] for the builder chain.
    #[must_use]
    pub fn with_on_action<C: Into<ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_action(data, callback);
        self
    }

    /// Pin the widget theme; unset, the strip follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty strip and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The strip's DOM. The look comes from the theme module
    /// (`themes::flat::info_bar` / `themes::flora::info_bar`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::info_bar(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::info_bar(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::info_bar,
                crate::widgets::themes::flora::info_bar,
            ),
        }
    }
}

impl Default for InfoBar {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<InfoBar> for Dom {
    fn from(b: InfoBar) -> Self {
        b.dom()
    }
}

impl_option!(
    InfoBar,
    OptionInfoBar,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The strip's DOM in `look`: strip [glyph?, text, action?]. Every part is
/// its base (the structure), then the look's skin; the action is a link
/// [`Button`] pinned to the strip's theme.
pub(crate) fn build(bar: InfoBar, look: &InfoBarLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let InfoBar {
        icon,
        text,
        action,
        on_action,
        kind,
        theme,
    } = bar;

    let mut children: Vec<Dom> = Vec::with_capacity(3);
    if !icon.as_str().is_empty() {
        children.push(
            Dom::create_icon(icon)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(BAR_ICON_CLASS))
                .with_css_props(part(INFO_BAR_FIXED_BASE, &look.icon)),
        );
    }
    children.push(
        crate::widgets::widget_p_with_text(text.clone())
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BAR_TEXT_CLASS))
            .with_css_props(part(INFO_BAR_TEXT_BASE, &look.text)),
    );
    if !action.as_str().is_empty() {
        let mut link = Button::with_type(action, ButtonType::Link);
        link.on_click = on_action;
        if let Some(theme) = theme.into_option() {
            link = link.with_theme(theme);
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(BAR_ACTION_CLASS))
                .with_css_props(part(INFO_BAR_FIXED_BASE, &look.action))
                .with_child(link.dom()),
        );
    }

    let mut classes: Vec<IdOrClass> = BAR_CLASS.to_vec();
    classes.push(Class(AzString::from_const_str(kind.class_name())));
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(INFO_BAR_BASE, &(look.strip)(kind)))
        // A notice: an ALERT named by what it says. It takes no focus; the
        // action does.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Alert,
            accessibility_name: Some(text).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod info_bar_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        callbacks::Update,
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::{
        callbacks::CallbackInfo,
        widgets::{
            button::ButtonOnClickCallbackType,
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks, UiTheme},
        },
    };

    const NOTICE: &str = "Click here to download pictures.";

    fn pictures() -> InfoBar {
        InfoBar::create(AzString::from(NOTICE))
            .with_icon(AzString::from("info"))
            .with_action(AzString::from("Download pictures"))
    }

    /// The text of a `p > text` label.
    fn text_of(node: &Dom) -> Option<&str> {
        match node.children.as_ref() {
            [only] => match only.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_ref().as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    #[test]
    fn the_strip_is_the_glyph_the_text_and_the_action() {
        for theme in checks::BOTH {
            let dom = pictures().with_theme(theme).dom();
            assert!(
                theme_checks::has_class(&dom, "__azul-native-info-bar"),
                "{}",
                theme.name()
            );
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}: glyph, text, action", theme.name());
            assert!(matches!(parts[0].root.get_node_type(), NodeType::Icon(_)));
            assert!(theme_checks::has_class(
                &parts[1],
                "__azul-native-info-bar-text"
            ));
            assert_eq!(text_of(&parts[1]), Some(NOTICE));
            assert!(theme_checks::has_class(
                &parts[2],
                "__azul-native-info-bar-action"
            ));
            let mut found = Vec::new();
            texts(&parts[2], &mut found);
            assert!(
                found.iter().any(|t| t == "Download pictures"),
                "{}: the action is a link with its label: {found:?}",
                theme.name()
            );
        }
        let bare = InfoBar::create(AzString::from(NOTICE))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            bare.children.as_ref().len(),
            1,
            "no glyph and no action: the text alone"
        );
    }

    #[test]
    fn the_strip_is_an_alert_named_by_its_text_and_only_the_action_takes_focus() {
        let dom = pictures().with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Alert);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some(NOTICE)
        );
        assert!(dom.root.get_tab_index().is_none(), "the strip takes no focus");
        let stops = theme_checks::focusable(&dom);
        assert_eq!(stops.len(), 1, "one stop, the action: {stops:?}");
        assert_eq!(
            stops[0]
                .1
                .root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string())),
            Some("Download pictures".to_string()),
            "the action is named by its label"
        );
    }

    #[test]
    fn each_kind_paints_its_own_strip() {
        let strip = |kind: AlertKind| {
            format!(
                "{:?}",
                pictures()
                    .with_kind(kind)
                    .with_theme(UiTheme::Flat)
                    .dom()
                    .root
                    .get_style()
            )
        };
        let kinds = [
            AlertKind::Info,
            AlertKind::Success,
            AlertKind::Warning,
            AlertKind::Danger,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(strip(*a), strip(*b), "{a:?} and {b:?} look alike");
            }
            assert!(
                theme_checks::has_class(
                    &pictures().with_kind(*a).with_theme(UiTheme::Flat).dom(),
                    a.class_name()
                ),
                "{a:?} carries its kind class"
            );
        }
    }

    type Log = Arc<Mutex<usize>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            *log.lock().expect("log") += 1;
        }
        Update::RefreshDom
    }

    #[test]
    fn the_action_reports_its_click() {
        let log: Log = Arc::new(Mutex::new(0));
        let dom = pictures()
            .with_on_action(RefAny::new(log.clone()), record as ButtonOnClickCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom);
        // The action's button is the one keyboard stop of the tree.
        let button = styled
            .node_data
            .as_ref()
            .iter()
            .position(|n| n.get_tab_index().is_some())
            .expect("the action button");
        let target = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(button))),
        };
        let (update, _) = rv::fire(&styled, target, EventFilter::Hover(HoverEventFilter::Click))
            .expect("the action takes the click");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert_eq!(*log.lock().expect("log"), 1);
    }

    #[test]
    fn a_strip_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        checks::assert_follows_the_app_theme(
            "info_bar",
            || pictures().dom(),
            |t: UiTheme| pictures().with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || pictures().dom());
            theme_checks::assert_structure_is_shared(
                &format!("info_bar built for {}", theme.name()),
                &dom,
                &[],
            );
        }
    }
}
