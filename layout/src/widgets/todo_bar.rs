//! To-Do bar widget - the right column of a mail window: a mini month
//! calendar with today ringed, the upcoming appointments ("No upcoming
//! appointments." when there are none), a line to type a new task into and
//! the task list, each task with a box to check it off. Outlook 2010's
//! To-Do bar.
//!
//! The bar shows what it is given and owns nothing: the app keeps the
//! calendar's month and day, the appointments and the tasks, hears a pick
//! (`on_pick`), a new or checked task (`on_task`) and an appointment click
//! (`on_appointment`), and rebuilds. The parts are the toolkit's own
//! widgets: the [`DatePicker`] inline for the calendar, a [`TextInput`] for
//! the new task, a [`CheckBox`] per task, link [`Button`]s for what opens.
//! For assistive technology the bar is a group named by its accessibility
//! name ("To-Do bar").
//!
//! Key types: [`ToDoBar`], [`ToDoTask`], [`ToDoBarEvent`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, OptionString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{Button, ButtonOnClickCallbackType, ButtonType},
        check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState},
        date_picker::{
            DatePicker, DatePickerOnChangeCallbackType, DatePickerState, DatePickerWeekStart,
            OptionDatePickerState,
        },
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnVirtualKeyDownCallbackType, TextInputState,
            TextInputValid,
        },
    },
};

static BAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-todo-bar"))];
static CALENDAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-calendar",
))];
static APPOINTMENTS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-appointments",
))];
static APPOINTMENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-appointment",
))];
static EMPTY_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-empty",
))];
static TASK_INPUT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-task-input",
))];
static TASKS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-tasks",
))];
static TASK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-task",
))];
static TASK_DONE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-task-done",
))];
static TASK_TITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-task-title",
))];
static TASK_DUE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-todo-bar-task-due",
))];

/// One task of the list.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToDoTask {
    /// The task's id, reported back with every action on it.
    pub id: u64,
    /// What to do.
    pub title: AzString,
    /// When ("Today", "Fri"), or empty for no date.
    pub due: AzString,
    /// Checked off.
    pub done: bool,
}

impl ToDoTask {
    /// An open task `id` titled `title`, with no date.
    #[must_use]
    pub fn create(id: u64, title: AzString) -> Self {
        Self {
            id,
            title,
            due: AzString::from_const_str(""),
            done: false,
        }
    }

    /// When the task is due.
    #[must_use]
    pub fn with_due(mut self, due: AzString) -> Self {
        self.due = due;
        self
    }

    /// Checked off.
    #[must_use]
    pub const fn with_done(mut self, done: bool) -> Self {
        self.done = done;
        self
    }
}

impl_option!(
    ToDoTask,
    OptionToDoTask,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    ToDoTask,
    ToDoTaskVec,
    ToDoTaskVecDestructor,
    ToDoTaskVecDestructorType,
    ToDoTaskVecSlice,
    OptionToDoTask
);
impl_vec_clone!(ToDoTask, ToDoTaskVec, ToDoTaskVecDestructor);
impl_vec_debug!(ToDoTask, ToDoTaskVec);
impl_vec_mut!(ToDoTask, ToDoTaskVec);

/// What happened in the bar.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToDoBarEventKind {
    /// A day was picked in the calendar (`date`); the arrows and PageUp /
    /// PageDown turn the month the same way.
    DatePicked,
    /// Enter in the task line: `text` is the new task.
    TaskAdded,
    /// A task's box was checked or unchecked (`index`, `id`).
    TaskToggled,
    /// A task's title was clicked (`index`, `id`): open it.
    TaskOpened,
    /// An appointment was clicked (`index`, `text`): open it.
    AppointmentOpened,
}

