//! `<input type=datetime-local>`: a date and a time in one control.
//!
//! COMPOSED, not forked: a [`DatePicker`] and a [`TimePicker`] side by side,
//! each wired to a small part handler that folds its part into ONE combined
//! [`DateTimeLocalPickerState`] and reports that to the app. The calendar,
//! the spinners and their keyboard handling are the existing widgets'.
//!
//! The value (for a [`crate::widgets::form::Form`] and for the app) is HTML's
//! `YYYY-MM-DDTHH:MM`, the time in canonical 24-hour form whatever the time
//! part displays.
//!
//! Key types: [`DateTimeLocalPicker`], [`DateTimeLocalPickerState`],
//! [`DateTimeLocalPickerOnChange`].

use alloc::string::String;

use azul_core::{
    callbacks::Update,
    dom::{AttributeType, Dom},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::OptionCssPropertyWithConditionsVec, impl_option_inner, AzString, OptionString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        date_picker::{DatePicker, DatePickerOnChangeCallbackType, DatePickerState},
        time_picker::{TimePicker, TimePickerOnChangeCallbackType, TimePickerState},
    },
};

/// The class of the row holding the date part and the time part.
pub const DATETIME_LOCAL_CLASS: &str = "__azul-native-datetime-local";

/// Callback type invoked when the date part or the time part changes; it is
/// handed the COMBINED state.
pub type DateTimeLocalPickerOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DateTimeLocalPickerState) -> Update;
impl_widget_callback!(
    DateTimeLocalPickerOnChange,
    OptionDateTimeLocalPickerOnChange,
    DateTimeLocalPickerOnChangeCallback,
    DateTimeLocalPickerOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DateTimeLocalPickerOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DATE_TIME_LOCAL_PICKER_ON_CHANGE_INVOKER,
    invoker_ty:     AzDateTimeLocalPickerOnChangeCallbackInvoker,
    thunk_fn:       az_date_time_local_picker_on_change_callback_thunk,
    setter_fn:      AzApp_setDateTimeLocalPickerOnChangeCallbackInvoker,
    from_handle_fn: AzDateTimeLocalPickerOnChangeCallback_createFromHostHandle,
    from_handle_byref_fn: AzDateTimeLocalPickerOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ state: DateTimeLocalPickerState ],
}

/// The combined value of a [`DateTimeLocalPicker`]: its date part and its
/// time part.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateTimeLocalPickerState {
    pub date: DatePickerState,
    pub time: TimePickerState,
}

impl DateTimeLocalPickerState {
    /// HTML's `datetime-local` value: `YYYY-MM-DDTHH:MM`, the hour in
    /// canonical 24-hour form even when the time part shows AM/PM.
    #[must_use]
    pub fn to_html_value(&self) -> String {
        alloc::format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}",
            self.date.year,
            self.date.month,
            self.date.day,
            self.time.canonical_hour(),
            self.time.minute
        )
    }
}

/// [`DateTimeLocalPickerState`] with the app's change callback. This is the
/// state both part handlers share, and the row's dataset (how a `Form` reads
/// the value).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateTimeLocalPickerStateWrapper {
    pub inner: DateTimeLocalPickerState,
    pub on_change: OptionDateTimeLocalPickerOnChange,
}

/// `<input type=datetime-local>`: a [`DatePicker`] and a [`TimePicker`] in
/// one row, reporting one combined value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateTimeLocalPicker {
    pub state: DateTimeLocalPickerStateWrapper,
    /// Style for the row, or `None` for the theme's.
    pub container_style: OptionCssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    pub accessibility_name: OptionString,
    /// The HTML `name` the value is submitted under in a form.
    pub name: OptionString,
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl Default for DateTimeLocalPicker {
    fn default() -> Self {
        Self::create(2000, 1, 1, 0, 0)
    }
}

