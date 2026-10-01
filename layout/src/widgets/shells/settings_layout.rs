//! ShellSettingsLayout - the settings window of every app, the same in every
//! app (04-app-shells.md, "Settings"): a category list on the left, form
//! sections on the right, a search field on top.
//!
//! ```text
//! ┌ 🔍 Search settings ───────────────────────────────────┐
//! ├──────────────┬────────────────────────────────────────┤
//! │ General      │ STARTUP                                │
//! │ Accounts   ◂ │  [x] Open the last folder              │
//! │ Appearance   │                                        │
//! │ Advanced     │ LANGUAGE                               │
//! │              │  Language  [ English ▾ ]               │
//! └──────────────┴────────────────────────────────────────┘
//! ```
//!
//! The categories are a tab list (one Tab stop; Up / Down / Home / End
//! move, Enter or a click chooses) and the sections are the app's `Dom`s
//! under a title each. The layout owns no state: a choice reports the
//! category ([`ShellSettingsLayout::on_category`]), typing reports the search
//! text ([`ShellSettingsLayout::on_search`]), and the app rebuilds - with the
//! sections of the chosen category, and the search text handed back
//! through [`ShellSettingsLayout::with_search`]. A non-empty search hides the
//! sections whose title does not match it ([`palette_matches`]) and whose
//! keywords ([`ShellSettingsSection::keywords`], the searchable text of its
//! settings) do not contain it - so a search finds a setting, not only a
//! heading. A category may carry an icon and a badge (the number of matches
//! while searching), and a footer (Apply / OK / Cancel, "Restore defaults")
//! sits under the body.
//!
//! Key types: [`ShellSettingsLayout`], [`ShellSettingsSection`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, NodeType, OptionDom},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    AzString, StringVec,
};

use super::{
    command_palette::palette_matches, inner_theme, look_for, part, root_classes, stack_state,
    state_classes, text, ShellLook, CHROME_ROW_BASE, COLUMN_BASE, FILL_COLUMN_BASE,
    GROW_LABEL_BASE, GROW_ROW_BASE, ITEM_BASE, LABEL_BASE, RAIL_BASE, SCROLL_COLUMN_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        roving,
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType, TextInputState,
            TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The layout's class.
pub const SETTINGS_CLASS: &str = "__azul-native-settings-layout";
/// The search row's class.
pub const SEARCH_CLASS: &str = "__azul-native-settings-layout-search";
/// The body's class (categories beside sections).
pub const BODY_CLASS: &str = "__azul-native-settings-layout-body";
/// The category list's class.
pub const CATEGORIES_CLASS: &str = "__azul-native-settings-layout-categories";
/// A category's class.
pub const CATEGORY_CLASS: &str = "__azul-native-settings-layout-category";
/// Added to the active category.
pub const CATEGORY_ACTIVE_CLASS: &str = "__azul-native-settings-layout-category-active";
/// The sections column's class.
pub const SECTIONS_CLASS: &str = "__azul-native-settings-layout-sections";
/// A section's class.
pub const SECTION_CLASS: &str = "__azul-native-settings-layout-section";
/// A section title's class.
pub const SECTION_TITLE_CLASS: &str = "__azul-native-settings-layout-section-title";
/// The class of a category's icon.
pub const CATEGORY_ICON_CLASS: &str = "__azul-native-settings-layout-category-icon";
/// The class of a category's badge.
pub const CATEGORY_BADGE_CLASS: &str = "__azul-native-settings-layout-category-badge";
/// The footer's class (the row under the body).
pub const FOOTER_CLASS: &str = "__azul-native-settings-layout-footer";

/// Callback invoked when a category is chosen: its index.
pub type ShellSettingsLayoutOnCategoryCallbackType =
    extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    ShellSettingsLayoutOnCategory,
    OptionShellSettingsLayoutOnCategory,
    ShellSettingsLayoutOnCategoryCallback,
    ShellSettingsLayoutOnCategoryCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellSettingsLayoutOnCategoryCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_SETTINGS_LAYOUT_ON_CATEGORY_INVOKER,
    invoker_ty:     AzShellSettingsLayoutOnCategoryCallbackInvoker,
    thunk_fn:       az_shell_settings_layout_on_category_callback_thunk,
    setter_fn:      AzApp_setShellSettingsLayoutOnCategoryCallbackInvoker,
    from_handle_fn: AzShellSettingsLayoutOnCategoryCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellSettingsLayoutOnCategoryCallback_createFromHostHandleByref,
    extra_args:     [ category_index: usize ],
}