/// One action in the bar.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToDoBarEvent {
    /// The new task's text (`TaskAdded`), the appointment
    /// (`AppointmentOpened`); empty otherwise.
    pub text: AzString,
    /// The task's id (`TaskToggled`, `TaskOpened`); 0 otherwise.
    pub id: u64,
    /// The task (`TaskToggled`, `TaskOpened`) or the appointment
    /// (`AppointmentOpened`); 0 otherwise.
    pub index: usize,
    /// The calendar's date (`DatePicked`); the calendar's current date
    /// otherwise.
    pub date: DatePickerState,
    /// What happened.
    pub kind: ToDoBarEventKind,
}

/// Callback invoked for an action in the bar.
pub type ToDoBarOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, ToDoBarEvent) -> Update;
impl_widget_callback!(
    ToDoBarOnEvent,
    OptionToDoBarOnEvent,
    ToDoBarOnEventCallback,
    ToDoBarOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ToDoBarOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TODO_BAR_ON_EVENT_INVOKER,
    invoker_ty:     AzToDoBarOnEventCallbackInvoker,
    thunk_fn:       az_todo_bar_on_event_callback_thunk,
    setter_fn:      AzApp_setToDoBarOnEventCallbackInvoker,
    from_handle_fn: AzToDoBarOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzToDoBarOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ToDoBarEvent ],
}

/// The To-Do bar: a calendar, the appointments, a task line and the tasks.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ToDoBar {
    /// The upcoming appointments ("14:00 Team sync"), in order.
    pub appointments: StringVec,
    /// What the appointments section says when there are none.
    pub appointments_empty: AzString,
    /// The task line's prompt when it is empty ("Type a new task").
    pub task_placeholder: AzString,
    /// The task line's text.
    pub task_text: AzString,
    /// The tasks, in order.
    pub tasks: ToDoTaskVec,
    /// What the bar is CALLED, for assistive technology; "To-Do bar" when
    /// unset.
    pub accessibility_name: OptionString,
    /// A day was picked.
    pub on_pick: OptionToDoBarOnEvent,
    /// A task was added, checked or opened.
    pub on_task: OptionToDoBarOnEvent,
    /// An appointment was clicked.
    pub on_appointment: OptionToDoBarOnEvent,
    /// The calendar: the month shown and the day picked.
    pub calendar: DatePickerState,
    /// Today, ringed in the calendar; `None` rings nothing.
    pub today: OptionDatePickerState,
    /// The weekday the calendar's rows start on (Sunday unless set; a
    /// calendar app passes its own, so the bar's weeks are its weeks).
    pub week_start: DatePickerWeekStart,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about a To-Do bar: the SKIN of each part, laid over
