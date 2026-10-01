//! ShellSettingsDialog - a whole settings window from a table of settings,
//! on [`ShellSettingsLayout`]: the categories (with icons) on the left, the
//! active category's settings grouped in sections on the right, the search
//! on top, the button row under it.
//!
//! The app describes each setting as data ([`ShellSetting`]: an id, a
//! label, a help line, its category and section, its value, its default)
//! and the dialog draws the row with the control its value calls for
//! ([`ShellSettingValue`]): a toggle (`Switch`), a choice (`DropDown`), a
//! number with its unit (`NumberInput`), a text (`TextInput`), a path
//! (`PathInput`), a colour (`ColorInput`), a shortcut (`ShortcutRecorder`),
//! a slider with its value read out (`Slider`), a radio set (`RadioGroup`).
//! A setting that takes effect only after a restart carries a "Requires
//! restart" badge.
//!
//! The SEARCH looks through every category: a non-empty query shows every
//! matching setting (its label, help, keywords, section or category),
//! grouped under "Category: Section", the matching text marked, and each
//! category shows how many of its settings match.
//!
//! DIRTY TRACKING: every setting keeps the value in effect (`applied`)
//! beside the value shown (`value`). With [`ShellSettingsApplyMode::Instant`]
//! (macOS) a change takes effect at once and there are no buttons; with
//! [`ShellSettingsApplyMode::ApplyButton`] (Windows) a changed setting is
//! marked, and OK / Cancel / Apply commit or drop the changes (Apply is
//! inert while nothing changed). "Restore defaults" resets the active
//! category. When a setting that requires a restart took effect, the
//! button row says "Restart to apply some changes."
//!
//! The dialog owns nothing: the app keeps the [`ShellSettingsDialog`]
//! itself, hears every request through ONE callback (a
//! [`ShellSettingsEvent`]), hands the event to
//! [`ShellSettingsDialog::apply_event`] (the rule above) and rebuilds; on
//! `Ok` / `Cancel` it also closes the window, on `Apply` / `Ok` it reads the
//! applied values ([`ShellSettingsDialog::value_of`]).
//!
//! Key types: [`ShellSettingsDialog`], [`ShellSetting`],
//! [`ShellSettingValue`], [`ShellSettingsEvent`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::Update,
    dom::{Dom, DomVec},
    global_hotkey::GlobalHotkey,
    refany::RefAny,
};
use azul_css::{
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq, props::basic::color::ColorU, AzString, StringVec,
};

use super::{
    settings_layout::{ShellSettingsLayout, ShellSettingsSection, ShellSettingsSectionVec},
    COLUMN_BASE, GROW_COLUMN_BASE, GROW_LABEL_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        badge::{Badge, BadgeKind},
        dialog_kit::{
            self, DialogKitLook, BUTTON_ROW_BASE, FIXED_BASE, FIXED_COLUMN_BASE, ROW_MIDDLE_BASE,
            ROW_TOP_BASE, SPACER_BASE,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The class of one setting row.
pub const SETTING_ROW_CLASS: &str = "__azul-native-settings-dialog-row";
/// The class of a "Requires restart" badge.
pub const SETTING_RESTART_CLASS: &str = "__azul-native-settings-dialog-restart";
/// The class of the mark of a setting changed but not applied.
pub const SETTING_MODIFIED_CLASS: &str = "__azul-native-settings-dialog-modified";
/// The class of the dialog's button row.
pub const SETTINGS_BUTTONS_CLASS: &str = "__azul-native-settings-dialog-buttons";
/// The class of the "Restart to apply" notice.
pub const SETTINGS_NOTICE_CLASS: &str = "__azul-native-settings-dialog-notice";

/// `recording` while no shortcut listens.
pub const NOT_RECORDING: usize = usize::MAX;

// ---------------------------------------------------------------------------
// The values
// ---------------------------------------------------------------------------

/// A choice among options (a drop-down, a radio set).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSettingChoice {
    /// The options, in order.
    pub options: StringVec,
    /// The chosen option.
    pub selected: usize,
}

impl ShellSettingChoice {
    /// Option `selected` of `options`.
    #[must_use]
    pub fn create(options: StringVec, selected: usize) -> Self {
        Self { options, selected }
    }
}

/// A number in a range, with its unit (a number field, a slider).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSettingNumber {
    /// The unit after the value ("ms", "%", "px"), or empty.
    pub unit: AzString,
    /// The value.
    pub value: f32,
    /// The smallest value.
    pub min: f32,
    /// The largest value.
    pub max: f32,
}

impl ShellSettingNumber {
    /// `value` in `min..=max`, without a unit.
    #[must_use]
    pub fn create(value: f32, min: f32, max: f32) -> Self {
        Self {
            unit: AzString::from_const_str(""),
            value,
            min,
            max,
        }
    }

    /// The unit after the value.
    #[must_use]
    pub fn with_unit(mut self, unit: AzString) -> Self {
        self.unit = unit;
        self
    }

    /// The value as a person reads it: "250 ms", "75 %", "1.5".
    #[must_use]
    pub fn display_text(&self) -> AzString {
        let v = if (self.value - self.value.round()).abs() < 0.005 {
            alloc::format!("{}", self.value.round())
        } else {
            alloc::format!("{:.1}", self.value)
        };
        if self.unit.as_str().is_empty() {
            AzString::from(v)
        } else {
            AzString::from(alloc::format!("{v} {}", self.unit.as_str()))
        }
    }
}

/// A keyboard shortcut, or none.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellSettingShortcut {
    /// The shortcut (meaningful while `has_hotkey`).
    pub hotkey: GlobalHotkey,
    /// Whether there is one.
    pub has_hotkey: bool,
}

impl ShellSettingShortcut {
    /// The shortcut `hotkey`.
    #[must_use]
    pub const fn create(hotkey: GlobalHotkey) -> Self {
        Self {
            hotkey,
            has_hotkey: true,
        }
    }

    /// No shortcut.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            hotkey: GlobalHotkey::create(
                azul_core::global_hotkey::HotkeyModifiers::NONE,
                azul_core::window::VirtualKeyCode::Escape,
            ),
            has_hotkey: false,
        }
    }
}

/// A setting's value - which also says which control edits it.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum ShellSettingValue {
    /// On / off: a `Switch`.
    Toggle(bool),
    /// One of a few options: a `DropDown`.
    Choice(ShellSettingChoice),
    /// A number with its unit: a `NumberInput`.
    Number(ShellSettingNumber),
    /// A line of text: a `TextInput`.
    Text(AzString),
    /// A folder or a file: a `PathInput`.
    Path(AzString),
    /// A colour: a `ColorInput`.
    Color(ColorU),
    /// A keyboard shortcut: a `ShortcutRecorder`.
    Shortcut(ShellSettingShortcut),
    /// A number in a range, read out beside it: a `Slider`.
    Slider(ShellSettingNumber),
    /// One of a few options, all in view: a `RadioGroup`.
    Radio(ShellSettingChoice),
}