/// Callback invoked when the search text changed.
pub type ShellSettingsLayoutOnSearchCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AzString) -> Update;
impl_widget_callback!(
    ShellSettingsLayoutOnSearch,
    OptionShellSettingsLayoutOnSearch,
    ShellSettingsLayoutOnSearchCallback,
    ShellSettingsLayoutOnSearchCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellSettingsLayoutOnSearchCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_SETTINGS_LAYOUT_ON_SEARCH_INVOKER,
    invoker_ty:     AzShellSettingsLayoutOnSearchCallbackInvoker,
    thunk_fn:       az_shell_settings_layout_on_search_callback_thunk,
    setter_fn:      AzApp_setShellSettingsLayoutOnSearchCallbackInvoker,
    from_handle_fn: AzShellSettingsLayoutOnSearchCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellSettingsLayoutOnSearchCallback_createFromHostHandleByref,
    extra_args:     [ query: AzString ],
}

/// One form section: a title over the app's content.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSettingsSection {
    /// The app's form content.
    pub content: Dom,
    /// The section's title ("Startup").
    pub title: AzString,
    /// The searchable text of the section's settings (their labels, help
    /// and keywords): a search that finds it keeps the section.
    pub keywords: AzString,
}

impl_option!(
    ShellSettingsSection,
    OptionShellSettingsSection,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellSettingsSection,
    ShellSettingsSectionVec,
    ShellSettingsSectionVecDestructor,
    ShellSettingsSectionVecDestructorType,
    ShellSettingsSectionVecSlice,
    OptionShellSettingsSection
);
impl_vec_clone!(ShellSettingsSection, ShellSettingsSectionVec, ShellSettingsSectionVecDestructor);
impl_vec_debug!(ShellSettingsSection, ShellSettingsSectionVec);
impl_vec_partialeq!(ShellSettingsSection, ShellSettingsSectionVec);
impl_vec_mut!(ShellSettingsSection, ShellSettingsSectionVec);

impl ShellSettingsSection {
    /// A section `title` over `content`.
    #[must_use]
    pub fn create(title: AzString, content: Dom) -> Self {
        Self {
            content,
            title,
            keywords: AzString::from_const_str(""),
        }
    }

    /// The searchable text of the section's settings.
    pub fn set_keywords(&mut self, keywords: AzString) {
        self.keywords = keywords;
    }

    /// [`Self::set_keywords`] for the builder chain.
    #[must_use]
    pub fn with_keywords(mut self, keywords: AzString) -> Self {
        self.set_keywords(keywords);
        self
    }
}