/// the part's base (the bar's structure, the same in every theme:
/// `TODO_BAR_*_BASE`) by [`build`].
pub(crate) struct ToDoBarLook {
    /// The bar.
    pub bar: Vec<CssPropertyWithConditions>,
    /// The box around the calendar.
    pub calendar: Vec<CssPropertyWithConditions>,
    /// The appointments section.
    pub appointments: Vec<CssPropertyWithConditions>,
    /// One appointment's row.
    pub appointment: Vec<CssPropertyWithConditions>,
    /// The "no appointments" line.
    pub empty: Vec<CssPropertyWithConditions>,
    /// The box around the task line.
    pub task_input: Vec<CssPropertyWithConditions>,
    /// The task list.
    pub tasks: Vec<CssPropertyWithConditions>,
    /// One task's row.
    pub task: Vec<CssPropertyWithConditions>,
    /// Added to a task's row when it is done.
    pub task_done: Vec<CssPropertyWithConditions>,
    /// The box around a task's title.
    pub task_title: Vec<CssPropertyWithConditions>,
    /// A task's due date.
    pub task_due: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the bar, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the bar's structure, in every theme ----

/// The bar and its sections: columns that never grow past their content.
pub(crate) static TODO_BAR_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A row of the bar (an appointment, a task): its parts on one midline,
/// its text never selected by a drag.
pub(crate) static TODO_BAR_ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A part that takes the rest of its row.
pub(crate) static TODO_BAR_GROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A part that keeps its size.
pub(crate) static TODO_BAR_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl ToDoBar {
    /// A bar whose calendar shows `year`/`month` with `day` picked, with no
    /// appointments and no tasks.
    #[must_use]
    pub fn create(year: u32, month: u32, day: u32) -> Self {
        Self {
            appointments: StringVec::from_const_slice(&[]),
            appointments_empty: AzString::from_const_str("No upcoming appointments."),
            task_placeholder: AzString::from_const_str("Type a new task"),
            task_text: AzString::from_const_str(""),
            tasks: ToDoTaskVec::from_const_slice(&[]),
            accessibility_name: OptionString::None,
            on_pick: None.into(),
            on_task: None.into(),
            on_appointment: None.into(),
            calendar: DatePickerState { year, month, day },
            today: OptionDatePickerState::None,
            week_start: DatePickerWeekStart::Sunday,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Today, ringed in the calendar.
    pub const fn set_today(&mut self, year: u32, month: u32, day: u32) {
        self.today = OptionDatePickerState::Some(DatePickerState { year, month, day });
    }

    /// [`Self::set_today`] for the builder chain.
    #[must_use]
    pub const fn with_today(mut self, year: u32, month: u32, day: u32) -> Self {
        self.set_today(year, month, day);
        self
    }

    /// The weekday the calendar's rows start on (`DatePicker::set_week_start`).
    pub const fn set_week_start(&mut self, week_start: DatePickerWeekStart) {
        self.week_start = week_start;
    }

    /// [`Self::set_week_start`] for the builder chain.
    #[must_use]
    pub const fn with_week_start(mut self, week_start: DatePickerWeekStart) -> Self {
        self.set_week_start(week_start);
        self
    }

    /// The upcoming appointments.
    pub fn set_appointments(&mut self, appointments: StringVec) {
        self.appointments = appointments;
    }

    /// [`Self::set_appointments`] for the builder chain.
    #[must_use]
    pub fn with_appointments(mut self, appointments: StringVec) -> Self {
        self.set_appointments(appointments);
        self
    }

    /// What the appointments section says when there are none.
    pub fn set_appointments_empty(&mut self, text: AzString) {
        self.appointments_empty = text;
    }

    /// [`Self::set_appointments_empty`] for the builder chain.
    #[must_use]
    pub fn with_appointments_empty(mut self, text: AzString) -> Self {
        self.set_appointments_empty(text);
        self
    }

    /// The task line's prompt and text.
    pub fn set_task_line(&mut self, placeholder: AzString, text: AzString) {
        self.task_placeholder = placeholder;
        self.task_text = text;
    }

    /// [`Self::set_task_line`] for the builder chain.
    #[must_use]
    pub fn with_task_line(mut self, placeholder: AzString, text: AzString) -> Self {
        self.set_task_line(placeholder, text);
        self
    }

    /// The tasks.
    pub fn set_tasks(&mut self, tasks: ToDoTaskVec) {
        self.tasks = tasks;
    }

    /// [`Self::set_tasks`] for the builder chain.
    #[must_use]
    pub fn with_tasks(mut self, tasks: ToDoTaskVec) -> Self {
        self.set_tasks(tasks);
        self
    }

    /// Name the bar for assistive technology.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = OptionString::Some(name);
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Pin the widget theme; unset, the bar follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// A day was picked.
    pub fn set_on_pick<C: Into<ToDoBarOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_pick = OptionToDoBarOnEvent::Some(ToDoBarOnEvent::create(data, cb));
    }

    /// [`Self::set_on_pick`] for the builder chain.
    #[must_use]
    pub fn with_on_pick<C: Into<ToDoBarOnEventCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_pick(data, cb);
        self
    }

    /// A task was added, checked or opened.
    pub fn set_on_task<C: Into<ToDoBarOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_task = OptionToDoBarOnEvent::Some(ToDoBarOnEvent::create(data, cb));
    }

