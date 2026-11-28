//! What the windows of the heavy Azlin apps decide the same way (feature `azul`): whether the
//! window is built as a phone's or a desktop's, how the sidebar starts, what a choice does to a
//! phone's drawer, and how far the tab strip keeps from the window's edges.
//!
//! AzMail and AzDrive each wrote these once for themselves (`ui_frame.rs`); AzNotes, AzContacts and
//! AzTasks - and every app after them - call this, so the rules are one set of numbers and one set
//! of tests (`form_test.rs`):
//!
//! - [`form_of`] / [`form_from_env`]: `phone` / `desktop` (any case) in an environment variable
//!   (`AZNOTES_FORM`, ...) is what the scenarios and the pictures of a desktop window use for the
//!   phone preset; anything else is [`FormFactor::Auto`], the window decides;
//! - [`is_phone`]: `Phone` always, `Desktop` never, `Auto` where the platform is a touch one and the
//!   window is narrower than [`PHONE_MAX_WIDTH`] (a desktop window narrowed stays a desktop's, a
//!   tablet or a phone on its side at 600 dp and more is a desktop's: text headers beside the slim
//!   rail);
//! - [`initial_sidebar`] / [`after_choice`]: a phone starts with the drawer shut (the rail), a desktop
//!   as it was left; choosing a row closes a phone's drawer and nothing else;
//! - [`Strip`] / [`strip_offsets`]: the room the tab strip keeps before its first tab (macOS's
//!   traffic lights, as far as the sidebar leaves them) and after its last (Windows' and Linux's
//!   controls), and the width the ribbon is laid out in.
//!
//! Every left / right decision stays in the widget family (`sidebar_edge`, `preview_edge`,
//! `Handedness`): an app decides no side of its own.

use azul::widgets::{FormFactor, SidebarState, TabsInTitlebar};

/// The widest a window is that is a phone's, px: the line the widgets draw (`Auto`'s).
pub const PHONE_MAX_WIDTH: f32 = 600.0;

/// The form a variable's value asks for: `phone`, `desktop` (any case, blanks around), else the
/// window decides.
#[must_use]
pub fn form_of(value: Option<&str>) -> FormFactor {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("phone") => FormFactor::Phone,
        Some("desktop") => FormFactor::Desktop,
        _ => FormFactor::Auto,
    }
}

/// [`form_of`] the environment variable `var`.
#[must_use]
pub fn form_from_env(var: &str) -> FormFactor {
    form_of(std::env::var(var).ok().as_deref())
}

/// Whether this build runs on a touch platform.
#[must_use]
pub const fn touch_platform() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// Whether a window `width` wide is built as a phone's: `Phone` always, `Desktop` never, `Auto`
/// where the platform is a touch one (`touch`) and the window is narrower than
/// [`PHONE_MAX_WIDTH`]. A width that is not measured yet (0, NaN) is not narrow.
#[must_use]
pub fn is_phone(form: FormFactor, width: f32, touch: bool) -> bool {
    match form {
        FormFactor::Phone => true,
        FormFactor::Desktop => false,
        FormFactor::Auto => touch && width > 0.0 && width < PHONE_MAX_WIDTH,
    }
}

/// The sidebar's state at the start: a phone starts with the drawer shut (the rail), a desktop as it
/// was left (`collapsed`: the rail, else expanded). A touch platform whose width is not known yet
/// starts as a phone does.
#[must_use]
pub fn initial_sidebar(form: FormFactor, touch: bool, collapsed: bool) -> SidebarState {
    match form {
        FormFactor::Phone => SidebarState::Rail,
        FormFactor::Desktop => {
            if collapsed {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
        FormFactor::Auto => {
            if touch || collapsed {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
    }
}

/// The sidebar's state after a choice (a row, Help, a place): choosing closes a phone's drawer.
#[must_use]
pub const fn after_choice(state: SidebarState, phone: bool) -> SidebarState {
    match (state, phone) {
        (SidebarState::Expanded, true) => SidebarState::Rail,
        (other, _) => other,
    }
}

/// The room the tab strip keeps at the window's edges for the OS's window controls the sidebar
/// does not cover, and the width it is laid out in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strip {
    /// Room before the first tab (macOS's traffic lights, as far as the sidebar leaves them).
    pub left: f32,
    /// Room after the last tab (Linux's and Windows's controls over the strip).
    pub right: f32,
    /// The width the ribbon is laid out in, px.
    pub width: f32,
    /// Whether the window is a phone's: the tabs are icon squares.
    pub phone: bool,
    /// How far the ribbon's content band keeps from the window's left edge: the phone's rail
    /// stands beside it, below the tab row (`Sidebar::phone_content_inset`); 0 elsewhere.
    pub content_inset: f32,
}

/// The tab strip's offsets: the platform's row of tab buttons that IS the title bar, with the room
/// before the first tab the sidebar leaves of the window's controls.
#[must_use]
pub fn strip_offsets(strip: &Strip) -> TabsInTitlebar {
    let platform = crate::ui::tabs_in_titlebar();
    TabsInTitlebar::create(platform.top, strip.left.max(0.0), strip.right.max(platform.right))
}

#[cfg(test)]
#[path = "form_test.rs"]
mod form_test;
