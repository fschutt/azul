//! How a tab strip shows the content of its tabs ([`TabClickBehaviour`]).
//!
//! The content can be closed until a tab is hovered and then follow the
//! pointer, locked open by a double click (the default here); open on a
//! click and closed by the next one; or always open (Office's ribbon).
//!
//! [`RevealMachine`] is the behaviour as a pure state machine: the widgets
//! feed it the pointer, the clicks, the keys and the clock (milliseconds
//! from any fixed point), and read from it which tab is drawn selected and
//! whether the content shows. [`TabReveal`] is the handle an app keeps
//! between its rebuilds (as it keeps the active tab): the ribbon or tab bar
//! built with it reads the state when it is built, feeds it from its
//! callbacks and schedules the hover and leave timers on it, so a new DOM
//! shows the content exactly as the old one did.

use azul_core::{
    callbacks::{TimerCallbackReturn, Update},
    refany::RefAny,
    task::{Duration, Instant, TerminateTimer, TimerId},
};
use azul_css::corety::OptionUsize;

use crate::{
    callbacks::CallbackInfo,
    timer::{Timer, TimerCallback, TimerCallbackInfo},
};

/// How a tab strip opens and closes the content of its tabs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TabClickBehaviour {
    /// Closed until a tab is hovered for [`HOVER_OPEN_MS`]; then the content
    /// follows the pointer from tab to tab and closes [`LEAVE_CLOSE_MS`]
    /// after the pointer left the tabs and the content (not while a menu
    /// opened from it is up). A click only selects a tab; a double click
    /// locks the content open, and a double click on the locked tab (or the
    /// chevron) closes it again.
    #[default]
    HoverRevealDoubleClickLock,
    /// Closed until a click, which opens the content and locks it; a click
    /// on the open tab closes it.
    ClickToggle,
    /// Always open: a click switches the tab.
    AlwaysOpen,
}

/// How long the pointer rests on a tab before its content opens, in ms.
pub const HOVER_OPEN_MS: u64 = 120;
/// How long the pointer is away from the tabs and the content before the
/// content closes, in ms. Coming back within it keeps it open.
pub const LEAVE_CLOSE_MS: u64 = 320;
/// How long the content takes to open or close (its height), in ms.
pub const CONTENT_MOTION_MS: u32 = 260;
/// Two presses of Enter on one tab within this many ms are a double press,
/// the keyboard's double click.
pub const DOUBLE_PRESS_MS: u64 = 500;

/// What happened to a tab strip, as its callbacks see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealEvent {
    /// The pointer came onto a tab.
    EnterTab(usize),
    /// The pointer left a tab.
    LeaveTab(usize),
    /// The pointer came into the tab area (the tabs and the content).
    EnterArea,
    /// The pointer left the tab area.
    LeaveArea,
    /// A click on a tab.
    Click(usize),
    /// A double click on a tab.
    DoubleClick(usize),
    /// Enter or Space on a focused tab; `shift` with Shift held (the lock).
    Key {
        /// The focused tab.
        tab: usize,
        /// Shift was held: the press locks, as a double click does.
        shift: bool,
    },
    /// The chevron at the strip's end: it locks the content open, or closes
    /// locked content.
    Chevron,
    /// A menu or popover opened from the content came up (`true`) or went.
    MenuUp(bool),
    /// The clock moved on: a hover or a leave may have lasted long enough.
    Tick,
}

/// What an event changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealStep {
    /// Nothing anyone sees.
    Unchanged,
    /// The content opened, on this tab.
    Opened(usize),
    /// The open content switched from one tab to another.
    Switched {
        /// The tab shown before.
        from: usize,
        /// The tab shown now.
        to: usize,
    },
    /// The content closed.
    Closed,
    /// Still open on the same tab, but locked or unlocked now.
    Relocked,
}

/// The behaviour of one tab strip as a state machine over [`RevealEvent`]s.
// Its flags are independent facts about the pointer and the content, not
// states of one machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevealMachine {
    /// Which behaviour this strip follows.
    behaviour: TabClickBehaviour,
    /// Whether the content shows.
    open: bool,
    /// Whether the content is locked open: hovering another tab does not
    /// switch it and leaving does not close it.
    locked: bool,
    /// The tab the content shows (or showed last, while closed): the app's
    /// active tab once the app followed the switch.
    shown: usize,
    /// Closed by a click: hovering does not open the content again until the
    /// pointer has left the tab area and come back.
    rearm: bool,
    /// The tab the pointer rests on while the content is closed, and since
    /// when.
    hover: Option<(usize, u64)>,
    /// Since when the pointer is away from the tab area, while the content
    /// is open and not locked.
    leave_since: Option<u64>,
    /// Whether the pointer is in the tab area.
    inside: bool,
    /// Whether a menu opened from the content is up: the content stays.
    menu_up: bool,
    /// The first click (or Enter) of a possible double one: its tab, when it
    /// came, and whether that tab was the locked one before it - a double
    /// click on the locked tab closes, on any other it locks.
    first_press: Option<(usize, u64, bool)>,
}