impl_option!(
    ShellSettingValue,
    OptionShellSettingValue,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl ShellSettingValue {
    /// A toggle's state (`false` for any other value).
    #[must_use]
    pub const fn as_bool(&self) -> bool {
        matches!(self, Self::Toggle(true))
    }

    /// A number's or a slider's value (0 for any other value).
    #[must_use]
    pub const fn as_number(&self) -> f32 {
        match self {
            Self::Number(n) | Self::Slider(n) => n.value,
            _ => 0.0,
        }
    }

    /// A choice's or a radio set's option (0 for any other value).
    #[must_use]
    pub const fn as_index(&self) -> usize {
        match self {
            Self::Choice(c) | Self::Radio(c) => c.selected,
            _ => 0,
        }
    }

    /// The value as a person reads it: the text or path, the chosen
    /// option's label, the number with its unit, the shortcut, the colour
    /// as `#rrggbb`, "On" / "Off".
    #[must_use]
    pub fn display_text(&self) -> AzString {
        match self {
            Self::Toggle(b) => AzString::from_const_str(if *b { "On" } else { "Off" }),
            Self::Text(t) | Self::Path(t) => t.clone(),
            Self::Choice(c) | Self::Radio(c) => c
                .options
                .as_ref()
                .get(c.selected)
                .cloned()
                .unwrap_or_else(|| AzString::from_const_str("")),
            Self::Number(n) | Self::Slider(n) => n.display_text(),
            Self::Color(c) => AzString::from(alloc::format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
            Self::Shortcut(s) => {
                if s.has_hotkey {
                    s.hotkey.to_display_string()
                } else {
                    AzString::from_const_str("None")
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The settings
// ---------------------------------------------------------------------------

/// One setting: a row of the dialog.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSetting {
    /// The app's key for it ("editor.font_size").
    pub id: AzString,
    /// The row's label ("Font size").
    pub label: AzString,
    /// The help line under the label, or empty.
    pub help: AzString,
    /// More words a search finds it by ("zoom text"), or empty.
    pub keywords: AzString,
    /// The section of its category it sits in ("Editor").
    pub section: AzString,
    /// The value shown.
    pub value: ShellSettingValue,
    /// The value "Restore defaults" puts back.
    pub default_value: ShellSettingValue,
    /// The value in effect (the last applied one).
    pub applied: ShellSettingValue,
    /// Its category, an index into the dialog's categories.
    pub category: usize,
    /// Whether it takes effect only after a restart.
    pub requires_restart: bool,
}

impl_option!(
    ShellSetting,
    OptionShellSetting,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellSetting,
    ShellSettingVec,
    ShellSettingVecDestructor,
    ShellSettingVecDestructorType,
    ShellSettingVecSlice,
    OptionShellSetting
);
impl_vec_clone!(ShellSetting, ShellSettingVec, ShellSettingVecDestructor);
impl_vec_debug!(ShellSetting, ShellSettingVec);
impl_vec_partialeq!(ShellSetting, ShellSettingVec);
impl_vec_mut!(ShellSetting, ShellSettingVec);

impl ShellSetting {
    /// Setting `id`, labelled `label`, in section `section` of category
    /// `category`, holding `value` (also its default and its applied value).
    #[must_use]
    pub fn create(
        id: AzString,
        label: AzString,
        category: usize,
        section: AzString,
        value: ShellSettingValue,
    ) -> Self {
        Self {
            id,
            label,
            help: AzString::from_const_str(""),
            keywords: AzString::from_const_str(""),
            section,
            default_value: value.clone(),
            applied: value.clone(),
            value,
            category,
            requires_restart: false,
        }
    }

    /// The help line under the label.
    #[must_use]
    pub fn with_help(mut self, help: AzString) -> Self {
        self.help = help;
        self
    }

    /// More words a search finds it by.
    #[must_use]
    pub fn with_keywords(mut self, keywords: AzString) -> Self {
        self.keywords = keywords;
        self
    }

    /// Whether it takes effect only after a restart.
    #[must_use]
    pub const fn with_requires_restart(mut self, requires_restart: bool) -> Self {
        self.requires_restart = requires_restart;
        self
    }

    /// The value "Restore defaults" puts back (when the shown one is not).
    #[must_use]
    pub fn with_default(mut self, default_value: ShellSettingValue) -> Self {
        self.default_value = default_value;
        self
    }

    /// Whether the shown value differs from the one in effect.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.value != self.applied
    }

    /// Whether the shown value is the default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.value == self.default_value
    }

    /// Whether a search for `query` finds it: its label, help, keywords or
    /// section, or the name of its category (`category_name`), contain it.
    #[must_use]
    pub fn matches(&self, query: &str, category_name: &str) -> bool {
        use crate::widgets::dialog_kit::find_ignore_case;
        let q = query.trim();
        if q.is_empty() {
            return true;
        }
        [
            self.label.as_str(),
            self.help.as_str(),
            self.keywords.as_str(),
            self.section.as_str(),
            category_name,
        ]
        .iter()
        .any(|t| find_ignore_case(t, q).is_some())
    }

    /// Every text a search looks at, in one line (a section's keywords).
    #[must_use]
    pub(crate) fn search_text(&self) -> String {
        alloc::format!(
            "{} {} {} {}",
            self.label.as_str(),
            self.help.as_str(),
            self.keywords.as_str(),
            self.section.as_str()
        )
    }
}

// ---------------------------------------------------------------------------
// The event
// ---------------------------------------------------------------------------

/// When a change takes effect.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ShellSettingsApplyMode {
    /// At once, with no buttons (macOS System Settings).
    Instant,
    /// On OK or Apply; Cancel drops it (a Windows dialog).
    #[default]
    ApplyButton,
}

impl ShellSettingsApplyMode {
    /// The host platform's convention: `Instant` on macOS, `ApplyButton`
    /// elsewhere.
    #[must_use]
    pub fn platform() -> Self {
        if azul_core::window::mac_shortcut_conventions() {
            Self::Instant
        } else {
            Self::ApplyButton
        }
    }
}

/// What the user asked of the dialog.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShellSettingsEventKind {
    /// Setting `index` now shows `value`.
    Changed,
    /// Category `index` was chosen.
    CategoryChosen,
    /// The search reads `text`.
    SearchChanged,
    /// "Restore defaults" for category `index`.
    RestoreDefaults,
    /// Apply the changes.
    Apply,
    /// Apply the changes and close.
    Ok,
    /// Drop the changes and close.
    Cancel,
    /// The shortcut of setting `index` listens.
    StartRecording,
    /// No shortcut listens any more.
    StopRecording,
}

/// One request from the dialog.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSettingsEvent {
    /// The new value (`Changed`), else `Toggle(false)`.
    pub value: ShellSettingValue,
    /// The search text (`SearchChanged`), else empty.
    pub text: AzString,
    /// The setting (`Changed`, `StartRecording`) or the category
    /// (`CategoryChosen`, `RestoreDefaults`), else 0.
    pub index: usize,
    /// What was asked.
    pub kind: ShellSettingsEventKind,
}

impl ShellSettingsEvent {
    /// An event of `kind` at `index`.
    #[must_use]
    pub fn create(kind: ShellSettingsEventKind, index: usize) -> Self {
        Self {
            value: ShellSettingValue::Toggle(false),
            text: AzString::from_const_str(""),
            index,
            kind,
        }
    }

    /// `Changed`: setting `index` now shows `value`.
    #[must_use]
    pub fn changed(index: usize, value: ShellSettingValue) -> Self {
        Self {
            value,
            ..Self::create(ShellSettingsEventKind::Changed, index)
        }
    }
}

/// Callback invoked for a request from the dialog.
pub type ShellSettingsDialogOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ShellSettingsEvent) -> Update;
impl_widget_callback!(
    ShellSettingsDialogOnEvent,
    OptionShellSettingsDialogOnEvent,
    ShellSettingsDialogOnEventCallback,
    ShellSettingsDialogOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellSettingsDialogOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_SETTINGS_DIALOG_ON_EVENT_INVOKER,
    invoker_ty:     AzShellSettingsDialogOnEventCallbackInvoker,
    thunk_fn:       az_shell_settings_dialog_on_event_callback_thunk,
    setter_fn:      AzApp_setShellSettingsDialogOnEventCallbackInvoker,
    from_handle_fn: AzShellSettingsDialogOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellSettingsDialogOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ShellSettingsEvent ],
}