    /// [`Self::set_on_task`] for the builder chain.
    #[must_use]
    pub fn with_on_task<C: Into<ToDoBarOnEventCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_task(data, cb);
        self
    }

    /// An appointment was clicked.
    pub fn set_on_appointment<C: Into<ToDoBarOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_appointment = OptionToDoBarOnEvent::Some(ToDoBarOnEvent::create(data, cb));
    }

    /// [`Self::set_on_appointment`] for the builder chain.
    #[must_use]
    pub fn with_on_appointment<C: Into<ToDoBarOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_appointment(data, cb);
        self
    }

    /// Replaces `self` with an empty bar and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(2000, 1, 1);
        core::mem::swap(&mut s, self);
        s
    }

    /// The bar's DOM. The look comes from the theme module
    /// (`themes::flat::todo_bar` / `themes::flora::todo_bar`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::todo_bar(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::todo_bar(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::todo_bar,
                crate::widgets::themes::flora::todo_bar,
            ),
        }
    }
}

impl Default for ToDoBar {
    fn default() -> Self {
        Self::create(2000, 1, 1)
    }
}

impl From<ToDoBar> for Dom {
    fn from(b: ToDoBar) -> Self {
        b.dom()
    }
}

/// What every part of one bar shares: the app's hooks and the calendar's
/// date.
struct BarShared {
    on_pick: OptionToDoBarOnEvent,
    on_task: OptionToDoBarOnEvent,
    on_appointment: OptionToDoBarOnEvent,
    calendar: DatePickerState,
}

