//! The window's frame (azul's `AppFrame`, the one every heavy app shares): the Sidebar at the
//! left, full height - the OS's window controls over its top, no title bar -, the tab ribbon over
//! the rest (its first tab is the location, `ui_location`), the folder view as the body with the
//! Preview docked at its right (a phone's: under it), and the command line at the foot
//! (`ui_command_bar`). A phone's quick-action sheet lies over the whole frame (`ui_sheet`).
//!
//! The windows that keep a title row are the Options and the About (the backstage): they have no
//! ribbon, and the title row is what the window is moved by there.
//!
//! The window is made for the right hand and AzDrive decides no side of its own: the sidebar's
//! edge and the preview's dock are the widget family's (`sidebar_edge`, `preview_edge`, read from
//! one `Handedness` in `sidebar_layout`), so the mirror for a left-handed user is one change there.

use azul::{prelude::*, widgets::AppFrame};
use azul_appkit::ui::TitleInsets;

use crate::{
    ui_command_bar, ui_dialogs, ui_preview,
    ui_ribbon::{self, Strip},
    ui_sheet, ui_sidebar, ui_view, DriveState,
};

/// What the window is, as far as the frame's parts care: its size, whether it is dark, how much
/// room the OS's window controls take.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Look {
    /// The window's width and height, px.
    pub window: (f32, f32),
    pub dark: bool,
    pub insets: TitleInsets,
}

/// The widest a window is that is a phone's (px): the same line the widgets draw (`Auto`'s).
pub(crate) const PHONE_MAX_WIDTH: f32 = 600.0;

/// Whether the window is built as a phone's: `Phone` always, `Desktop` never, `Auto` where the
/// platform is a touch one and the window is narrower than [`PHONE_MAX_WIDTH`] - AzDrive ships for
/// desktops, so a desktop window is a phone's only by `--form phone`.
#[must_use]
pub(crate) fn is_phone(form: azul::widgets::FormFactor, window_width: f32, touch: bool) -> bool {
    match form {
        azul::widgets::FormFactor::Phone => true,
        azul::widgets::FormFactor::Desktop => false,
        azul::widgets::FormFactor::Auto => touch && window_width > 0.0 && window_width < PHONE_MAX_WIDTH,
    }
}

/// Whether this build runs on a touch platform.
#[must_use]
pub(crate) const fn touch_platform() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// The frame of the window now.
pub(crate) fn frame(s: &DriveState, app: &RefAny, look: &Look) -> Dom {
    // The sidebar first: the ribbon starts where it leaves off, and clears what it leaves of the
    // window's controls.
    let sidebar = ui_sidebar::sidebar(s, app, look.window.0);
    let strip = Strip {
        left: sidebar.header_left_pad(),
        right: look.insets.right,
        width: (look.window.0 - s.sidebar_px()).max(1.0),
        // A phone's rail stands over the left edge under the tab row: the content clears it.
        inset: sidebar.phone_content_inset(),
    };
    let header = ui_ribbon::ribbon(s, app, &strip);
    let body = ui_view::content(s, app, s.content_size(look.window));
    let mut frame = AppFrame::create(header, body)
        .with_sidebar(sidebar)
        .with_preview(ui_preview::preview(s, app, look.dark))
        .with_footer(ui_command_bar::footer(s, app))
        .with_form(s.form);
    if let Some(sheet) = ui_sheet::sheet(s, app) {
        frame = frame.with_sheet(sheet);
    }
    frame.dom()
}

/// The Options and the About: the title row over the page (the window is moved by the row; there
/// is no ribbon to move it by).
pub(crate) fn backstage_window(s: &DriveState, app: &RefAny, page: usize) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px;")
        .with_child(azul_appkit::ui::title_row(&crate::window_title(s)))
        .with_child(
            Dom::create_div()
                .with_id("shell-backstage")
                .with_class("__azul-native-office-shell-backstage")
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                     min-width: 0px;",
                )
                .with_child(ui_dialogs::backstage(s, app, page)),
        )
}

#[cfg(test)]
#[path = "ui_frame_test.rs"]
mod ui_frame_test;