// ---------------------------------------------------------------------------
// The dialog
// ---------------------------------------------------------------------------

/// A settings window built from a table of settings.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ShellSettingsDialog {
    /// The categories, top to bottom.
    pub categories: StringVec,
    /// The categories' icons, parallel to `categories`.
    pub category_icons: StringVec,
    /// Every setting, of every category.
    pub settings: ShellSettingVec,
    /// The search text.
    pub search: AzString,
    /// "Restore defaults".
    pub restore_label: AzString,
    /// "OK".
    pub ok_label: AzString,
    /// "Cancel".
    pub cancel_label: AzString,
    /// "Apply".
    pub apply_label: AzString,
    /// The badge of a setting that requires a restart.
    pub restart_label: AzString,
    /// The notice once such a setting took effect.
    pub restart_notice: AzString,
    /// The line a search that finds nothing shows.
    pub empty_label: AzString,
    /// Hears every request.
    pub on_event: OptionShellSettingsDialogOnEvent,
    /// The category shown while there is no search.
    pub active_category: usize,
    /// The setting whose shortcut listens, or [`NOT_RECORDING`].
    pub recording: usize,
    /// The widget theme this dialog is PINNED to, or `None` to follow the
    /// app theme.
    pub theme: OptionUiTheme,
    /// When a change takes effect.
    pub apply_mode: ShellSettingsApplyMode,
    /// Whether a setting that requires a restart took effect.
    pub restart_pending: bool,
}

impl ShellSettingsDialog {
    /// A dialog over `categories` with no settings yet, the first category
    /// shown, Windows' OK / Cancel / Apply.
    #[must_use]
    pub fn create(categories: StringVec) -> Self {
        Self {
            categories,
            category_icons: StringVec::from_const_slice(&[]),
            settings: ShellSettingVec::from_const_slice(&[]),
            search: AzString::from_const_str(""),
            restore_label: AzString::from_const_str("Restore defaults"),
            ok_label: AzString::from_const_str("OK"),
            cancel_label: AzString::from_const_str("Cancel"),
            apply_label: AzString::from_const_str("Apply"),
            restart_label: AzString::from_const_str("Requires restart"),
            restart_notice: AzString::from_const_str("Restart to apply some changes."),
            empty_label: AzString::from_const_str("No settings match the search."),
            on_event: None.into(),
            active_category: 0,
            recording: NOT_RECORDING,
            theme: OptionUiTheme::None,
            apply_mode: ShellSettingsApplyMode::ApplyButton,
            restart_pending: false,
        }
    }

    /// The categories' icons.
    pub fn set_category_icons(&mut self, icons: StringVec) {
        self.category_icons = icons;
    }

    /// [`Self::set_category_icons`] for the builder chain.
    #[must_use]
    pub fn with_category_icons(mut self, icons: StringVec) -> Self {
        self.set_category_icons(icons);
        self
    }

    /// Appends a setting.
    pub fn add_setting(&mut self, setting: ShellSetting) {
        let mut v = core::mem::replace(&mut self.settings, ShellSettingVec::from_const_slice(&[]))
            .into_library_owned_vec();
        v.push(setting);
        self.settings = ShellSettingVec::from_vec(v);
    }

    /// [`Self::add_setting`] for the builder chain.
    #[must_use]
    pub fn with_setting(mut self, setting: ShellSetting) -> Self {
        self.add_setting(setting);
        self
    }

    /// Replaces the settings.
    pub fn set_settings(&mut self, settings: ShellSettingVec) {
        self.settings = settings;
    }