impl RevealMachine {
    /// A strip following `behaviour`, its content on `tab`: open only when
    /// it is always open.
    #[must_use]
    pub fn new(behaviour: TabClickBehaviour, tab: usize) -> Self {
        Self {
            behaviour,
            open: behaviour == TabClickBehaviour::AlwaysOpen,
            locked: false,
            shown: tab,
            rearm: false,
            hover: None,
            leave_since: None,
            inside: false,
            menu_up: false,
            first_press: None,
        }
    }

    /// The behaviour this strip follows.
    #[must_use]
    pub const fn behaviour(&self) -> TabClickBehaviour {
        self.behaviour
    }

    /// Whether the content shows.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the content is locked open.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.open && self.locked
    }

    /// The tab the content shows, or showed last while it is closed.
    #[must_use]
    pub const fn shown(&self) -> usize {
        self.shown
    }

    /// The tab drawn selected: the shown one while the content is open, none
    /// while it is closed.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        if self.open {
            Some(self.shown)
        } else {
            None
        }
    }

    /// When the next [`RevealEvent::Tick`] can change something (a hover or
    /// a leave lasting long enough), in the event clock's ms.
    #[must_use]
    pub fn deadline(&self) -> Option<u64> {
        let hover = self.hover.map(|(_, since)| since.saturating_add(HOVER_OPEN_MS));
        let leave = self.leave_since.map(|since| since.saturating_add(LEAVE_CLOSE_MS));
        match (hover, leave) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The app made `tab` its active tab (a click it handled, a tab it
    /// selected itself): the content shows that tab from now on.
    pub const fn follow(&mut self, tab: usize) {
        self.shown = tab;
    }

    /// Feeds `event`, which happened at `now` ms, and says what it changed.
    pub fn apply(&mut self, event: RevealEvent, now: u64) -> RevealStep {
        let before = (self.open, self.shown, self.locked);
        let hovers = self.behaviour == TabClickBehaviour::HoverRevealDoubleClickLock;
        match event {
            RevealEvent::EnterTab(tab) => {
                self.inside = true;
                self.leave_since = None;
                if hovers && !self.rearm {
                    if self.open {
                        if !self.locked {
                            self.shown = tab;
                        }
                    } else {
                        self.hover = Some((tab, now));
                    }
                }
            }
            RevealEvent::LeaveTab(tab) => {
                if self.hover.is_some_and(|(hovered, _)| hovered == tab) {
                    self.hover = None;
                }
            }
            RevealEvent::EnterArea => {
                self.inside = true;
                self.leave_since = None;
            }
            RevealEvent::LeaveArea => {
                self.inside = false;
                self.hover = None;
                self.rearm = false;
                if hovers && self.open && !self.locked && !self.menu_up {
                    self.leave_since = Some(now);
                }
            }
            RevealEvent::Click(tab) => self.press(tab, now, false),
            RevealEvent::DoubleClick(tab) => self.double_press(tab),
            RevealEvent::Key { tab, shift } => {
                // The keyboard has no double click: a second press on the
                // same tab soon after the first is one.
                let double = self.first_press.is_some_and(|(t, at, _)| {
                    t == tab && now.saturating_sub(at) <= DOUBLE_PRESS_MS
                });
                self.press(tab, now, shift || double);
            }
            RevealEvent::Chevron => {
                if self.behaviour != TabClickBehaviour::AlwaysOpen {
                    if self.open && self.locked {
                        self.close(hovers);
                    } else {
                        self.open = true;
                        self.locked = true;
                        self.hover = None;
                        self.leave_since = None;
                    }
                }
            }
            RevealEvent::MenuUp(up) => {
                self.menu_up = up;
                if up {
                    self.leave_since = None;
                } else if hovers && !self.inside && self.open && !self.locked {
                    self.leave_since = Some(now);
                }
            }
            RevealEvent::Tick => {
                if let Some((tab, since)) = self.hover {
                    if now >= since.saturating_add(HOVER_OPEN_MS) {
                        self.hover = None;
                        if !self.open {
                            self.open = true;
                            self.shown = tab;
                        }
                    }
                }
                if let Some(since) = self.leave_since {
                    if now >= since.saturating_add(LEAVE_CLOSE_MS) {
                        self.leave_since = None;
                        self.open = false;
                    }
                }
            }
        }
        match (before.0, self.open) {
            (false, true) => RevealStep::Opened(self.shown),
            (true, false) => RevealStep::Closed,
            (true, true) if before.1 != self.shown => RevealStep::Switched {
                from: before.1,
                to: self.shown,
            },
            (true, true) if before.2 != self.locked => RevealStep::Relocked,
            _ => RevealStep::Unchanged,
        }
    }

    /// The content closes and unlocks; closed by a click, hovering does not
    /// open it again until the pointer left the tab area.
    const fn close(&mut self, by_click: bool) {
        self.open = false;
        self.locked = false;
        self.hover = None;
        self.leave_since = None;
        self.rearm = by_click && self.inside;
    }

    /// A click, or Enter / Space (`lock`: Shift+Enter) on `tab`.
    fn press(&mut self, tab: usize, now: u64, lock: bool) {
        let was_locked_here = self.open && self.locked && self.shown == tab;
        let second = self
            .first_press
            .is_some_and(|(t, at, _)| t == tab && now.saturating_sub(at) <= DOUBLE_PRESS_MS);
        if !second {
            self.first_press = Some((tab, now, was_locked_here));
        }
        match self.behaviour {
            TabClickBehaviour::HoverRevealDoubleClickLock => {
                // A click only selects the tab: it is open under the pointer
                // already, or opens now (a click before the hover delay).
                self.hover = None;
                self.rearm = false;
                self.open = true;
                self.shown = tab;
            }
            TabClickBehaviour::ClickToggle => {
                if self.open && self.shown == tab {
                    self.open = false;
                    self.locked = false;
                } else {
                    self.open = true;
                    self.locked = true;
                    self.shown = tab;
                }
            }
            TabClickBehaviour::AlwaysOpen => self.shown = tab,
        }
        if lock {
            self.double_press(tab);
        }
    }

    /// A double click on `tab` (or a double press of Enter): the locked tab
    /// closes and unlocks, any other locks open.
    fn double_press(&mut self, tab: usize) {
        if self.behaviour != TabClickBehaviour::HoverRevealDoubleClickLock {
            return;
        }
        let was_locked_here = match self.first_press.take() {
            Some((t, _, locked)) if t == tab => locked,
            _ => self.open && self.locked && self.shown == tab,
        };
        if was_locked_here {
            self.close(true);
        } else {
            self.open = true;
            self.locked = true;
            self.shown = tab;
            self.hover = None;
            self.leave_since = None;
        }
    }
}

// ---- the handle an app keeps ----

/// The state a [`RevealMachine`] keeps between events: the machine, the
/// clock's origin and the timer waiting for its next deadline.
struct RevealShared {
    machine: RevealMachine,
    /// The instant the event clock counts from (the first event's).
    epoch: Option<Instant>,
    /// The timer that ticks the machine at its next deadline.
    timer: Option<TimerId>,
    /// The tab the content was last built for: a build for another tab
    /// slides the new content in from that side.
    built: usize,
}

/// Where a tab strip's content stands, kept by the app between rebuilds (as
/// it keeps the active tab) and handed to the ribbon or the tab bar on every
/// build. Clones share one state.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabReveal {
    /// The shared state: a `RevealShared`, never read by the app.
    pub state: RefAny,
}