/// The settings layout: categories, sections, search.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ShellSettingsLayout {
    /// The categories, top to bottom.
    pub categories: StringVec,
    /// The sections of the active category, top to bottom.
    pub sections: ShellSettingsSectionVec,
    /// The search text.
    pub search: AzString,
    /// The search field's placeholder.
    pub search_placeholder: AzString,
    /// A category was chosen.
    pub on_category: OptionShellSettingsLayoutOnCategory,
    /// The search text changed.
    pub on_search: OptionShellSettingsLayoutOnSearch,
    /// The categories' icons (`Dom::create_icon` names), parallel to
    /// `categories`; empty (or an empty name) for none.
    pub category_icons: StringVec,
    /// The categories' badges ("3"), parallel to `categories`; empty (or an
    /// empty text) for none.
    pub category_badges: StringVec,
    /// The row under the body (Apply / OK / Cancel), or `None`.
    pub footer: OptionDom,
    /// The active category.
    pub active_category: usize,
    /// The widget theme this layout is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl ShellSettingsLayout {
    /// A layout with `categories`, the first active, no sections.
    #[must_use]
    pub fn create(categories: StringVec) -> Self {
        Self {
            categories,
            sections: ShellSettingsSectionVec::from_const_slice(&[]),
            search: AzString::from_const_str(""),
            search_placeholder: AzString::from_const_str("Search settings"),
            on_category: None.into(),
            on_search: None.into(),
            category_icons: StringVec::from_const_slice(&[]),
            category_badges: StringVec::from_const_slice(&[]),
            footer: OptionDom::None,
            active_category: 0,
            theme: OptionUiTheme::None,
        }
    }

    /// The categories' icons, parallel to the categories.
    pub fn set_category_icons(&mut self, icons: StringVec) {
        self.category_icons = icons;
    }

    /// [`Self::set_category_icons`] for the builder chain.
    #[must_use]
    pub fn with_category_icons(mut self, icons: StringVec) -> Self {
        self.set_category_icons(icons);
        self
    }

    /// The categories' badges, parallel to the categories.
    pub fn set_category_badges(&mut self, badges: StringVec) {
        self.category_badges = badges;
    }

    /// [`Self::set_category_badges`] for the builder chain.
    #[must_use]
    pub fn with_category_badges(mut self, badges: StringVec) -> Self {
        self.set_category_badges(badges);
        self
    }

    /// The row under the body.
    pub fn set_footer(&mut self, footer: Dom) {
        self.footer = OptionDom::Some(footer);
    }

    /// [`Self::set_footer`] for the builder chain.
    #[must_use]
    pub fn with_footer(mut self, footer: Dom) -> Self {
        self.set_footer(footer);
        self
    }

    /// Appends a section.
    pub fn add_section(&mut self, section: ShellSettingsSection) {
        let mut v = self.sections.clone().into_library_owned_vec();
        v.push(section);
        self.sections = ShellSettingsSectionVec::from_vec(v);
    }

    /// [`Self::add_section`] for the builder chain.
    #[must_use]
    pub fn with_section(mut self, section: ShellSettingsSection) -> Self {
        self.add_section(section);
        self
    }

    /// Replaces the sections.
    pub fn set_sections(&mut self, sections: ShellSettingsSectionVec) {
        self.sections = sections;
    }

    /// [`Self::set_sections`] for the builder chain.
    #[must_use]
    pub fn with_sections(mut self, sections: ShellSettingsSectionVec) -> Self {
        self.set_sections(sections);
        self
    }

    /// The search text.
    pub fn set_search(&mut self, search: AzString) {
        self.search = search;
    }

    /// [`Self::set_search`] for the builder chain.
    #[must_use]
    pub fn with_search(mut self, search: AzString) -> Self {
        self.set_search(search);
        self
    }

    /// The search field's placeholder.
    pub fn set_search_placeholder(&mut self, placeholder: AzString) {
        self.search_placeholder = placeholder;
    }

    /// [`Self::set_search_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_search_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_search_placeholder(placeholder);
        self
    }

    /// The active category.
    pub const fn set_active_category(&mut self, index: usize) {
        self.active_category = index;
    }

    /// [`Self::set_active_category`] for the builder chain.
    #[must_use]
    pub const fn with_active_category(mut self, index: usize) -> Self {
        self.set_active_category(index);
        self
    }

    /// A category was chosen.
    pub fn set_on_category<C: Into<ShellSettingsLayoutOnCategoryCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_category = Some(ShellSettingsLayoutOnCategory {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_category`] for the builder chain.
    #[must_use]
    pub fn with_on_category<C: Into<ShellSettingsLayoutOnCategoryCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_category(data, callback);
        self
    }

    /// The search text changed.
    pub fn set_on_search<C: Into<ShellSettingsLayoutOnSearchCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_search = Some(ShellSettingsLayoutOnSearch {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_search`] for the builder chain.
    #[must_use]
    pub fn with_on_search<C: Into<ShellSettingsLayoutOnSearchCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_search(data, callback);
        self
    }

    /// Pin the widget theme; unset, the layout follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty layout and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The layout's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ShellSettingsLayout {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

impl From<ShellSettingsLayout> for Dom {
    fn from(s: ShellSettingsLayout) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

struct CategoryRef {
    on_category: OptionShellSettingsLayoutOnCategory,
    index: usize,
}

struct SearchRef {
    on_search: OptionShellSettingsLayoutOnSearch,
}

extern "C" fn on_category_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(c) = data.downcast_ref::<CategoryRef>() else {
        return Update::DoNothing;
    };
    match c.on_category.as_ref() {
        Some(ShellSettingsLayoutOnCategory { callback, refany }) => {
            callback.invoke(refany.clone(), info, c.index)
        }
        None => Update::DoNothing,
    }
}

/// Up / Down / Home / End move the stop between the categories.
extern "C" fn on_category_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let step = match key {
        VirtualKeyCode::Up => roving::Step::Previous,
        VirtualKeyCode::Down => roving::Step::Next,
        VirtualKeyCode::Home => roving::Step::First,
        VirtualKeyCode::End => roving::Step::Last,
        _ => return Update::DoNothing,
    };
    let me = info.get_hit_node();
    let Some(parent) = info.get_parent(me) else {
        return Update::DoNothing;
    };
    let items = roving::items_of(&info, parent, CATEGORY_CLASS);
    let Some(current) = items.iter().position(|n| *n == me) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, items.len(), step, false) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