    /// [`Self::set_settings`] for the builder chain.
    #[must_use]
    pub fn with_settings(mut self, settings: ShellSettingVec) -> Self {
        self.set_settings(settings);
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

    /// The category shown.
    pub const fn set_active_category(&mut self, index: usize) {
        self.active_category = index;
    }

    /// [`Self::set_active_category`] for the builder chain.
    #[must_use]
    pub const fn with_active_category(mut self, index: usize) -> Self {
        self.set_active_category(index);
        self
    }

    /// When a change takes effect.
    pub const fn set_apply_mode(&mut self, mode: ShellSettingsApplyMode) {
        self.apply_mode = mode;
    }

    /// [`Self::set_apply_mode`] for the builder chain.
    #[must_use]
    pub const fn with_apply_mode(mut self, mode: ShellSettingsApplyMode) -> Self {
        self.set_apply_mode(mode);
        self
    }

    /// The button labels: "Restore defaults", "OK", "Cancel", "Apply".
    pub fn set_labels(
        &mut self,
        restore: AzString,
        ok: AzString,
        cancel: AzString,
        apply: AzString,
    ) {
        self.restore_label = restore;
        self.ok_label = ok;
        self.cancel_label = cancel;
        self.apply_label = apply;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(
        mut self,
        restore: AzString,
        ok: AzString,
        cancel: AzString,
        apply: AzString,
    ) -> Self {
        self.set_labels(restore, ok, cancel, apply);
        self
    }

    /// The restart texts: the badge, the notice; and the empty search line.
    pub fn set_texts(
        &mut self,
        restart_label: AzString,
        restart_notice: AzString,
        empty_label: AzString,
    ) {
        self.restart_label = restart_label;
        self.restart_notice = restart_notice;
        self.empty_label = empty_label;
    }

    /// [`Self::set_texts`] for the builder chain.
    #[must_use]
    pub fn with_texts(
        mut self,
        restart_label: AzString,
        restart_notice: AzString,
        empty_label: AzString,
    ) -> Self {
        self.set_texts(restart_label, restart_notice, empty_label);
        self
    }

    /// The callback that hears every request.
    pub fn set_on_event<C: Into<ShellSettingsDialogOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(ShellSettingsDialogOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ShellSettingsDialogOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme; unset, the dialog follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Whether any setting shows a value that is not in effect.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty_count() > 0
    }

    /// How many settings show a value that is not in effect.
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.settings
            .as_ref()
            .iter()
            .filter(|s| s.is_dirty())
            .count()
    }

    /// The value in effect of setting `id`, or `None`.
    #[must_use]
    pub fn value_of(&self, id: AzString) -> OptionShellSettingValue {
        self.settings
            .as_ref()
            .iter()
            .find(|s| s.id.as_str() == id.as_str())
            .map(|s| s.applied.clone())
            .into()
    }

    /// The category's name (empty past the end).
    fn category_name(&self, index: usize) -> &str {
        self.categories
            .as_ref()
            .get(index)
            .map_or("", |c| c.as_str())
    }

    /// How many settings of category `index` the search finds.
    #[must_use]
    pub fn matches_in(&self, index: usize) -> usize {
        let name = self.category_name(index);
        self.settings
            .as_ref()
            .iter()
            .filter(|s| s.category == index && s.matches(self.search.as_str(), name))
            .count()
    }

    /// The rule an app keeps the dialog with: a change shows the new value
    /// (and, `Instant`, puts it in effect); a category clears the search;
    /// "Restore defaults" resets the category's settings; Apply and OK put
    /// every change in effect, Cancel drops them; a setting that requires
    /// a restart and took effect raises the restart notice.
    pub fn apply_event(&mut self, event: ShellSettingsEvent) {
        let instant = self.apply_mode == ShellSettingsApplyMode::Instant;
        let mut v = core::mem::replace(&mut self.settings, ShellSettingVec::from_const_slice(&[]))
            .into_library_owned_vec();
        let mut restart = false;
        // Puts `s`'s shown value in effect; a restart is due when it changed.
        let mut take_effect = |s: &mut ShellSetting| {
            if s.value != s.applied {
                restart |= s.requires_restart;
                s.applied = s.value.clone();
            }
        };
        match event.kind {
            ShellSettingsEventKind::Changed => {
                if let Some(s) = v.get_mut(event.index) {
                    s.value = event.value;
                    if instant {
                        take_effect(s);
                    }
                }
                if self.recording == event.index {
                    self.recording = NOT_RECORDING;
                }
            }
            ShellSettingsEventKind::CategoryChosen => {
                self.active_category = event.index;
                self.search = AzString::from_const_str("");
            }
            ShellSettingsEventKind::SearchChanged => self.search = event.text,
            ShellSettingsEventKind::RestoreDefaults => {
                for s in v.iter_mut().filter(|s| s.category == event.index) {
                    s.value = s.default_value.clone();
                    if instant {
                        take_effect(s);
                    }
                }
            }
            ShellSettingsEventKind::Apply | ShellSettingsEventKind::Ok => {
                for s in &mut v {
                    take_effect(s);
                }
            }
            ShellSettingsEventKind::Cancel => {
                for s in &mut v {
                    s.value = s.applied.clone();
                }
                self.recording = NOT_RECORDING;
            }
            ShellSettingsEventKind::StartRecording => self.recording = event.index,
            ShellSettingsEventKind::StopRecording => self.recording = NOT_RECORDING,
        }
        self.settings = ShellSettingVec::from_vec(v);
        self.restart_pending |= restart;
    }

    /// Replaces `self` with an empty dialog and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The dialog's DOM: a [`ShellSettingsLayout`] of the rows.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for ShellSettingsDialog {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

impl From<ShellSettingsDialog> for Dom {
    fn from(d: ShellSettingsDialog) -> Self {
        d.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// What every control of one row shares: the dialog's callback, the
/// setting and its current value (the template a new value is made from).
struct SettingRef {
    on_event: OptionShellSettingsDialogOnEvent,
    index: usize,
    value: ShellSettingValue,
}

/// What the layout's callbacks and the button row share.
struct DialogRef {
    on_event: OptionShellSettingsDialogOnEvent,
    active_category: usize,
}

/// Hands `event` to the dialog's callback.
fn emit(
    on_event: &OptionShellSettingsDialogOnEvent,
    info: CallbackInfo,
    event: ShellSettingsEvent,
) -> Update {
    match on_event.as_ref() {
        Some(ShellSettingsDialogOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, event)
        }
        None => Update::DoNothing,
    }
}

/// Reports `Changed` with the value `make` builds from the row's template.
fn change(
    data: &mut RefAny,
    info: CallbackInfo,
    make: impl FnOnce(&ShellSettingValue) -> ShellSettingValue,
) -> Update {
    let Some(r) = data.downcast_ref::<SettingRef>() else {
        return Update::DoNothing;
    };
    let value = make(&r.value);
    emit(
        &r.on_event,
        info,
        ShellSettingsEvent::changed(r.index, value),
    )
}

/// The template's number with `value` (clamped to its range).
fn number_with(template: &ShellSettingValue, value: f32) -> Option<ShellSettingNumber> {
    match template {
        ShellSettingValue::Number(n) | ShellSettingValue::Slider(n) => Some(ShellSettingNumber {
            value: value.clamp(n.min, n.max),
            ..n.clone()
        }),
        _ => None,
    }
}

/// The template's choice with `selected`.
fn choice_with(template: &ShellSettingValue, selected: usize) -> ShellSettingChoice {
    match template {
        ShellSettingValue::Choice(c) | ShellSettingValue::Radio(c) => ShellSettingChoice {
            options: c.options.clone(),
            selected,
        },
        _ => ShellSettingChoice::create(StringVec::from_const_slice(&[]), selected),
    }
}

extern "C" fn on_switch(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::switch::SwitchState,
) -> Update {
    change(&mut data, info, |_| {
        ShellSettingValue::Toggle(state.checked)
    })
}

extern "C" fn on_choice(mut data: RefAny, info: CallbackInfo, index: usize) -> Update {
    change(&mut data, info, |t| {
        ShellSettingValue::Choice(choice_with(t, index))
    })
}

extern "C" fn on_radio(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::radio_group::RadioGroupState,
) -> Update {
    change(&mut data, info, |t| {
        ShellSettingValue::Radio(choice_with(t, state.selected_index))
    })
}

extern "C" fn on_number(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::number_input::NumberInputState,
) -> Update {
    change(&mut data, info, |t| match number_with(t, state.number) {
        Some(n) => ShellSettingValue::Number(n),
        None => t.clone(),
    })
}

extern "C" fn on_slider(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::slider::SliderState,
) -> Update {
    change(&mut data, info, |t| match number_with(t, state.value) {
        Some(n) => ShellSettingValue::Slider(n),
        None => t.clone(),
    })
}

extern "C" fn on_text(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::text_input::TextInputState,
) -> crate::widgets::text_input::OnTextInputReturn {
    let text = AzString::from(state.get_text());
    crate::widgets::text_input::OnTextInputReturn {
        update: change(&mut data, info, |_| ShellSettingValue::Text(text)),
        valid: crate::widgets::text_input::TextInputValid::Yes,
    }
}

extern "C" fn on_path(mut data: RefAny, info: CallbackInfo, path: AzString) -> Update {
    change(&mut data, info, |_| ShellSettingValue::Path(path))
}

extern "C" fn on_color(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::color_input::ColorInputState,
) -> Update {
    change(&mut data, info, |_| ShellSettingValue::Color(state.color))
}

/// The recorder: listening starts and stops through the dialog; a recorded
/// or cleared shortcut is a change.
extern "C" fn on_shortcut(
    mut data: RefAny,
    info: CallbackInfo,
    event: crate::widgets::shortcut_recorder::ShortcutRecorderEvent,
) -> Update {
    use crate::widgets::shortcut_recorder::ShortcutRecorderEventKind as K;
    let Some(r) = data.downcast_ref::<SettingRef>() else {
        return Update::DoNothing;
    };
    let event = match event.kind {
        K::StartRecording => {
            ShellSettingsEvent::create(ShellSettingsEventKind::StartRecording, r.index)
        }
        K::Cancelled => ShellSettingsEvent::create(ShellSettingsEventKind::StopRecording, r.index),
        K::Recorded => ShellSettingsEvent::changed(
            r.index,
            ShellSettingValue::Shortcut(ShellSettingShortcut::create(event.hotkey)),
        ),
        K::Cleared => ShellSettingsEvent::changed(
            r.index,
            ShellSettingValue::Shortcut(ShellSettingShortcut::none()),
        ),
    };
    emit(&r.on_event, info, event)
}

extern "C" fn on_category(mut data: RefAny, info: CallbackInfo, index: usize) -> Update {
    let Some(r) = data.downcast_ref::<DialogRef>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        ShellSettingsEvent::create(ShellSettingsEventKind::CategoryChosen, index),
    )
}

extern "C" fn on_search(mut data: RefAny, info: CallbackInfo, query: AzString) -> Update {
    let Some(r) = data.downcast_ref::<DialogRef>() else {
        return Update::DoNothing;
    };
    let mut event = ShellSettingsEvent::create(ShellSettingsEventKind::SearchChanged, 0);
    event.text = query;
    emit(&r.on_event, info, event)
}

/// One of the button row's buttons: report `kind` (for the shown category).
fn on_button(data: &mut RefAny, info: CallbackInfo, kind: ShellSettingsEventKind) -> Update {
    let Some(r) = data.downcast_ref::<DialogRef>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        ShellSettingsEvent::create(kind, r.active_category),
    )
}

extern "C" fn on_restore(mut data: RefAny, info: CallbackInfo) -> Update {
    on_button(&mut data, info, ShellSettingsEventKind::RestoreDefaults)
}

extern "C" fn on_ok(mut data: RefAny, info: CallbackInfo) -> Update {
    on_button(&mut data, info, ShellSettingsEventKind::Ok)
}

extern "C" fn on_cancel(mut data: RefAny, info: CallbackInfo) -> Update {
    on_button(&mut data, info, ShellSettingsEventKind::Cancel)
}

extern "C" fn on_apply(mut data: RefAny, info: CallbackInfo) -> Update {
    on_button(&mut data, info, ShellSettingsEventKind::Apply)
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

/// The control that edits `s` (setting `index`), named by its label.
fn control(
    index: usize,
    s: &ShellSetting,
    recording: bool,
    on_event: &OptionShellSettingsDialogOnEvent,
    inner: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    use crate::widgets::{
        color_input::{ColorInput, ColorInputOnValueChangeCallbackType},
        drop_down::{DropDown, DropDownOnChoiceChangeCallbackType},
        number_input::{NumberInput, NumberInputOnValueChangeCallbackType},
        path_input::{PathInput, PathInputOnChangeCallbackType},
        radio_group::{RadioGroup, RadioGroupOnChangeCallbackType},
        shortcut_recorder::{ShortcutRecorder, ShortcutRecorderOnEventCallbackType},
        slider::{Slider, SliderOnValueChangeCallbackType},
        switch::{Switch, SwitchOnToggleCallbackType},
        text_input::{TextInput, TextInputOnTextInputCallbackType},
    };
    let data = RefAny::new(SettingRef {
        on_event: on_event.clone(),
        index,
        value: s.value.clone(),
    });
    let label = s.label.clone();
    // A control with its read-out beside it (a unit, a slider's value).
    let with_unit = |control: Dom, unit: AzString| {
        Dom::create_div()
            .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &[]))
            .with_children(DomVec::from_vec(alloc::vec![
                control,
                dialog_kit::line(unit, FIXED_BASE, &look.unit),
            ]))
    };
    match &s.value {
        ShellSettingValue::Toggle(b) => {
            let mut w = Switch::create(*b)
                .with_accessibility_name(label)
                .with_on_toggle(data, on_switch as SwitchOnToggleCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Choice(c) => {
            let mut w = DropDown::new(c.options.clone())
                .with_selected(c.selected)
                .with_accessibility_name(label)
                .with_on_choice_change(data, on_choice as DropDownOnChoiceChangeCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Number(n) => {
            let mut w = NumberInput::create(n.value)
                .with_accessibility_name(label)
                .with_on_value_change(data, on_number as NumberInputOnValueChangeCallbackType);
            w.number_input_state.inner.min = n.min;
            w.number_input_state.inner.max = n.max;
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            with_unit(w.dom(), n.unit.clone())
        }
        ShellSettingValue::Text(text) => {
            let mut w = TextInput::create()
                .with_text(text.clone())
                .with_accessibility_name(label)
                .with_on_text_input(data, on_text as TextInputOnTextInputCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Path(path) => {
            let mut w = PathInput::create(path.clone())
                .with_accessibility_name(label)
                .with_on_change(data, on_path as PathInputOnChangeCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Color(c) => {
            let mut w = ColorInput::create(*c)
                .with_accessibility_name(label)
                .with_on_value_change(data, on_color as ColorInputOnValueChangeCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Shortcut(sc) => {
            let mut w = ShortcutRecorder::create()
                .with_accessibility_name(label)
                .with_recording(recording)
                .with_on_event(data, on_shortcut as ShortcutRecorderOnEventCallbackType);
            if sc.has_hotkey {
                w = w.with_hotkey(sc.hotkey);
            }
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
        ShellSettingValue::Slider(n) => {
            let mut w = Slider::create(n.value, n.min, n.max)
                .with_accessibility_name(label)
                .with_on_value_change(data, on_slider as SliderOnValueChangeCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            with_unit(
                Dom::create_div()
                    .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
                    .with_child(w.dom()),
                n.display_text(),
            )
        }
        ShellSettingValue::Radio(c) => {
            let mut w = RadioGroup::create(c.options.clone())
                .with_selected_index(c.selected)
                .with_accessibility_name(label)
                .with_on_change(data, on_radio as RadioGroupOnChangeCallbackType);
            if let Some(t) = inner {
                w = w.with_theme(t);
            }
            w.dom()
        }
    }
}

/// One setting's row: the label column (the label, the modified mark, the
/// restart badge, the help line - the search marked) beside the control.
#[allow(clippy::too_many_arguments)]
fn row(
    index: usize,
    s: &ShellSetting,
    query: &str,
    recording: bool,
    restart_label: &AzString,
    on_event: &OptionShellSettingsDialogOnEvent,
    inner: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    let mut head: Vec<Dom> = alloc::vec![dialog_kit::highlighted_line(
        &s.label,
        query,
        GROW_LABEL_BASE,
        &look.text,
        &look.mark,
    )];
    if s.is_dirty() {
        head.push(
            dialog_kit::line(
                AzString::from_const_str("\u{25CF}"),
                FIXED_BASE,
                &look.modified,
            )
            .with_ids_and_classes(dialog_kit::class(SETTING_MODIFIED_CLASS))
            .with_accessibility_info(AccessibilityInfo::named(
                "Modified",
                AccessibilityRole::StaticText,
            )),
        );
    }
    if s.requires_restart {
        let mut badge = Badge::create(restart_label.clone()).with_badge_kind(BadgeKind::Warning);
        if let Some(t) = inner {
            badge = badge.with_theme(t);
        }
        head.push(
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(SETTING_RESTART_CLASS))
                .with_css_props(dialog_kit::part(FIXED_BASE, &look.button))
                .with_child(badge.dom()),
        );
    }
    let mut label_column: Vec<Dom> = alloc::vec![Dom::create_div()
        .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &[]))
        .with_children(DomVec::from_vec(head))];
    if !s.help.as_str().is_empty() {
        label_column.push(dialog_kit::highlighted_line(
            &s.help,
            query,
            &[],
            &look.help,
            &look.mark,
        ));
    }
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(SETTING_ROW_CLASS))
        .with_css_props(dialog_kit::part(ROW_TOP_BASE, &look.field_row))
        .with_children(DomVec::from_vec(alloc::vec![
            Dom::create_div()
                .with_css_props(dialog_kit::part(FIXED_COLUMN_BASE, &look.field_label))
                .with_children(DomVec::from_vec(label_column)),
            Dom::create_div()
                .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
                .with_child(control(index, s, recording, on_event, inner, look)),
        ]))
}

/// The button row: "Restore defaults", the restart notice (or a spacer),
/// then - with Apply buttons - OK, Cancel and Apply (inert while nothing
/// changed).
fn buttons(dialog: &ShellSettingsDialog, inner: Option<UiTheme>, look: &DialogKitLook) -> Dom {
    let data = RefAny::new(DialogRef {
        on_event: dialog.on_event.clone(),
        active_category: dialog.active_category,
    });
    let button =
        |label: &AzString,
         kind: crate::widgets::button::ButtonType,
         click: Option<crate::widgets::button::ButtonOnClickCallbackType>| {
            dialog_kit::row_button(
                label.clone(),
                kind,
                click.map(|cb| (data.clone(), cb)),
                None,
                inner,
                (dialog_kit::BUTTON_BOX_CLASS, dialog_kit::HELD_CLASS),
                dialog_kit::BUTTON_BOX_BASE,
                (&look.button, &look.held),
            )
        };
    use crate::widgets::button::{ButtonOnClickCallbackType as Cb, ButtonType};
    let mut items: Vec<Dom> = alloc::vec![button(
        &dialog.restore_label,
        ButtonType::Default,
        Some(on_restore as Cb)
    )];
    if dialog.restart_pending {
        items.push(
            dialog_kit::line(dialog.restart_notice.clone(), GROW_LABEL_BASE, &look.notice)
                .with_ids_and_classes(dialog_kit::class(SETTINGS_NOTICE_CLASS)),
        );
    } else {
        items.push(Dom::create_div().with_css_props(dialog_kit::part(SPACER_BASE, &[])));
    }
    if dialog.apply_mode == ShellSettingsApplyMode::ApplyButton {
        items.push(button(
            &dialog.ok_label,
            ButtonType::Primary,
            Some(on_ok as Cb),
        ));
        items.push(button(
            &dialog.cancel_label,
            ButtonType::Default,
            Some(on_cancel as Cb),
        ));
        items.push(button(
            &dialog.apply_label,
            ButtonType::Default,
            dialog.is_dirty().then_some(on_apply as Cb),
        ));
    }
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(SETTINGS_BUTTONS_CLASS))
        .with_css_props(dialog_kit::part(BUTTON_ROW_BASE, &look.buttons))
        .with_children(DomVec::from_vec(items))
}

/// The dialog's DOM: the layout with the shown settings' rows in their
/// sections, the categories' icons and (while searching) match counts,
/// and the button row as its footer.
pub(crate) fn build(dialog: ShellSettingsDialog) -> Dom {
    let look = dialog_kit::look_for(dialog.theme);
    let inner = dialog_kit::inner_theme(dialog.theme);
    let query = String::from(dialog.search.as_str().trim());
    let searching = !query.is_empty();

    // The shown settings, grouped by section (by "Category: Section" while
    // searching), in the table's order.
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, s) in dialog.settings.as_ref().iter().enumerate() {
        let category = dialog.category_name(s.category);
        let shown = if searching {
            s.matches(&query, category)
        } else {
            s.category == dialog.active_category
        };
        if !shown {
            continue;
        }
        let title = if searching {
            alloc::format!("{category}: {}", s.section.as_str())
        } else {
            String::from(s.section.as_str())
        };
        match groups.iter_mut().find(|(t, _)| *t == title) {
            Some((_, members)) => members.push(i),
            None => groups.push((title, alloc::vec![i])),
        }
    }
    let settings = dialog.settings.as_ref();
    let mut sections: Vec<ShellSettingsSection> = groups
        .into_iter()
        .map(|(title, members)| {
            // Everything a search looks at, so the layout keeps the section.
            let mut keywords = String::new();
            let rows: Vec<Dom> = members
                .iter()
                .map(|&i| {
                    let s = &settings[i];
                    keywords.push_str(&s.search_text());
                    keywords.push(' ');
                    keywords.push_str(dialog.category_name(s.category));
                    keywords.push(' ');
                    row(
                        i,
                        s,
                        &query,
                        dialog.recording == i,
                        &dialog.restart_label,
                        &dialog.on_event,
                        inner,
                        &look,
                    )
                })
                .collect();
            ShellSettingsSection::create(
                AzString::from(title),
                Dom::create_div()
                    .with_css_props(dialog_kit::part(COLUMN_BASE, &[]))
                    .with_children(DomVec::from_vec(rows)),
            )
            .with_keywords(AzString::from(keywords))
        })
        .collect();
    if searching && sections.is_empty() {
        sections.push(
            ShellSettingsSection::create(dialog.empty_label.clone(), Dom::create_div())
                .with_keywords(AzString::from(query.clone())),
        );
    }
    let badges: Vec<AzString> = (0..dialog.categories.as_ref().len())
        .map(|c| match dialog.matches_in(c) {
            n if searching && n > 0 => AzString::from(alloc::format!("{n}")),
            _ => AzString::from_const_str(""),
        })
        .collect();

    let shared = RefAny::new(DialogRef {
        on_event: dialog.on_event.clone(),
        active_category: dialog.active_category,
    });
    let mut layout = ShellSettingsLayout::create(dialog.categories.clone())
        .with_category_icons(dialog.category_icons.clone())
        .with_category_badges(StringVec::from_vec(badges))
        .with_sections(ShellSettingsSectionVec::from_vec(sections))
        .with_search(dialog.search.clone())
        .with_active_category(dialog.active_category)
        .with_footer(buttons(&dialog, inner, &look))
        .with_on_category(
            shared.clone(),
            on_category as super::settings_layout::ShellSettingsLayoutOnCategoryCallbackType,
        )
        .with_on_search(
            shared,
            on_search as super::settings_layout::ShellSettingsLayoutOnSearchCallbackType,
        );
    if let Some(t) = inner {
        layout = layout.with_theme(t);
    }
    layout.dom()
}

#[cfg(test)]
pub(crate) mod settings_dialog_fixtures {
    //! A settings table of every kind of value, for the tests and the
    //! shells' lint manifest.

    use super::*;

    fn strs(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    pub(crate) fn dialog() -> ShellSettingsDialog {
        ShellSettingsDialog::create(strs(&["General", "Editor", "Appearance"]))
            .with_category_icons(strs(&["tune", "edit", "palette"]))
            .with_setting(
                ShellSetting::create(
                    AzString::from("general.restore"),
                    AzString::from("Reopen the last documents"),
                    0,
                    AzString::from("Startup"),
                    ShellSettingValue::Toggle(true),
                )
                .with_help(AzString::from("Open what was open when AzOffice closed.")),
            )
            .with_setting(
                ShellSetting::create(
                    AzString::from("general.language"),
                    AzString::from("Language"),
                    0,
                    AzString::from("Region"),
                    ShellSettingValue::Choice(ShellSettingChoice::create(
                        strs(&["English", "Deutsch", "Francais"]),
                        0,
                    )),
                )
                .with_requires_restart(true),
            )
            .with_setting(ShellSetting::create(
                AzString::from("general.folder"),
                AzString::from("Documents folder"),
                0,
                AzString::from("Files"),
                ShellSettingValue::Path(AzString::from("/home/me/Documents")),
            ))
            .with_setting(
                ShellSetting::create(
                    AzString::from("editor.font_size"),
                    AzString::from("Font size"),
                    1,
                    AzString::from("Text"),
                    ShellSettingValue::Number(
                        ShellSettingNumber::create(12.0, 6.0, 72.0).with_unit(AzString::from("pt")),
                    ),
                )
                .with_keywords(AzString::from("zoom text")),
            )
            .with_setting(ShellSetting::create(
                AzString::from("editor.author"),
                AzString::from("Author name"),
                1,
                AzString::from("Text"),
                ShellSettingValue::Text(AzString::from("Felix")),
            ))
            .with_setting(ShellSetting::create(
                AzString::from("editor.palette"),
                AzString::from("Command palette"),
                1,
                AzString::from("Keyboard"),
                ShellSettingValue::Shortcut(ShellSettingShortcut::create(GlobalHotkey::create(
                    azul_core::global_hotkey::HotkeyModifiers {
                        ctrl: true,
                        alt: false,
                        shift: true,
                        meta: false,
                    },
                    azul_core::window::VirtualKeyCode::P,
                ))),
            ))
            .with_setting(ShellSetting::create(
                AzString::from("appearance.accent"),
                AzString::from("Accent colour"),
                2,
                AzString::from("Colours"),
                ShellSettingValue::Color(ColorU::new(47, 74, 133, 255)),
            ))
            .with_setting(ShellSetting::create(
                AzString::from("appearance.zoom"),
                AzString::from("Interface zoom"),
                2,
                AzString::from("Size"),
                ShellSettingValue::Slider(
                    ShellSettingNumber::create(100.0, 50.0, 200.0).with_unit(AzString::from("%")),
                ),
            ))
            .with_setting(ShellSetting::create(
                AzString::from("appearance.mode"),
                AzString::from("Mode"),
                2,
                AzString::from("Colours"),
                ShellSettingValue::Radio(ShellSettingChoice::create(
                    strs(&["Light", "Dark", "System"]),
                    2,
                )),
            ))
    }
}

#[cfg(test)]
mod settings_dialog_model_tests {
    use super::{settings_dialog_fixtures::dialog, *};

    fn changed(index: usize, value: ShellSettingValue) -> ShellSettingsEvent {
        ShellSettingsEvent::changed(index, value)
    }

    #[test]
    fn a_change_marks_the_setting_until_apply_and_cancel_drops_it() {
        let mut d = dialog();
        assert!(!d.is_dirty());
        d.apply_event(changed(0, ShellSettingValue::Toggle(false)));
        assert_eq!(d.dirty_count(), 1, "the change waits for Apply");
        assert_eq!(
            d.value_of(AzString::from("general.restore")),
            OptionShellSettingValue::Some(ShellSettingValue::Toggle(true)),
            "the old value is still in effect"
        );
        d.apply_event(ShellSettingsEvent::create(ShellSettingsEventKind::Apply, 0));
        assert!(!d.is_dirty());
        assert_eq!(
            d.value_of(AzString::from("general.restore")),
            OptionShellSettingValue::Some(ShellSettingValue::Toggle(false))
        );
        d.apply_event(changed(4, ShellSettingValue::Text(AzString::from("Anna"))));
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::Cancel,
            0,
        ));
        assert!(!d.is_dirty());
        assert_eq!(
            d.settings.as_ref()[4].value,
            ShellSettingValue::Text(AzString::from("Felix")),
            "Cancel puts the shown value back"
        );
    }

    #[test]
    fn an_instant_dialog_puts_a_change_in_effect_at_once() {
        let mut d = dialog().with_apply_mode(ShellSettingsApplyMode::Instant);
        d.apply_event(changed(0, ShellSettingValue::Toggle(false)));
        assert!(!d.is_dirty());
        assert_eq!(
            d.value_of(AzString::from("general.restore")),
            OptionShellSettingValue::Some(ShellSettingValue::Toggle(false))
        );
    }

    #[test]
    fn a_setting_that_requires_a_restart_raises_the_notice_once_it_took_effect() {
        let mut d = dialog();
        let options = match &d.settings.as_ref()[1].value {
            ShellSettingValue::Choice(c) => c.options.clone(),
            other => panic!("the language is a choice, not {other:?}"),
        };
        let german = ShellSettingValue::Choice(ShellSettingChoice::create(options, 1));
        d.apply_event(changed(1, german));
        assert!(!d.restart_pending, "not before it takes effect");
        d.apply_event(ShellSettingsEvent::create(ShellSettingsEventKind::Ok, 0));
        assert!(d.restart_pending);
    }

    #[test]
    fn restore_defaults_resets_the_category_only() {
        let mut d = dialog();
        d.apply_event(changed(0, ShellSettingValue::Toggle(false)));
        d.apply_event(changed(4, ShellSettingValue::Text(AzString::from("Anna"))));
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::RestoreDefaults,
            0,
        ));
        assert!(d.settings.as_ref()[0].is_default());
        assert!(
            !d.settings.as_ref()[4].is_default(),
            "another category keeps its change"
        );
    }

    #[test]
    fn a_category_clears_the_search_and_a_search_counts_matches_per_category() {
        let mut d = dialog();
        let mut search = ShellSettingsEvent::create(ShellSettingsEventKind::SearchChanged, 0);
        search.text = AzString::from("zoom");
        d.apply_event(search);
        assert_eq!(d.search.as_str(), "zoom");
        assert_eq!(
            (d.matches_in(0), d.matches_in(1), d.matches_in(2)),
            (0, 1, 1),
            "Font size by its keywords, Interface zoom by its label"
        );
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::CategoryChosen,
            2,
        ));
        assert_eq!((d.active_category, d.search.as_str()), (2, ""));
    }

    #[test]
    fn a_shortcut_listens_until_it_records_or_stops() {
        let mut d = dialog();
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::StartRecording,
            5,
        ));
        assert_eq!(d.recording, 5);
        d.apply_event(changed(
            5,
            ShellSettingValue::Shortcut(ShellSettingShortcut::none()),
        ));
        assert_eq!(
            d.recording, NOT_RECORDING,
            "a recorded (or cleared) shortcut stops"
        );
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::StartRecording,
            5,
        ));
        d.apply_event(ShellSettingsEvent::create(
            ShellSettingsEventKind::StopRecording,
            0,
        ));
        assert_eq!(d.recording, NOT_RECORDING);
    }

    #[test]
    fn a_value_reads_as_a_person_says_it() {
        assert_eq!(
            ShellSettingValue::Toggle(true).display_text().as_str(),
            "On"
        );
        assert_eq!(
            ShellSettingValue::Number(
                ShellSettingNumber::create(12.0, 6.0, 72.0).with_unit(AzString::from("pt"))
            )
            .display_text()
            .as_str(),
            "12 pt"
        );
        assert_eq!(
            ShellSettingValue::Slider(ShellSettingNumber::create(1.5, 0.0, 2.0))
                .display_text()
                .as_str(),
            "1.5"
        );
        assert_eq!(
            ShellSettingValue::Color(ColorU::new(47, 74, 133, 255))
                .display_text()
                .as_str(),
            "#2f4a85"
        );
        assert_eq!(
            ShellSettingValue::Shortcut(ShellSettingShortcut::none())
                .display_text()
                .as_str(),
            "None"
        );
        assert_eq!(
            dialog().settings.as_ref()[8].value.display_text().as_str(),
            "System"
        );
        assert_eq!(dialog().settings.as_ref()[8].value.as_index(), 2);
        assert!(dialog().settings.as_ref()[0].value.as_bool());
        assert_eq!(dialog().settings.as_ref()[3].value.as_number(), 12.0);
    }
}