/// Hands `event` to `hook`.
fn fire(hook: &OptionToDoBarOnEvent, info: CallbackInfo, event: ToDoBarEvent) -> Update {
    match hook.as_ref() {
        Some(ToDoBarOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// A `kind` event with the bar's own date.
fn event(shared: &BarShared, kind: ToDoBarEventKind, index: usize, id: u64) -> ToDoBarEvent {
    ToDoBarEvent {
        text: AzString::from_const_str(""),
        id,
        index,
        date: shared.calendar,
        kind,
    }
}

/// The calendar reported a date: a pick, or a turned month.
extern "C" fn on_calendar_change(mut data: RefAny, info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(shared) = data.downcast_ref::<BarShared>() else {
        return Update::DoNothing;
    };
    let mut e = event(&shared, ToDoBarEventKind::DatePicked, 0, 0);
    e.date = state;
    fire(&shared.on_pick, info, e)
}

/// What a key in the task line asks for: Enter adds the task.
#[must_use]
pub(crate) fn task_key_adds(key: Option<VirtualKeyCode>) -> bool {
    matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter))
}

extern "C" fn on_task_key(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match data.downcast_ref::<BarShared>() {
        Some(shared) if task_key_adds(key) => {
            let mut e = event(&shared, ToDoBarEventKind::TaskAdded, 0, 0);
            e.text = AzString::from(state.get_text());
            fire(&shared.on_task, info, e)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A task row's payload.
struct TaskData {
    index: usize,
    id: u64,
    shared: RefAny,
}

/// An appointment row's payload.
struct AppointmentData {
    index: usize,
    text: AzString,
    shared: RefAny,
}

extern "C" fn on_task_toggle(mut data: RefAny, info: CallbackInfo, _: CheckBoxState) -> Update {
    let (index, id, mut shared) = {
        let Some(t) = data.downcast_ref::<TaskData>() else {
            return Update::DoNothing;
        };
        (t.index, t.id, t.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<BarShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_task,
        info,
        event(&shared, ToDoBarEventKind::TaskToggled, index, id),
    )
}

extern "C" fn on_task_open(mut data: RefAny, info: CallbackInfo) -> Update {
    let (index, id, mut shared) = {
        let Some(t) = data.downcast_ref::<TaskData>() else {
            return Update::DoNothing;
        };
        (t.index, t.id, t.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<BarShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_task,
        info,
        event(&shared, ToDoBarEventKind::TaskOpened, index, id),
    )
}

extern "C" fn on_appointment_open(mut data: RefAny, info: CallbackInfo) -> Update {
    let (index, text, mut shared) = {
        let Some(a) = data.downcast_ref::<AppointmentData>() else {
            return Update::DoNothing;
        };
        (a.index, a.text.clone(), a.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<BarShared>() else {
        return Update::DoNothing;
    };
    let mut e = event(&shared, ToDoBarEventKind::AppointmentOpened, index, 0);
    e.text = text;
    fire(&shared.on_appointment, info, e)
}

/// The bar's DOM in `look`: bar [calendar, appointments [row.. | empty],
/// task line, tasks [row [box, title, due]..]]. Every part is its base (the
/// structure), then the look's skin; the calendar, the task line, the boxes
/// and the links are the toolkit's own widgets, pinned to the bar's theme
/// (or following the app theme with it).
#[allow(clippy::too_many_lines)]
pub(crate) fn build(bar: ToDoBar, look: &ToDoBarLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let ToDoBar {
        appointments,
        appointments_empty,
        task_placeholder,
        task_text,
        tasks,
        accessibility_name,
        on_pick,
        on_task,
        on_appointment,
        calendar,
        today,
        week_start,
        theme,
    } = bar;
    let theme = theme.into_option();
    let shared = RefAny::new(BarShared {
        on_pick,
        on_task,
        on_appointment,
        calendar,
    });
    // An appointment and a task's title are data a user opens: a link, in
    // flora flora's text link rather than its quiet command.
    let link = |label: AzString, data: RefAny, cb: ButtonOnClickCallbackType| {
        crate::widgets::button::data_link(
            crate::widgets::button::DataLink {
                label,
                data,
                on_click: cb,
            },
            theme,
        )
    };

    // The calendar: the date picker, inline, today ringed.
    let mut picker = DatePicker::create(calendar.year, calendar.month, calendar.day)
        .with_inline(true)
        .with_week_start(week_start)
        .with_accessibility_name("Calendar")
        .with_on_change(shared.clone(), on_calendar_change as DatePickerOnChangeCallbackType);
    if let Some(t) = today.into_option() {
        picker = picker.with_today(t.year, t.month, t.day);
    }
    if let Some(theme) = theme {
        picker = picker.with_theme(theme);
    }
    let calendar_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CALENDAR_CLASS))
        .with_css_props(part(TODO_BAR_COLUMN_BASE, &look.calendar))
        .with_child(picker.dom());

    // The appointments, or the one line saying there are none.
    let appointment_rows: Vec<Dom> = if appointments.as_ref().is_empty() {
        alloc::vec![crate::widgets::widget_p_with_text(appointments_empty)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(EMPTY_CLASS))
            .with_css_props(part(&[], &look.empty))]
    } else {
        appointments
            .as_ref()
            .iter()
            .enumerate()
            .map(|(index, text)| {
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(APPOINTMENT_CLASS))
                    .with_css_props(part(TODO_BAR_ROW_BASE, &look.appointment))
                    .with_child(link(
                        text.clone(),
                        RefAny::new(AppointmentData {
                            index,
                            text: text.clone(),
                            shared: shared.clone(),
                        }),
                        on_appointment_open,
                    ))
            })
            .collect()
    };
    let appointments_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(APPOINTMENTS_CLASS))
        .with_css_props(part(TODO_BAR_COLUMN_BASE, &look.appointments))
        .with_children(DomVec::from_vec(appointment_rows));

    // The task line.
    let mut input = TextInput::create()
        .with_text(task_text)
        .with_placeholder(task_placeholder)
        .with_on_virtual_key_down(
            shared.clone(),
            on_task_key as TextInputOnVirtualKeyDownCallbackType,
        );
    if let Some(theme) = theme {
        input = input.with_theme(theme);
    }
    let task_input = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TASK_INPUT_CLASS))
        .with_css_props(part(TODO_BAR_COLUMN_BASE, &look.task_input))
        .with_child(input.dom());

    // The tasks: a box, the title as a link, the due date.
    let task_rows: Vec<Dom> = tasks
        .into_library_owned_vec()
        .into_iter()
        .enumerate()
        .map(|(index, task)| {
            let ToDoTask {
                id,
                title,
                due,
                done,
            } = task;
            let data = RefAny::new(TaskData {
                index,
                id,
                shared: shared.clone(),
            });
            let mut check = CheckBox::create(done)
                .with_accessibility_name(title.clone())
                .with_on_toggle(data.clone(), on_task_toggle as CheckBoxOnToggleCallbackType);
            if let Some(theme) = theme {
                check = check.with_theme(theme);
            }
            let mut row = alloc::vec![
                Dom::create_div()
                    .with_css_props(part(TODO_BAR_FIXED_BASE, &[]))
                    .with_child(check.dom()),
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(TASK_TITLE_CLASS))
                    .with_css_props(part(TODO_BAR_GROW_BASE, &look.task_title))
                    .with_child(link(title, data, on_task_open)),
            ];
            if !due.as_str().is_empty() {
                row.push(
                    crate::widgets::widget_p_with_text(due)
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(TASK_DUE_CLASS))
                        .with_css_props(part(TODO_BAR_FIXED_BASE, &look.task_due)),
                );
            }
            let mut skin = look.task.clone();
            let mut classes: Vec<IdOrClass> = TASK_CLASS.to_vec();
            if done {
                skin.extend(look.task_done.iter().cloned());
                classes.push(TASK_DONE_CLASS[0].clone());
            }
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                .with_css_props(part(TODO_BAR_ROW_BASE, &skin))
                .with_children(DomVec::from_vec(row))
        })
        .collect();
    let tasks_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TASKS_CLASS))
        .with_css_props(part(TODO_BAR_COLUMN_BASE, &look.tasks))
        .with_children(DomVec::from_vec(task_rows));

    let mut classes: Vec<IdOrClass> = BAR_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(TODO_BAR_COLUMN_BASE, &look.bar))
        // A GROUP named by the caller, or "To-Do bar".
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Grouping,
            accessibility_name: Some(
                accessibility_name
                    .into_option()
                    .unwrap_or_else(|| AzString::from_const_str("To-Do bar")),
            )
            .into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(alloc::vec![
            calendar_box,
            appointments_box,
            task_input,
            tasks_box,
        ]))
}