extern "C" fn on_search_text(mut data: RefAny, info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let update = match data.downcast_ref::<SearchRef>() {
        Some(s) => match s.on_search.as_ref() {
            Some(ShellSettingsLayoutOnSearch { callback, refany }) => {
                callback.invoke(refany.clone(), info, AzString::from(state.get_text()))
            }
            None => Update::DoNothing,
        },
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn hook(event: EventFilter, cb: usize, refany: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// One category: a tab of the list.
#[allow(clippy::too_many_arguments)]
fn category(
    label: AzString,
    index: usize,
    stop: usize,
    active: bool,
    on_category: &OptionShellSettingsLayoutOnCategory,
    extras: (AzString, AzString, Option<UiTheme>),
    look: &ShellLook,
    kit: &crate::widgets::dialog_kit::DialogKitLook,
) -> Dom {
    let (icon, badge, inner) = extras;
    let base = part(ITEM_BASE, &look.settings_category);
    let css = if active {
        stack_state(&base, &look.settings_category_active)
    } else {
        base
    };
    let cat_ref = RefAny::new(CategoryRef {
        on_category: on_category.clone(),
        index,
    });
    Dom::create_div()
        .with_ids_and_classes(state_classes(CATEGORY_CLASS, active, CATEGORY_ACTIVE_CLASS))
        .with_css_props(css)
        .with_tab_index(roving::item_tab_index(index, stop))
        .with_accessibility_info(AccessibilityInfo {
            states: if active {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            },
            ..AccessibilityInfo::named(label.clone(), AccessibilityRole::PageTab)
        })
        .with_callbacks(
            alloc::vec![
                hook(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_category_click as usize,
                    cat_ref.clone()
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    on_category_key as usize,
                    cat_ref
                ),
            ]
            .into(),
        )
        .with_children(DomVec::from_vec({
            let mut row: Vec<Dom> = Vec::with_capacity(3);
            if !icon.as_str().is_empty() {
                row.push(
                    Dom::create_icon(icon)
                        .with_class(AzString::from_const_str(CATEGORY_ICON_CLASS))
                        .with_css_props(part(LABEL_BASE, &kit.category_icon)),
                );
            }
            row.push(text(label).with_css_props(part(GROW_LABEL_BASE, &[])));
            if !badge.as_str().is_empty() {
                let mut b = crate::widgets::badge::Badge::create(badge)
                    .with_badge_kind(crate::widgets::badge::BadgeKind::Primary);
                if let Some(t) = inner {
                    b = b.with_theme(t);
                }
                row.push(
                    Dom::create_div()
                        .with_class(AzString::from_const_str(CATEGORY_BADGE_CLASS))
                        .with_css_props(part(LABEL_BASE, &[]))
                        .with_child(b.dom()),
                );
            }
            row
        }))
}

/// The layout's DOM in `look`: root [search row, body [nav categories,
/// main sections [section [title, content]]]].
pub(crate) fn build(layout: ShellSettingsLayout, look: &ShellLook) -> Dom {
    let ShellSettingsLayout {
        categories,
        sections,
        search,
        search_placeholder,
        on_category,
        on_search,
        category_icons,
        category_badges,
        footer,
        active_category,
        theme,
    } = layout;
    let inner = inner_theme(theme);
    let kit = crate::widgets::dialog_kit::look_for(theme);

    let mut field = TextInput::create_search()
        .with_text(search.clone())
        .with_placeholder(search_placeholder)
        .with_accessibility_name("Search settings")
        .with_on_text_input(
            RefAny::new(SearchRef { on_search }),
            on_search_text as TextInputOnTextInputCallbackType,
        );
    if let Some(t) = inner {
        field = field.with_theme(t);
    }
    let search_row = Dom::create_div()
        .with_class(AzString::from_const_str(SEARCH_CLASS))
        .with_css_props(part(CHROME_ROW_BASE, &look.settings_search))
        .with_child(field.dom());

    let labels = categories.into_library_owned_vec();
    let stop = roving::stop_index(Some(active_category), labels.len());
    let nth = |v: &StringVec, i: usize| {
        v.as_ref()
            .get(i)
            .cloned()
            .unwrap_or_else(|| AzString::from_const_str(""))
    };
    let tabs: Vec<Dom> = labels
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            category(
                l,
                i,
                stop,
                i == active_category,
                &on_category,
                (nth(&category_icons, i), nth(&category_badges, i), inner),
                look,
                &kit,
            )
        })
        .collect();
    let nav = Dom::create_node(NodeType::Nav)
        .with_class(AzString::from_const_str(CATEGORIES_CLASS))
        .with_css_props(part(RAIL_BASE, &look.settings_categories))
        .with_accessibility_info(AccessibilityInfo::named(
            "Settings categories",
            AccessibilityRole::PageTabList,
        ))
        .with_children(DomVec::from_vec(tabs));

    let query = search.as_str();
    let shown: Vec<Dom> = sections
        .into_library_owned_vec()
        .into_iter()
        .filter(|s| {
            query.is_empty()
                || palette_matches(query, s.title.as_str())
                || crate::widgets::dialog_kit::contains_ignore_case(s.keywords.as_str(), query)
        })
        .map(|s| {
            Dom::create_node(NodeType::Section)
                .with_class(AzString::from_const_str(SECTION_CLASS))
                .with_css_props(part(COLUMN_BASE, &look.settings_section))
                .with_accessibility_name(s.title.clone())
                .with_children(DomVec::from_vec(alloc::vec![
                    text(s.title)
                        .with_class(AzString::from_const_str(SECTION_TITLE_CLASS))
                        .with_css_props(part(LABEL_BASE, &look.settings_section_title)),
                    s.content,
                ]))
        })
        .collect();
    let main = Dom::create_node(NodeType::Main)
        .with_class(AzString::from_const_str(SECTIONS_CLASS))
        .with_css_props(part(SCROLL_COLUMN_BASE, &look.settings_sections))
        .with_children(DomVec::from_vec(shown));

    let body = Dom::create_div()
        .with_class(AzString::from_const_str(BODY_CLASS))
        .with_css_props(part(GROW_ROW_BASE, &[]))
        .with_children(DomVec::from_vec(alloc::vec![nav, main]));

    let mut rows = alloc::vec![search_row, body];
    if let Some(footer) = footer.into_option() {
        rows.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(FOOTER_CLASS))
                .with_css_props(part(CHROME_ROW_BASE, &[]))
                .with_child(footer),
        );
    }
    Dom::create_div()
        .with_ids_and_classes(root_classes(SETTINGS_CLASS, look))
        .with_css_props(part(FILL_COLUMN_BASE, &look.settings_root))
        .with_children(DomVec::from_vec(rows))
}