azul_css::impl_option!(TabReveal, OptionTabReveal, copy = false, [Debug, Clone]);

/// Who hears of a tab the content switched to: the widget's tab callback,
/// which makes the app follow (set its active tab and rebuild).
#[derive(Clone)]
pub(crate) enum RevealOwner {
    /// A ribbon's tab-click callback.
    Ribbon(crate::widgets::ribbon::OptionRibbonOnTabClick),
    /// A tab bar's click callback.
    Header(crate::widgets::tabs::OptionTabOnClick),
}

impl RevealOwner {
    /// Tells the owner the content now shows `tab`.
    fn switch_to(&mut self, info: CallbackInfo, tab: usize) -> Update {
        match self {
            Self::Ribbon(cb) => match cb.as_mut() {
                Some(c) => c.callback.invoke(c.refany.clone(), info, tab),
                None => Update::DoNothing,
            },
            Self::Header(cb) => match cb.as_mut() {
                Some(c) => c.callback.invoke(
                    c.refany.clone(),
                    info,
                    crate::widgets::tabs::TabHeaderState { active_tab: tab },
                ),
                None => Update::DoNothing,
            },
        }
    }
}

/// What a timer of a [`TabReveal`] carries.
struct RevealTimer {
    reveal: TabReveal,
    owner: RevealOwner,
}