#[cfg(test)]
mod todo_bar_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<(ToDoBarEventKind, usize, u64, u32, String)>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: ToDoBarEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((
                event.kind,
                event.index,
                event.id,
                event.date.day,
                event.text.as_str().to_string(),
            ));
        }
        Update::RefreshDom
    }

    fn bar(log: &Log) -> ToDoBar {
        let data = || RefAny::new(log.clone());
        let cb = record as ToDoBarOnEventCallbackType;
        ToDoBar::create(2026, 9, 12)
            .with_today(2026, 9, 30)
            .with_tasks(ToDoTaskVec::from_vec(vec![
                ToDoTask::create(1, AzString::from("Reply to Alice")).with_due(AzString::from("Today")),
                ToDoTask::create(2, AzString::from("Book flights")).with_done(true),
            ]))
            .with_on_pick(data(), cb)
            .with_on_task(data(), cb)
            .with_on_appointment(data(), cb)
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

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The node whose direct text child reads `label`.
    fn node_labelled(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        for (i, nd) in styled.node_data.as_ref().iter().enumerate() {
            if let NodeType::Text(s) = nd.get_node_type() {
                if s.as_ref().as_str() == label {
                    return hierarchy[i].parent_id().expect("a label sits in its block");
                }
            }
        }
        panic!("no node is labelled {label:?}");
    }

    /// The weekday the bar's calendar rows start on: its first weekday name.
    fn first_weekday(dom: &Dom) -> Option<String> {
        const NAMES: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
        let mut calendar = Vec::new();
        texts(&dom.children.as_ref()[0], &mut calendar);
        calendar.into_iter().find(|t| NAMES.contains(&t.as_str()))
    }

    /// AzCalendar's navigator runs Monday to Sunday, its To-Do bar ran
    /// Sunday to Saturday beside it (PIM6): the bar passes the week start it
    /// is given to its calendar; Sunday stays the default.
    #[test]
    fn a_bar_lays_its_calendar_on_the_week_start_it_is_given() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let monday = bar(&log)
                .with_week_start(DatePickerWeekStart::Monday)
                .with_theme(theme)
                .dom();
            assert_eq!(
                first_weekday(&monday).as_deref(),
                Some("Mo"),
                "{}: Monday first",
                theme.name()
            );
            let default = bar(&log).with_theme(theme).dom();
            assert_eq!(
                first_weekday(&default).as_deref(),
                Some("Su"),
                "{}: Sunday by default",
                theme.name()
            );
        }
    }

    #[test]
    fn the_bar_is_the_calendar_the_appointments_the_task_line_and_the_tasks() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = bar(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 4, "{}", theme.name());
            assert!(
                theme_checks::find(&parts[0], "__azul-native-date-picker-inline").is_some(),
                "{}: the calendar is the inline date picker",
                theme.name()
            );
            assert!(
                theme_checks::find(&parts[0], "__azul-native-date-picker-today").is_some(),
                "{}: today is ringed",
                theme.name()
            );
            let mut appointments = Vec::new();
            texts(&parts[1], &mut appointments);
            assert_eq!(appointments, vec!["No upcoming appointments."], "{}", theme.name());
            assert_eq!(parts[2].children.as_ref().len(), 1, "{}: the task line", theme.name());
            let tasks = parts[3].children.as_ref();
            assert_eq!(tasks.len(), 2, "{}: one row per task", theme.name());
            let mut first = Vec::new();
            texts(&tasks[0], &mut first);
            assert_eq!(first, vec!["Reply to Alice", "Today"], "{}", theme.name());
            assert!(theme_checks::has_class(&tasks[1], "__azul-native-todo-bar-task-done"));
            assert!(!theme_checks::has_class(&tasks[0], "__azul-native-todo-bar-task-done"));
            assert_ne!(
                tasks[0].root.get_style(),
                tasks[1].root.get_style(),
                "{}: a done task is painted differently",
                theme.name()
            );
        }
        let busy = bar(&log)
            .with_appointments(StringVec::from_vec(vec![AzString::from("14:00 Team sync")]))
            .with_theme(UiTheme::Flat)
            .dom();
        let mut appointments = Vec::new();
        texts(&busy.children.as_ref()[1], &mut appointments);
        assert_eq!(appointments, vec!["14:00 Team sync"]);
    }

    #[test]
    fn the_bar_is_a_group_named_to_do_bar_unless_named_otherwise() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = bar(&log).with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Grouping);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("To-Do bar")
        );
        let named = bar(&log)
            .with_accessibility_name(AzString::from("Aufgabenleiste"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            named
                .root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string())),
            Some("Aufgabenleiste".to_string())
        );
    }

    #[test]
    fn a_pick_in_the_calendar_reports_the_day() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let seven = node_labelled(&styled, "7");
        let (update, _) = rv::fire(&styled, id(seven), EventFilter::Hover(HoverEventFilter::Click))
            .expect("a day takes the click");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].0, events[0].3), (ToDoBarEventKind::DatePicked, 7));
    }

    #[test]
    fn enter_adds_a_task_a_box_checks_one_off_and_a_title_opens_it() {
        assert!(task_key_adds(Some(VirtualKeyCode::Return)));
        assert!(task_key_adds(Some(VirtualKeyCode::NumpadEnter)));
        assert!(!task_key_adds(Some(VirtualKeyCode::A)));
        assert!(!task_key_adds(None));

        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        // The box: the first keyboard stop of the second task's row.
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let title = node_labelled(&styled, "Book flights");
        let row = (0..title.index())
            .rev()
            .map(NodeId::new)
            .find(|n| {
                nodes[n.index()]
                    .get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == "__azul-native-todo-bar-task"))
            })
            .expect("the task row");
        let box_slot = hierarchy[row.index()].first_child_id(row).expect("the box slot");
        let check = (box_slot.index() + 1..title.index())
            .map(NodeId::new)
            .find(|n| nodes[n.index()].get_tab_index().is_some())
            .expect("the check box");
        rv::fire(&styled, id(check), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the box takes the click");
        // The title is a link Button: its label sits inside the button, and
        // the click reaches the button by bubbling.
        let takes_click = |n: NodeId| {
            nodes[n.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
        };
        let mut link = title;
        while !takes_click(link) {
            link = hierarchy[link.index()].parent_id().expect("the title's link");
        }
        rv::fire(&styled, id(link), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the title takes the click");
        let events = log.lock().expect("log").clone();
        assert_eq!(
            events.iter().map(|e| (e.0, e.1, e.2)).collect::<Vec<_>>(),
            vec![
                (ToDoBarEventKind::TaskToggled, 1, 2),
                (ToDoBarEventKind::TaskOpened, 1, 2)
            ]
        );
    }

    /// The node whose direct text child reads `text`, and its parent: a
    /// button's label and the button.
    fn label_and_button<'a>(dom: &'a Dom, text: &str) -> Option<(&'a Dom, &'a Dom)> {
        for child in dom.children.as_ref() {
            let holds = child.children.as_ref().iter().any(|t| {
                matches!(t.root.get_node_type(), NodeType::Text(s) if s.as_ref().as_str() == text)
            });
            if holds {
                return Some((child, dom));
            }
            if let Some(found) = label_and_button(child, text) {
                return Some(found);
            }
        }
        None
    }

    /// Under flora a task's title is DATA, set as flora's text link
    /// (flora.css `a`: brass ink, underlined, in the running hand) - not as
    /// its quiet command (`.btn-quiet`: a boxed note in capitals), which
    /// turned "Reply to Alice" into "REPLY TO ALICE" (AzMail's and
    /// AzCalendar's To-Do bars).
    #[test]
    fn under_flora_a_task_title_is_a_text_link_not_a_command() {
        use azul_css::props::{
            property::{CssProperty, CssPropertyType},
            style::text::StyleTextTransform,
        };

        use crate::widgets::themes::flora;
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = bar(&log).with_theme(UiTheme::Flora).dom();
        let (label, button) = label_and_button(&dom, "Reply to Alice").expect("the task's title");
        let transform = theme_checks::resolve(label, CssPropertyType::TextTransform, false, None);
        assert!(
            !matches!(
                &transform,
                Some(CssProperty::TextTransform(v))
                    if v.get_property() == Some(&StyleTextTransform::Uppercase)
            ),
            "a title keeps its case: {transform:?}"
        );
        assert_eq!(theme_checks::text_color(label, false), Some(flora::LIGHT_QT), "brass ink");
        assert_eq!(theme_checks::text_color(label, true), Some(flora::DARK_QT));
        let width = theme_checks::resolve(button, CssPropertyType::BorderTopWidth, false, None);
        assert!(
            !matches!(
                &width,
                Some(CssProperty::BorderTopWidth(w))
                    if w.get_property().is_some_and(|w| w.inner.number.get() > 0.0)
            ),
            "no box around a link: {width:?}"
        );
    }

    #[test]
    fn a_bar_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "todo_bar",
            || bar(&log).dom(),
            |t: UiTheme| bar(&log).with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || bar(&log).dom());
            // Its task rows' check boxes: flora's mark is laid over the box.
            theme_checks::assert_structure_is_shared(
                &format!("todo_bar built for {}", theme.name()),
                &dom,
                &crate::widgets::check_box::app_theme_tests::FLORA_MARK_STRUCTURE,
            );
        }
    }
}