#[cfg(test)]
mod settings_layout_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, TabIndex},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        shells::fixtures::settings_layout,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn indices_of(dom: &Dom, class: &str) -> Vec<usize> {
        tc::nodes(dom)
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| tc::has_class(n, class))
            .map(|(i, _)| i)
            .collect()
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    #[test]
    fn the_layout_is_a_search_row_over_categories_beside_sections() {
        let dom = settings_layout().with_theme(UiTheme::Flat).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 2, "search, body");
        assert!(tc::has_class(&kids[0], SEARCH_CLASS));
        assert!(tc::has_class(&kids[1], BODY_CLASS));
        let body = kids[1].children.as_ref();
        assert!(matches!(body[0].root.get_node_type(), NodeType::Nav));
        assert!(matches!(body[1].root.get_node_type(), NodeType::Main));
        let cats = tc::find_all(&dom, CATEGORY_CLASS);
        assert_eq!(cats.len(), 3);
        assert!(tc::has_class(cats[1], CATEGORY_ACTIVE_CLASS), "Accounts is active");
        assert_eq!(cats[1].root.get_tab_index(), Some(TabIndex::Auto));
        assert_eq!(cats[0].root.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
        assert_eq!(
            cats[1].root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::PageTab)
        );
        let sections = tc::find_all(&dom, SECTION_CLASS);
        assert_eq!(sections.len(), 2);
        assert!(matches!(sections[0].root.get_node_type(), NodeType::Section));
        assert!(tc::has_class(&sections[0].children.as_ref()[0], SECTION_TITLE_CLASS));
    }

    #[test]
    fn a_search_hides_the_sections_whose_title_does_not_match() {
        let dom = settings_layout()
            .with_search(AzString::from("lang"))
            .with_theme(UiTheme::Flat)
            .dom();
        let sections = tc::find_all(&dom, SECTION_CLASS);
        assert_eq!(sections.len(), 1);
        assert_eq!(
            sections[0]
                .root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|s| s.as_str().to_string())),
            Some("Language".to_string())
        );
    }

    type Log = Arc<Mutex<Vec<usize>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(index);
        }
        Update::DoNothing
    }

    #[test]
    fn a_click_chooses_the_category_and_the_arrows_move_the_stop() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = settings_layout()
            .with_on_category(RefAny::new(log.clone()), record as ShellSettingsLayoutOnCategoryCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom.clone());
        let cats = indices_of(&dom, CATEGORY_CLASS);
        rv::fire(&styled, node(cats[2]), EventFilter::Hover(HoverEventFilter::Click)).expect("click");
        assert_eq!(*log.lock().expect("log"), vec![2]);
        let (_, changes) = rv::press(&styled, node(cats[1]), VirtualKeyCode::Up, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(cats[0])));
        assert!(rv::prevented(&changes));
        let (_, changes) = rv::press(&styled, node(cats[0]), VirtualKeyCode::End, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(cats[2])));
    }

    #[test]
    fn a_layout_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "settings_layout",
            || settings_layout().dom(),
            |t: UiTheme| settings_layout().with_theme(t).dom(),
        );
    }

    #[test]
    fn a_category_shows_its_icon_and_its_badge() {
        for theme in checks::BOTH {
            let dom = settings_layout()
                .with_category_icons(StringVec::from_vec(vec![
                    AzString::from("tune"),
                    AzString::from("person"),
                    AzString::from(""),
                ]))
                .with_category_badges(StringVec::from_vec(vec![
                    AzString::from(""),
                    AzString::from("3"),
                ]))
                .with_theme(theme)
                .dom();
            let cats = tc::find_all(&dom, CATEGORY_CLASS);
            assert_eq!(cats.len(), 3, "{}", theme.name());
            assert!(tc::find(cats[0], CATEGORY_ICON_CLASS).is_some(), "General has its icon");
            assert!(
                matches!(
                    tc::find(cats[0], CATEGORY_ICON_CLASS).map(|n| n.root.get_node_type()),
                    Some(NodeType::Icon(_))
                ),
                "the icon is a glyph"
            );
            assert!(tc::find(cats[2], CATEGORY_ICON_CLASS).is_none(), "an empty name: no icon");
            assert!(tc::find(cats[0], CATEGORY_BADGE_CLASS).is_none(), "an empty badge: none");
            let badge = tc::find(cats[1], CATEGORY_BADGE_CLASS).expect("Accounts' badge");
            let shown = tc::nodes(badge).into_iter().any(|(_, n)| {
                matches!(n.root.get_node_type(), NodeType::Text(t) if t.as_str() == "3")
            });
            assert!(shown, "{}: the badge says 3", theme.name());
            assert!(tc::find(cats[2], CATEGORY_BADGE_CLASS).is_none(), "past the badges: none");
        }
    }

    #[test]
    fn a_footer_sits_under_the_body() {
        let dom = settings_layout()
            .with_footer(Dom::create_div().with_class(AzString::from("app-footer")))
            .with_theme(UiTheme::Flat)
            .dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 3, "search, body, footer");
        assert!(tc::has_class(&kids[2], FOOTER_CLASS));
        assert!(tc::find(&kids[2], "app-footer").is_some());
        assert_eq!(
            settings_layout().with_theme(UiTheme::Flat).dom().children.as_ref().len(),
            2,
            "no footer, no row"
        );
    }

    #[test]
    fn a_search_keeps_a_section_whose_settings_match() {
        let layout = || {
            ShellSettingsLayout::create(StringVec::from_vec(vec![AzString::from("General")]))
                .with_section(
                    ShellSettingsSection::create(AzString::from("Startup"), Dom::create_div())
                        .with_keywords(AzString::from("Open the last folder")),
                )
                .with_section(ShellSettingsSection::create(
                    AzString::from("Language"),
                    Dom::create_div(),
                ))
                .with_theme(UiTheme::Flat)
        };
        let names = |dom: &Dom| -> Vec<String> {
            tc::find_all(dom, SECTION_CLASS)
                .iter()
                .filter_map(|s| {
                    s.root
                        .get_accessibility_info()
                        .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
                })
                .collect()
        };
        assert_eq!(names(&layout().with_search(AzString::from("FOLDER")).dom()), vec!["Startup"]);
        assert_eq!(names(&layout().with_search(AzString::from("lang")).dom()), vec!["Language"]);
        assert!(names(&layout().with_search(AzString::from("zzz")).dom()).is_empty());
    }
}