impl TabReveal {
    /// A strip following `behaviour`, its content on the first tab: closed,
    /// unless it is always open.
    #[must_use]
    pub fn create(behaviour: TabClickBehaviour) -> Self {
        Self {
            state: RefAny::new(RevealShared {
                machine: RevealMachine::new(behaviour, 0),
                epoch: None,
                timer: None,
                built: 0,
            }),
        }
    }

    /// The behaviour the strip follows.
    #[must_use]
    pub fn get_behaviour(&self) -> TabClickBehaviour {
        self.machine()
            .map_or(TabClickBehaviour::AlwaysOpen, |m| m.behaviour())
    }

    /// Whether the content shows: build the panel open or closed by it.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.machine().is_none_or(|m| m.is_open())
    }

    /// Whether the content is locked open.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.machine().is_some_and(|m| m.is_locked())
    }

    /// The tab drawn selected: the shown one while the content is open,
    /// none while it is closed.
    #[must_use]
    pub fn get_selected(&self) -> OptionUsize {
        self.machine().and_then(|m| m.selected()).into()
    }

    /// Feeds `event` at `now_ms` (any clock; the widgets use their own)
    /// without a callback: no timer, no app to tell. For tests and for an
    /// app that drives the content itself (a shortcut that locks it open).
    #[must_use]
    pub fn apply(&self, event: RevealEvent, now_ms: u64) -> RevealStep {
        let mut state = self.state.clone();
        let Some(mut shared) = state.downcast_mut::<RevealShared>() else {
            return RevealStep::Unchanged;
        };
        shared.machine.apply(event, now_ms)
    }

    /// A copy of the machine as it stands.
    pub(crate) fn machine(&self) -> Option<RevealMachine> {
        let mut state = self.state.clone();
        let shared = state.downcast_ref::<RevealShared>()?;
        Some(shared.machine)
    }

    /// At build time: the app's active tab is `tab`, which the content
    /// follows. Returns the tab the content was built for before, so a
    /// build for another tab can slide its content in.
    pub(crate) fn begin_build(&self, tab: usize) -> usize {
        let mut state = self.state.clone();
        let Some(mut shared) = state.downcast_mut::<RevealShared>() else {
            return tab;
        };
        shared.machine.follow(tab);
        core::mem::replace(&mut shared.built, tab)
    }

    /// Feeds `event` from a callback: applies it at the callback's time,
    /// moves the timer to the machine's next deadline and tells `owner` of a
    /// tab the content switched to. Returns what the callback returns: a
    /// rebuild whenever anything shows differently.
    pub(crate) fn feed(
        &self,
        info: &mut CallbackInfo,
        owner: &RevealOwner,
        event: RevealEvent,
    ) -> Update {
        let now_instant = info.get_current_time();
        let mut state = self.state.clone();
        let (step, deadline, now, old_timer) = {
            let Some(mut shared) = state.downcast_mut::<RevealShared>() else {
                return Update::DoNothing;
            };
            let epoch = shared.epoch.get_or_insert_with(|| now_instant.clone()).clone();
            let now = now_instant.duration_since(&epoch).as_millis_u64();
            let step = shared.machine.apply(event, now);
            (step, shared.machine.deadline(), now, shared.timer.take())
        };
        if let Some(timer) = old_timer {
            info.remove_timer(timer);
        }
        if let Some(deadline) = deadline {
            let id = TimerId::unique();
            let timer = Timer::create(
                RefAny::new(RevealTimer {
                    reveal: self.clone(),
                    owner: owner.clone(),
                }),
                TimerCallback::create(on_reveal_deadline),
                info.get_system_time_fn(),
            )
            .with_delay(Duration::from_millis(deadline.saturating_sub(now).max(1)));
            info.add_timer(id, timer);
            if let Some(mut shared) = state.downcast_mut::<RevealShared>() {
                shared.timer = Some(id);
            }
        }
        let mut owner = owner.clone();
        match step {
            RevealStep::Unchanged => Update::DoNothing,
            RevealStep::Opened(tab) | RevealStep::Switched { to: tab, .. } => {
                let answer = owner.switch_to(*info, tab);
                if answer == Update::DoNothing {
                    Update::RefreshDom
                } else {
                    answer
                }
            }
            RevealStep::Closed | RevealStep::Relocked => Update::RefreshDom,
        }
    }
}