impl DateTimeLocalPicker {
    /// A picker showing `year-month-day` at `hour:minute` (24-hour), each
    /// clamped the way the date and time parts clamp them.
    #[must_use]
    pub fn create(year: u32, month: u32, day: u32, hour: u32, minute: u32) -> Self {
        // The parts' own constructors do the clamping, so the combined state
        // can never hold what a part would refuse to show.
        let date = DatePicker::create(year, month, day).state.inner;
        let time = TimePicker::create(hour, minute).state.inner;
        Self {
            state: DateTimeLocalPickerStateWrapper {
                inner: DateTimeLocalPickerState { date, time },
                on_change: None.into(),
            },
            container_style: OptionCssPropertyWithConditionsVec::None,
            accessibility_name: OptionString::None,
            name: OptionString::None,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Show the time part in 12-hour form (AM/PM); the value stays 24-hour.
    #[must_use]
    pub fn with_24h(mut self, is_24h: bool) -> Self {
        let mut time = TimePicker::create(0, 0);
        time.state.inner = self.state.inner.time;
        time.set_24h(is_24h);
        self.state.inner.time = time.state.inner;
        self
    }

    /// Sets the callback invoked when either part changes.
    pub fn set_on_change<C: Into<DateTimeLocalPickerOnChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.state.on_change = Some(DateTimeLocalPickerOnChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder variant of [`Self::set_on_change`].
    #[must_use]
    pub fn with_on_change<C: Into<DateTimeLocalPickerOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// The name the value is submitted under in a form.
    pub fn set_name(&mut self, name: AzString) {
        self.name = Some(name).into();
    }

    /// [`Self::set_name`] for the builder chain.
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.set_name(name);
        self
    }

    /// Pick the widget theme; unset, the default theme renders it.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the row: the date part, then the time part, each the existing
    /// widget, each reporting into the one shared state.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, UiTheme};

        let inner = self.state.inner;
        let shared = RefAny::new(self.state);
        // The row's theme is its parts' theme.
        let theme = self.theme.into_option().unwrap_or(UiTheme::Flat);

        let date = DatePicker::create(inner.date.year, inner.date.month, inner.date.day)
            .with_theme(theme)
            .with_on_change(
                shared.clone(),
                on_date_part_change as DatePickerOnChangeCallbackType,
            )
            .with_accessibility_name(AzString::from_const_str("Date"))
            .dom();
        let mut time_part = TimePicker::create(0, 0);
        time_part.state.inner = inner.time;
        let time = time_part
            .with_theme(theme)
            .with_on_change(
                shared.clone(),
                on_time_part_change as TimePickerOnChangeCallbackType,
            )
            .with_accessibility_name(AzString::from_const_str("Time"))
            .dom();

        let mut row = match theme {
            UiTheme::Flat => flat::datetime_local(date, time),
            UiTheme::Flora => flora::datetime_local(date, time),
        };
        if let Some(style) = self.container_style.into_option() {
            row = row.with_css_props(style);
        }
        let mut row = row
            .with_dataset(Some(shared).into())
            .with_attribute(AttributeType::InputType(AzString::from_const_str(
                "datetime-local",
            )))
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Grouping,
                accessibility_name: self.accessibility_name,
                accessibility_value: Some(AzString::from(inner.to_html_value())).into(),
                ..Default::default()
            });
        if let Some(name) = self.name.into_option() {
            row = row.with_attribute(AttributeType::Name(name));
        }
        row
    }
}

impl From<DateTimeLocalPicker> for Dom {
    fn from(p: DateTimeLocalPicker) -> Self {
        p.dom()
    }
}

/// The date part changed: fold it into the combined state and report that.
pub(crate) extern "C" fn on_date_part_change(
    mut data: RefAny,
    info: CallbackInfo,
    date: DatePickerState,
) -> Update {
    let Some(mut w) = data.downcast_mut::<DateTimeLocalPickerStateWrapper>() else {
        return Update::DoNothing;
    };
    w.inner.date = date;
    report(&mut w, info)
}

/// The time part changed: fold it into the combined state and report that.
pub(crate) extern "C" fn on_time_part_change(
    mut data: RefAny,
    info: CallbackInfo,
    time: TimePickerState,
) -> Update {
    let Some(mut w) = data.downcast_mut::<DateTimeLocalPickerStateWrapper>() else {
        return Update::DoNothing;
    };
    w.inner.time = time;
    report(&mut w, info)
}