#[cfg(test)]
mod settings_dialog_build_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::{settings_dialog_fixtures::dialog, *};
    use crate::widgets::{
        dialog_kit::{HELD_CLASS, MARK_CLASS},
        path_input::PATH_INPUT_CLASS,
        roving::test_support as rv,
        shells::settings_layout::{
            CATEGORY_BADGE_CLASS, CATEGORY_CLASS, FOOTER_CLASS, SECTION_CLASS,
        },
        shortcut_recorder::RECORDER_CLASS,
        themes::{theme_blocks::checks, theme_checks as tc},
    };

    type Log = Arc<Mutex<Vec<ShellSettingsEvent>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: ShellSettingsEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn texts(node: &Dom) -> Vec<String> {
        tc::nodes(node)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) if !s.as_str().is_empty() => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn section_names(dom: &Dom) -> Vec<String> {
        tc::find_all(dom, SECTION_CLASS)
            .iter()
            .filter_map(|s| {
                s.root.get_accessibility_info().and_then(|i| {
                    i.accessibility_name
                        .as_ref()
                        .map(|n| n.as_str().to_string())
                })
            })
            .collect()
    }

    fn id(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    /// The first node at or above the text `label` that takes a click.
    fn clickable(styled: &StyledDom, label: &str) -> usize {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let text = nodes
            .iter()
            .position(|n| matches!(n.get_node_type(), NodeType::Text(s) if s.as_str() == label))
            .unwrap_or_else(|| panic!("no text {label:?}"));
        let mut node = NodeId::new(text);
        loop {
            if nodes[node.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click))
            {
                return node.index();
            }
            node = hierarchy[node.index()]
                .parent_id()
                .unwrap_or_else(|| panic!("nothing above {label:?} takes a click"));
        }
    }

    #[test]
    fn the_active_category_shows_its_settings_in_their_sections() {
        for theme in checks::BOTH {
            let dom = dialog().with_theme(theme).dom();
            assert_eq!(
                section_names(&dom),
                vec!["Startup", "Region", "Files"],
                "{}",
                theme.name()
            );
            let rows = tc::find_all(&dom, SETTING_ROW_CLASS);
            assert_eq!(rows.len(), 3, "{}: General's three settings", theme.name());
            let first = texts(rows[0]);
            assert_eq!(first[0], "Reopen the last documents");
            assert!(
                first
                    .iter()
                    .any(|t| t == "Open what was open when AzOffice closed."),
                "the help line"
            );
            let badge =
                tc::find(rows[1], SETTING_RESTART_CLASS).expect("Language requires a restart");
            assert_eq!(texts(badge), vec!["Requires restart"]);
            assert!(tc::find(rows[0], SETTING_RESTART_CLASS).is_none());
            assert!(
                tc::find(rows[2], PATH_INPUT_CLASS).is_some(),
                "a path is a PathInput"
            );
        }
    }

    #[test]
    fn every_kind_of_value_gets_its_control() {
        let editor = dialog()
            .with_active_category(1)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(
            tc::find(&editor, RECORDER_CLASS).is_some(),
            "a shortcut is recorded"
        );
        let all = texts(&editor);
        assert!(
            all.iter().any(|t| t == "pt"),
            "a number shows its unit: {all:?}"
        );
        assert!(all.iter().any(|t| t == "Ctrl+Shift+P"), "the shortcut");
        let appearance = dialog()
            .with_active_category(2)
            .with_theme(UiTheme::Flat)
            .dom();
        let all = texts(&appearance);
        assert!(
            all.iter().any(|t| t == "100 %"),
            "a slider reads its value out: {all:?}"
        );
        for option in ["Light", "Dark", "System"] {
            assert!(
                all.iter().any(|t| t == option),
                "the radio set shows {option}"
            );
        }
    }

    #[test]
    fn a_search_finds_settings_in_every_category_marks_them_and_counts_them() {
        for theme in checks::BOTH {
            let dom = dialog()
                .with_search(AzString::from("zoom"))
                .with_theme(theme)
                .dom();
            assert_eq!(
                section_names(&dom),
                vec!["Editor: Text", "Appearance: Size"],
                "{}",
                theme.name()
            );
            let marks = tc::find_all(&dom, MARK_CLASS);
            assert_eq!(marks.len(), 1, "{}: Interface zoom is marked", theme.name());
            assert_eq!(texts(marks[0]), vec!["zoom"]);
            let cats = tc::find_all(&dom, CATEGORY_CLASS);
            let badges: Vec<Vec<String>> = cats
                .iter()
                .map(|c| {
                    tc::find(c, CATEGORY_BADGE_CLASS)
                        .map(texts)
                        .unwrap_or_default()
                })
                .collect();
            assert_eq!(
                badges,
                vec![vec![], vec!["1".to_string()], vec!["1".to_string()]],
                "{}",
                theme.name()
            );
        }
        let nothing = dialog()
            .with_search(AzString::from("qqq"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            section_names(&nothing),
            vec!["No settings match the search."]
        );
    }

    #[test]
    fn a_changed_setting_is_marked_and_apply_waits_for_a_change() {
        let fresh = dialog().with_theme(UiTheme::Flat).dom();
        let footer = tc::find(&fresh, FOOTER_CLASS).expect("the button row");
        assert_eq!(
            texts(footer),
            vec!["Restore defaults", "OK", "Cancel", "Apply"]
        );
        assert_eq!(
            tc::find_all(footer, HELD_CLASS).len(),
            1,
            "Apply is inert while nothing changed"
        );
        assert!(tc::find(&fresh, SETTING_MODIFIED_CLASS).is_none());

        let mut changed = dialog();
        changed.apply_event(ShellSettingsEvent::changed(
            0,
            ShellSettingValue::Toggle(false),
        ));
        let dom = changed.with_theme(UiTheme::Flat).dom();
        let rows = tc::find_all(&dom, SETTING_ROW_CLASS);
        assert!(
            tc::find(rows[0], SETTING_MODIFIED_CLASS).is_some(),
            "the changed row is marked"
        );
        assert!(tc::find(rows[1], SETTING_MODIFIED_CLASS).is_none());
        let footer = tc::find(&dom, FOOTER_CLASS).expect("the button row");
        assert!(tc::find_all(footer, HELD_CLASS).is_empty(), "Apply goes");
    }

    #[test]
    fn an_instant_dialog_has_no_ok_cancel_or_apply_and_a_pending_restart_says_so() {
        let mut d = dialog().with_apply_mode(ShellSettingsApplyMode::Instant);
        let dom = d.clone().with_theme(UiTheme::Flora).dom();
        let footer = tc::find(&dom, FOOTER_CLASS).expect("the button row");
        assert_eq!(texts(footer), vec!["Restore defaults"]);
        assert!(tc::find(&dom, SETTINGS_NOTICE_CLASS).is_none());
        d.restart_pending = true;
        let dom = d.with_theme(UiTheme::Flora).dom();
        let notice = tc::find(&dom, SETTINGS_NOTICE_CLASS).expect("the notice");
        assert_eq!(texts(notice), vec!["Restart to apply some changes."]);
    }

    #[test]
    fn the_buttons_and_a_toggle_report_through_the_one_callback() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let d = dialog()
            .with_active_category(0)
            .with_on_event(
                RefAny::new(log.clone()),
                record as ShellSettingsDialogOnEventCallbackType,
            )
            .with_theme(UiTheme::Flat);
        let styled = StyledDom::create_from_dom(d.clone().dom());
        for label in ["OK", "Restore defaults", "Cancel"] {
            rv::fire(
                &styled,
                id(clickable(&styled, label)),
                EventFilter::Hover(HoverEventFilter::Click),
            )
            .unwrap_or_else(|| panic!("{label} takes the click"));
        }
        // The toggle: the switch named by the setting's label.
        let nodes = styled.node_data.as_ref();
        let switch = nodes
            .iter()
            .position(|n| {
                n.get_accessibility_info().is_some_and(|i| {
                    i.accessibility_name.as_ref().map(|s| s.as_str())
                        == Some("Reopen the last documents")
                }) && n
                    .get_callbacks()
                    .as_ref()
                    .iter()
                    .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click))
            })
            .expect("the switch");
        rv::fire(
            &styled,
            id(switch),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("toggle");
        let got = log.lock().expect("log").clone();
        let kinds: Vec<ShellSettingsEventKind> = got.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ShellSettingsEventKind::Ok,
                ShellSettingsEventKind::RestoreDefaults,
                ShellSettingsEventKind::Cancel,
                ShellSettingsEventKind::Changed
            ]
        );
        assert_eq!(got[1].index, 0, "the shown category's defaults");
        assert_eq!(
            (got[3].index, got[3].value.clone()),
            (0, ShellSettingValue::Toggle(false))
        );
    }

    #[test]
    fn an_unpinned_dialog_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "settings_dialog",
            || dialog().dom(),
            |t: UiTheme| dialog().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "settings_dialog (search)",
            || dialog().with_search(AzString::from("zoom")).dom(),
            |t: UiTheme| {
                dialog()
                    .with_search(AzString::from("zoom"))
                    .with_theme(t)
                    .dom()
            },
        );
    }
}