// ---- the callbacks a widget hangs on its tabs, its area and its chevron ----

/// What a reveal callback carries: the tab it sits on (the shown one for
/// the area and the chevron), the reveal and who hears of a switch.
pub(crate) struct RevealTabData {
    tab: usize,
    reveal: TabReveal,
    owner: RevealOwner,
}

/// Feeds the event `event` makes of the callback's tab.
fn feed_from(data: &mut RefAny, info: &mut CallbackInfo, event: fn(usize) -> RevealEvent) -> Update {
    let Some((tab, reveal, owner)) = data
        .downcast_ref::<RevealTabData>()
        .map(|d| (d.tab, d.reveal.clone(), d.owner.clone()))
    else {
        return Update::DoNothing;
    };
    reveal.feed(info, &owner, event(tab))
}

extern "C" fn on_reveal_tab_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, RevealEvent::Click)
}

extern "C" fn on_reveal_tab_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, RevealEvent::DoubleClick)
}

extern "C" fn on_reveal_tab_enter(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, RevealEvent::EnterTab)
}

extern "C" fn on_reveal_tab_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, RevealEvent::LeaveTab)
}

extern "C" fn on_reveal_area_enter(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, |_| RevealEvent::EnterArea)
}

extern "C" fn on_reveal_area_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, |_| RevealEvent::LeaveArea)
}

extern "C" fn on_reveal_chevron(mut data: RefAny, mut info: CallbackInfo) -> Update {
    feed_from(&mut data, &mut info, |_| RevealEvent::Chevron)
}

/// Enter or Space on a focused tab is a click; Shift+Enter, or a second
/// Enter soon after the first, the lock.
extern "C" fn on_reveal_tab_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;
    let keys = info.get_current_keyboard_state();
    if keys.ctrl_down() || keys.alt_down() || keys.super_down() {
        return Update::DoNothing;
    }
    let shift = keys.shift_down();
    match keys.current_virtual_keycode.into_option() {
        Some(K::Return) => {}
        Some(K::Space) if !shift => {}
        _ => return Update::DoNothing,
    }
    let Some((tab, reveal, owner)) = data
        .downcast_ref::<RevealTabData>()
        .map(|d| (d.tab, d.reveal.clone(), d.owner.clone()))
    else {
        return Update::DoNothing;
    };
    info.prevent_default();
    reveal.feed(&mut info, &owner, RevealEvent::Key { tab, shift })
}

/// One callback on `event` carrying `data`.
fn callback(
    event: azul_core::dom::EventFilter,
    cb: usize,
    data: &RefAny,
) -> azul_core::callbacks::CoreCallbackData {
    azul_core::callbacks::CoreCallbackData {
        event,
        callback: azul_core::callbacks::CoreCallback {
            cb,
            ctx: azul_core::refany::OptionRefAny::None,
        },
        refany: data.clone(),
    }
}

/// The callbacks a tab hangs so `reveal` hears the pointer, the clicks
/// and the keys on it (`keys`: the tab is focusable).
pub(crate) fn tab_callbacks(
    tab: usize,
    reveal: &TabReveal,
    owner: &RevealOwner,
    keys: bool,
) -> Vec<azul_core::callbacks::CoreCallbackData> {
    use azul_core::{
        dom::{EventFilter as E, HoverEventFilter as H},
        events::FocusEventFilter,
    };
    let data = RefAny::new(RevealTabData {
        tab,
        reveal: reveal.clone(),
        owner: owner.clone(),
    });
    let mut v = alloc::vec![
        callback(E::Hover(H::Click), on_reveal_tab_click as usize, &data),
        callback(E::Hover(H::DoubleClick), on_reveal_tab_double_click as usize, &data),
        callback(E::Hover(H::MouseEnter), on_reveal_tab_enter as usize, &data),
        callback(E::Hover(H::MouseLeave), on_reveal_tab_leave as usize, &data),
    ];
    if keys {
        v.push(callback(
            E::Focus(FocusEventFilter::VirtualKeyDown),
            on_reveal_tab_key as usize,
            &data,
        ));
    }
    v
}