/// Hand the app the combined state.
fn report(w: &mut DateTimeLocalPickerStateWrapper, info: CallbackInfo) -> Update {
    let inner = w.inner;
    match w.on_change.as_mut() {
        Some(DateTimeLocalPickerOnChange { callback, refany }) => {
            callback.invoke(refany.clone(), info, inner)
        }
        None => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{AttributeType, Dom, DomId, DomNodeId, IdOrClass, NodeId},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::{OptionRefAny, RefAny},
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::{
            date_picker::DatePickerState,
            theme_probe,
            themes::{OptionUiTheme, UiTheme},
            time_picker::TimePickerState,
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// Runs `f` with a real `CallbackInfo` over an empty window: the part
    /// handlers never look at the DOM, only at their payload.
    fn with_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> R {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window.layout_results.insert(
            DomId::ROOT_ID,
            DomLayoutResult {
                styled_dom: StyledDom::default(),
                layout_tree: LayoutTree {
                    nodes: Vec::new(),
                    warm: Vec::new(),
                    cold: Vec::new(),
                    root: 0,
                    dom_to_layout: BTreeMap::new(),
                    children_arena: Vec::new(),
                    children_offsets: Vec::new(),
                    subtree_needs_intrinsic: Vec::new(),
                },
                calculated_positions: Vec::new(),
                viewport: LogicalRect::zero(),
                display_list: Arc::new(DisplayList::default()),
                scroll_ids: HashMap::new(),
                scroll_id_to_node_id: HashMap::new(),
            },
        );
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };
        let changes = Arc::new(Mutex::new(Vec::new()));
        let hit = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        };
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        f(info)
    }

    struct Seen(Vec<DateTimeLocalPickerState>);

    extern "C" fn record(
        mut data: RefAny,
        _: CallbackInfo,
        state: DateTimeLocalPickerState,
    ) -> Update {
        if let Some(mut seen) = data.downcast_mut::<Seen>() {
            seen.0.push(state);
        }
        Update::RefreshDom
    }

    fn seen(log: &RefAny) -> Vec<DateTimeLocalPickerState> {
        let mut log = log.clone();
        let entries = log.downcast_ref::<Seen>().expect("the log changed type").0.clone();
        entries
    }

    fn shared_of(dom: &Dom) -> RefAny {
        dom.root
            .get_dataset()
            .cloned()
            .expect("the picker carries its state as its dataset")
    }

    fn state_of(shared: &RefAny) -> DateTimeLocalPickerState {
        let mut shared = shared.clone();
        let inner = shared
            .downcast_ref::<DateTimeLocalPickerStateWrapper>()
            .expect("the dataset is the picker's state")
            .inner;
        inner
    }

    #[test]
    fn a_datetime_local_picker_is_a_date_picker_and_a_time_picker_in_one_row() {
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 2, "one date part, one time part");
        assert!(classes(&kids[0]).contains(&"__azul-native-date-picker".to_string()));
        assert!(classes(&kids[1]).contains(&"__azul-native-time-picker".to_string()));
        assert!(classes(&dom).contains(&DATETIME_LOCAL_CLASS.to_string()));
    }

    #[test]
    fn its_value_is_the_html_datetime_local_string() {
        let state = state_of(&shared_of(&DateTimeLocalPicker::create(2026, 9, 29, 14, 5).dom()));
        assert_eq!(state.to_html_value(), "2026-09-29T14:05");

        // A 12-hour display still submits the canonical 24-hour time.
        let mut s = state;
        s.time = TimePickerState {
            hour: 2,
            minute: 5,
            is_pm: true,
            is_24h: false,
        };
        assert_eq!(s.to_html_value(), "2026-09-29T14:05");
    }

    #[test]
    fn it_carries_its_state_name_and_type_for_a_form() {
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_name("when".into())
            .dom();
        let _ = state_of(&shared_of(&dom));
        let attrs = dom.root.attributes();
        assert!(attrs
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::Name(n) if n.as_str() == "when")));
        assert!(attrs.as_ref().iter().any(
            |a| matches!(a, AttributeType::InputType(t) if t.as_str() == "datetime-local")
        ));
    }

    #[test]
    fn a_date_part_change_updates_the_combined_state_and_reports_it() {
        let log = RefAny::new(Seen(Vec::new()));
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_on_change(log.clone(), record as DateTimeLocalPickerOnChangeCallbackType)
            .dom();
        let shared = shared_of(&dom);
        let update = with_info(|info| {
            on_date_part_change(
                shared.clone(),
                info,
                DatePickerState {
                    year: 2026,
                    month: 10,
                    day: 1,
                },
            )
        });
        assert_eq!(update, Update::RefreshDom, "the app's verdict");
        let s = state_of(&shared);
        assert_eq!((s.date.year, s.date.month, s.date.day), (2026, 10, 1));
        assert_eq!((s.time.hour, s.time.minute), (14, 5), "the time part is kept");
        assert_eq!(
            seen(&log).last().map(DateTimeLocalPickerState::to_html_value).as_deref(),
            Some("2026-10-01T14:05")
        );
    }

    #[test]
    fn a_time_part_change_keeps_the_date() {
        let log = RefAny::new(Seen(Vec::new()));
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_on_change(log.clone(), record as DateTimeLocalPickerOnChangeCallbackType)
            .dom();
        let shared = shared_of(&dom);
        let _ = with_info(|info| {
            on_time_part_change(
                shared.clone(),
                info,
                TimePickerState {
                    hour: 9,
                    minute: 30,
                    is_pm: false,
                    is_24h: true,
                },
            )
        });
        assert_eq!(state_of(&shared).to_html_value(), "2026-09-29T09:30");
        assert_eq!(seen(&log).len(), 1);
    }

    /// The row's theme is its parts' theme: a flora datetime-local renders its
    /// date AND its time picker in flora (both carry the flora marker class),
    /// not a flora row around two flat parts.
    #[test]
    fn a_flora_row_renders_its_date_and_time_parts_in_flora() {
        fn has_flora_marker(dom: &Dom) -> bool {
            dom.root.has_class("__azul-theme-flora")
                || dom.children.as_ref().iter().any(has_flora_marker)
        }
        let row = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_theme(UiTheme::Flora)
            .dom();
        let parts: Vec<&Dom> = row.children.as_ref().iter().collect();
        assert!(parts.len() >= 2, "a date part and a time part");
        for (i, part) in parts.iter().take(2).enumerate() {
            assert!(has_flora_marker(part), "part {i} is not flora");
        }
    }

    #[test]
    fn both_themes_style_the_control_in_light_and_dark() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let mut picker = DateTimeLocalPicker::create(2026, 9, 29, 14, 5);
            picker.theme = OptionUiTheme::Some(theme);
            let dom = picker.dom();
            assert!(
                !theme_probe::dark(&dom).is_empty(),
                "{theme:?}: no dark-mode declarations on the datetime-local row"
            );
        }
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_theme(UiTheme::Flora)
            .dom();
        assert_eq!(
            state_of(&shared_of(&dom)).to_html_value(),
            "2026-09-29T14:05"
        );
    }
}