/// The callbacks that make a node part of `reveal`'s hover area (the tab
/// row and the content are one area: leaving it starts the close, coming
/// back cancels it).
pub(crate) fn area_callbacks(
    reveal: &TabReveal,
    owner: &RevealOwner,
) -> Vec<azul_core::callbacks::CoreCallbackData> {
    use azul_core::dom::{EventFilter as E, HoverEventFilter as H};
    let data = RefAny::new(RevealTabData {
        tab: 0,
        reveal: reveal.clone(),
        owner: owner.clone(),
    });
    alloc::vec![
        callback(E::Hover(H::MouseEnter), on_reveal_area_enter as usize, &data),
        callback(E::Hover(H::MouseLeave), on_reveal_area_leave as usize, &data),
    ]
}

/// The click of the chevron that locks the content open or closes it.
pub(crate) fn chevron_callback(
    reveal: &TabReveal,
    owner: &RevealOwner,
) -> azul_core::callbacks::CoreCallbackData {
    use azul_core::dom::{EventFilter as E, HoverEventFilter as H};
    let data = RefAny::new(RevealTabData {
        tab: 0,
        reveal: reveal.clone(),
        owner: owner.clone(),
    });
    callback(E::Hover(H::Click), on_reveal_chevron as usize, &data)
}

/// What a revealing content panel declares after its own style: it opens
/// and closes by its height (over [`CONTENT_MOTION_MS`] on the house
/// curve, so whatever lies under it moves with it), and while closed it
/// has none.
pub(crate) fn content_motion(open: bool) -> Vec<azul_css::dynamic_selector::CssPropertyWithConditions> {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions as C,
        props::{
            basic::{
                animation::{AnimationIterationCount, StyleAnimation, StyleAnimationVec},
                time::CssDuration,
            },
            layout::{
                LayoutHeight, LayoutMinHeight, LayoutOverflow, LayoutPaddingBottom,
                LayoutPaddingTop,
            },
            property::{CssProperty as P, StyleAnimationVecValue},
            style::LayoutBorderBottomWidth,
        },
        AzString,
    };
    let mut v = alloc::vec![
        C::simple(P::const_overflow_y(LayoutOverflow::Hidden)),
        C::simple(P::Animation(StyleAnimationVecValue::Exact(StyleAnimationVec::from_vec(
            alloc::vec![StyleAnimation {
                name: AzString::from_const_str("height"),
                duration: CssDuration::from_millis(CONTENT_MOTION_MS),
                delay: CssDuration::from_millis(0),
                iterations: AnimationIterationCount::Count(1),
                timing: crate::widgets::themes::flora::FL_EASE,
                clip: true,
            }],
        )))),
    ];
    if !open {
        v.extend([
            C::simple(P::const_height(LayoutHeight::const_px(0))),
            C::simple(P::const_min_height(LayoutMinHeight::const_px(0))),
            C::simple(P::const_padding_top(LayoutPaddingTop::const_px(0))),
            C::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(0))),
            C::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(0))),
        ]);
    }
    v
}

/// A reveal's deadline came: the machine ticks (and the timer ends; a new
/// one is set for the next deadline, if any).
extern "C" fn on_reveal_deadline(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some((reveal, owner)) = data
        .downcast_ref::<RevealTimer>()
        .map(|t| (t.reveal.clone(), t.owner.clone()))
    else {
        return TimerCallbackReturn::create(Update::DoNothing, TerminateTimer::Terminate);
    };
    {
        let mut state = reveal.state.clone();
        if let Some(mut shared) = state.downcast_mut::<RevealShared>() {
            // This timer ends here: `feed` must not remove it again.
            shared.timer = None;
        };
    }
    let mut callback_info = *info.get_callback_info();
    let update = reveal.feed(&mut callback_info, &owner, RevealEvent::Tick);
    TimerCallbackReturn::create(update, TerminateTimer::Terminate)
}

#[cfg(test)]
mod tests {
    //! The behaviours against the design's timings: a tab opens after
    //! 120 ms of hovering, the content closes 320 ms after the pointer left.

    use super::*;

    fn hover() -> RevealMachine {
        RevealMachine::new(TabClickBehaviour::HoverRevealDoubleClickLock, 0)
    }

    #[test]
    fn at_rest_the_content_is_closed_and_no_tab_is_drawn_selected() {
        let m = hover();
        assert!(!m.is_open());
        assert_eq!(m.selected(), None);
    }

    #[test]
    fn a_tab_hovered_for_120_ms_opens_its_content_and_is_drawn_selected() {
        let mut m = hover();
        m.apply(RevealEvent::EnterArea, 0);
        assert_eq!(m.apply(RevealEvent::EnterTab(1), 0), RevealStep::Unchanged);
        assert_eq!(m.deadline(), Some(120));
        assert_eq!(m.apply(RevealEvent::Tick, 119), RevealStep::Unchanged, "not yet at 119 ms");
        assert_eq!(m.apply(RevealEvent::Tick, 120), RevealStep::Opened(1));
        assert_eq!(m.selected(), Some(1));
    }

    #[test]
    fn a_tab_left_before_120_ms_opens_nothing() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::LeaveTab(1), 80);
        assert_eq!(m.deadline(), None);
        assert_eq!(m.apply(RevealEvent::Tick, 200), RevealStep::Unchanged);
        assert!(!m.is_open());
    }

    #[test]
    fn while_open_moving_to_another_tab_switches_the_content_at_once() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(0), 0);
        m.apply(RevealEvent::Tick, 120);
        m.apply(RevealEvent::LeaveTab(0), 300);
        assert_eq!(
            m.apply(RevealEvent::EnterTab(2), 300),
            RevealStep::Switched { from: 0, to: 2 }
        );
        assert_eq!(m.selected(), Some(2));
    }

    #[test]
    fn leaving_the_tab_area_for_320_ms_closes_the_content() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::Tick, 120);
        m.apply(RevealEvent::LeaveArea, 1000);
        assert_eq!(m.deadline(), Some(1320));
        assert_eq!(m.apply(RevealEvent::Tick, 1319), RevealStep::Unchanged, "still open");
        assert_eq!(m.apply(RevealEvent::Tick, 1320), RevealStep::Closed);
        assert_eq!(m.selected(), None, "closed, no tab is drawn selected");
    }

    #[test]
    fn coming_back_within_320_ms_cancels_the_close() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::Tick, 120);
        m.apply(RevealEvent::LeaveArea, 1000);
        m.apply(RevealEvent::EnterArea, 1200);
        assert_eq!(m.deadline(), None);
        assert_eq!(m.apply(RevealEvent::Tick, 2000), RevealStep::Unchanged);
        assert!(m.is_open());
    }

    #[test]
    fn a_menu_opened_from_the_content_keeps_it_open() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::Tick, 120);
        m.apply(RevealEvent::MenuUp(true), 500);
        m.apply(RevealEvent::LeaveArea, 600);
        assert_eq!(m.apply(RevealEvent::Tick, 2000), RevealStep::Unchanged, "the menu is up");
        m.apply(RevealEvent::MenuUp(false), 2000);
        assert_eq!(m.apply(RevealEvent::Tick, 2320), RevealStep::Closed, "320 ms after it went");
    }

    #[test]
    fn a_single_click_only_selects_the_tab_and_does_not_lock_it() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::Tick, 120);
        assert_eq!(m.apply(RevealEvent::Click(1), 400), RevealStep::Unchanged);
        assert!(!m.is_locked());
        m.apply(RevealEvent::LeaveArea, 1000);
        assert_eq!(m.apply(RevealEvent::Tick, 1320), RevealStep::Closed);
    }

    #[test]
    fn a_double_click_locks_the_content_open() {
        let mut m = hover();
        m.apply(RevealEvent::EnterTab(1), 0);
        m.apply(RevealEvent::Tick, 120);
        m.apply(RevealEvent::Click(1), 400);
        m.apply(RevealEvent::Click(1), 600);
        assert_eq!(m.apply(RevealEvent::DoubleClick(1), 600), RevealStep::Relocked);
        assert!(m.is_locked());
        m.apply(RevealEvent::LeaveArea, 1000);
        assert_eq!(m.apply(RevealEvent::Tick, 5000), RevealStep::Unchanged, "locked open");
        m.apply(RevealEvent::EnterArea, 6000);
        assert_eq!(
            m.apply(RevealEvent::EnterTab(2), 6000),
            RevealStep::Unchanged,
            "locked, hovering another tab does not switch"
        );
        assert_eq!(
            m.apply(RevealEvent::Click(2), 7000),
            RevealStep::Switched { from: 1, to: 2 },
            "a click switches, staying locked"
        );
        assert!(m.is_locked());
    }

    #[test]
    fn a_double_click_on_the_locked_tab_closes_it_until_the_pointer_comes_back() {
        let mut m = hover();
        m.apply(RevealEvent::EnterArea, 0);
        m.apply(RevealEvent::Click(1), 0);
        m.apply(RevealEvent::Click(1), 200);
        m.apply(RevealEvent::DoubleClick(1), 200);
        assert!(m.is_locked(), "premise: locked");
        m.apply(RevealEvent::Click(1), 3000);
        m.apply(RevealEvent::Click(1), 3200);
        assert_eq!(m.apply(RevealEvent::DoubleClick(1), 3200), RevealStep::Closed);
        assert!(!m.is_locked());
        m.apply(RevealEvent::LeaveTab(1), 3300);
        m.apply(RevealEvent::EnterTab(2), 3300);
        assert_eq!(m.apply(RevealEvent::Tick, 4000), RevealStep::Unchanged, "no reopening yet");
        m.apply(RevealEvent::LeaveArea, 4100);
        m.apply(RevealEvent::EnterArea, 4500);
        m.apply(RevealEvent::EnterTab(2), 4500);
        assert_eq!(m.apply(RevealEvent::Tick, 4620), RevealStep::Opened(2), "back: it opens");
    }

    #[test]
    fn a_double_click_on_another_tab_while_locked_switches_and_stays_locked() {
        let mut m = hover();
        m.apply(RevealEvent::Click(0), 0);
        m.apply(RevealEvent::Click(0), 100);
        m.apply(RevealEvent::DoubleClick(0), 100);
        m.apply(RevealEvent::Click(2), 2000);
        m.apply(RevealEvent::Click(2), 2150);
        m.apply(RevealEvent::DoubleClick(2), 2150);
        assert_eq!(m.selected(), Some(2));
        assert!(m.is_locked());
    }

    #[test]
    fn the_chevron_locks_the_closed_content_open_and_closes_locked_content() {
        let mut m = hover();
        m.follow(1);
        assert_eq!(m.apply(RevealEvent::Chevron, 0), RevealStep::Opened(1));
        assert!(m.is_locked());
        assert_eq!(m.apply(RevealEvent::Chevron, 500), RevealStep::Closed);
        assert!(!m.is_open());
    }

    #[test]
    fn enter_on_a_tab_selects_it_and_shift_enter_or_a_double_enter_locks_it() {
        let mut m = hover();
        assert_eq!(m.apply(RevealEvent::Key { tab: 1, shift: false }, 0), RevealStep::Opened(1));
        assert!(!m.is_locked());
        m.apply(RevealEvent::Key { tab: 1, shift: false }, 300);
        assert!(m.is_locked(), "a double Enter locks");
        let mut m = hover();
        m.apply(RevealEvent::Key { tab: 2, shift: true }, 0);
        assert!(m.is_locked(), "Shift+Enter locks");
        assert_eq!(m.selected(), Some(2));
    }

    #[test]
    fn click_toggle_opens_and_locks_on_a_click_and_closes_on_a_click_on_the_open_tab() {
        let mut m = RevealMachine::new(TabClickBehaviour::ClickToggle, 0);
        m.apply(RevealEvent::EnterTab(1), 0);
        assert_eq!(m.apply(RevealEvent::Tick, 500), RevealStep::Unchanged, "hover opens nothing");
        assert_eq!(m.apply(RevealEvent::Click(1), 600), RevealStep::Opened(1));
        assert!(m.is_locked());
        m.apply(RevealEvent::LeaveArea, 700);
        assert_eq!(m.apply(RevealEvent::Tick, 5000), RevealStep::Unchanged, "leaving keeps it");
        assert_eq!(m.apply(RevealEvent::Click(2), 6000), RevealStep::Switched { from: 1, to: 2 });
        assert_eq!(m.apply(RevealEvent::Click(2), 7000), RevealStep::Closed);
    }

    #[test]
    fn always_open_is_open_and_a_click_switches_the_tab() {
        let mut m = RevealMachine::new(TabClickBehaviour::AlwaysOpen, 0);
        assert_eq!(m.selected(), Some(0));
        m.apply(RevealEvent::LeaveArea, 0);
        assert_eq!(m.apply(RevealEvent::Tick, 5000), RevealStep::Unchanged);
        assert_eq!(m.apply(RevealEvent::Click(2), 6000), RevealStep::Switched { from: 0, to: 2 });
        assert_eq!(m.apply(RevealEvent::Chevron, 6100), RevealStep::Unchanged);
        assert!(m.is_open());
    }

    #[test]
    fn a_tab_reveal_handle_shares_its_state_between_clones() {
        let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
        let copy = reveal.clone();
        assert!(!reveal.is_open());
        assert_eq!(copy.begin_build(2), 0, "built for the first tab before");
        {
            let mut state = reveal.state.clone();
            let mut shared = state.downcast_mut::<RevealShared>().expect("the state");
            shared.machine.apply(RevealEvent::Chevron, 0);
        }
        assert!(copy.is_open() && copy.is_locked(), "the copy sees the change");
        assert_eq!(copy.get_selected(), OptionUsize::Some(2), "on the app's tab");
    }
}
