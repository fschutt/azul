//! The editor's window: azul's `AppFrame`, the one every heavy app shares.
//!
//! ```text
//! desktop                                         phone (the rail; the drawer pushes it all aside)
//! +----+-----------------------------------+      +--------------------------+
//! | ●●●| ^ File | Home | Insert | Layout...|      | [^][F][H][I][L][R][V]    |  <- in the status bar's
//! |    +-----------------------------------+      +-+------------------------+     band, round the camera
//! |the | the text flows here, a column     |      |r| the text flows here    |
//! |side| of a comfortable measure, no      |      |a|                        |
//! |bar | Preview, no list inside           |      |i|                        |
//! |    +-----------------------------------+      +-+------------------------+
//! |    | status bar: page, words, views    |      | status bar               |
//! +----+-----------------------------------+      +--------------------------+
//! ```
//!
//! - The LEFT sidebar (`ui_sidebar`) lists the open documents (sync pie, collaboration icon), the
//!   outline, the pages and the styles; collapsed it is the 18 px rail with the formatting quick
//!   access at the bottom, where the thumb is.
//! - The tab strip is the title bar (there is no title row): File shows the location of the
//!   document (`ui_location`), then Home, Insert, Layout, Review, View (`ribbon`). On a phone the
//!   strip stands IN the status bar's band, round the camera cutout: AzWriter draws under the
//!   system bars on purpose ('I do not need the battery level when writing a document'), so it
//!   pads the frame by the insets itself.
//! - The body is the document (`pages`): the text FLOWS, the page layout is a View option. There is
//!   no Preview, no list in the main view: the text gets all the room.
//! - The footer is the status bar. A phone's quick-action sheet (`ui_sheet`) lies over it all.

use azul::{
    callbacks::{LayoutCallbackInfo, RefAny},
    css::LogicalPosition,
    dom::Dom,
    svg::LogicalRect,
    widgets::{AppFrame, FormFactor, SidebarState, StatusBarBand},
};
use azul_appkit::insets::Insets;

use crate::{
    app::AppState,
    ids,
    pages::{self, Area},
    ribbon::{self, Strip},
    ui_sheet, ui_sidebar,
};

/// The sidebar's state at the start: a phone starts with the drawer shut (the rail), a desktop
/// with the sidebar open (the open documents are what it is for).
#[must_use]
pub(crate) const fn initial_sidebar(phone: bool) -> SidebarState {
    if phone {
        SidebarState::Rail
    } else {
        SidebarState::Expanded
    }
}

/// Whether the layout being built is a phone's: the engine's one rule (a touch platform, or a
/// window simulating a phone, and a window narrower than 600 dp), asked while the DOM is built so
/// that a resize across the width builds it again.
#[must_use]
pub(crate) fn is_phone() -> bool {
    FormFactor::Auto.resolved() == FormFactor::Phone
}

/// How far the frame keeps from the window's left edge, its right edge and its bottom edge, px.
/// A window the engine insets itself (a desktop, anything that does not draw under the bars) needs
/// only the keyboard's lift; one that draws under the bars (a phone's) keeps clear of the cutout
/// at the sides, of the gesture bar or the home indicator at the bottom (nothing of it while the
/// keyboard stands on it) and rises by the keyboard.
#[must_use]
pub(crate) fn pads(insets: &Insets, draws_under_the_bars: bool) -> (f32, f32, f32) {
    if draws_under_the_bars {
        (insets.left, insets.right, insets.bottom_clear() + insets.lift())
    } else {
        (0.0, 0.0, insets.inside_root().lift())
    }
}

/// A status band as a box padded by `left` and `right` at its sides sees it. The lanes of a band
/// are in WINDOW coordinates; the strip lays itself out in the box it stands in, which the padding
/// moved. (The cutout lanes already keep clear of the side insets: the padding must not move the
/// strip twice.)
#[must_use]
pub(crate) fn band_in_box(band: StatusBarBand, left: f32, right: f32) -> StatusBarBand {
    if left <= 0.0 && right <= 0.0 {
        return band;
    }
    let lanes: Vec<LogicalRect> = band
        .lanes
        .as_slice()
        .iter()
        .map(|lane| LogicalRect {
            origin: LogicalPosition {
                x: lane.origin.x - left,
                y: lane.origin.y,
            },
            size: lane.size,
        })
        .collect();
    StatusBarBand {
        lanes: lanes.into(),
        height: band.height,
        width: (band.width - left - right).max(0.0),
    }
}

/// The frame's CSS padding box around it, or none while there is nothing to keep clear of.
fn padded(frame: Dom, (left, right, bottom): (f32, f32, f32)) -> Dom {
    if left <= 0.0 && right <= 0.0 && bottom <= 0.0 {
        return frame;
    }
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px; min-width: 0px; \
             box-sizing: border-box; padding: 0px {right}px {bottom}px {left}px;"
        ))
        .with_child(frame)
}

/// `content` (the settings page, the backstage) kept clear of the system bars on a window that draws
/// under them: a box padded by the insets on all four sides, and risen by the keyboard. A window the
/// engine insets itself gets the content as it is.
#[must_use]
pub(crate) fn clear_of_bars(content: Dom, info: &LayoutCallbackInfo) -> Dom {
    if !ribbon::draws_under_the_bars() {
        return content;
    }
    let insets = Insets::of(info);
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px; \
             box-sizing: border-box; padding: {}px {}px {}px {}px;",
            insets.top,
            insets.right,
            insets.bottom_clear() + insets.lift(),
            insets.left
        ))
        .with_child(content)
}

/// The editor's frame: the sidebar, the ribbon, the document, the status bar.
#[must_use]
pub(crate) fn editor_frame(st: &AppState, app: &RefAny, info: &LayoutCallbackInfo, phone: bool) -> Dom {
    let width = info.get_window_width();
    let title = azul_appkit::ui::title_insets(info);
    let insets = Insets::of(info);
    let pad = pads(&insets, ribbon::draws_under_the_bars());
    // The sidebar first: the tab strip starts where it leaves off and clears what it leaves of the
    // window's controls.
    let sidebar = ui_sidebar::sidebar(st, app, info);
    let band = ribbon::status_band(info).map(|band| band_in_box(band, pad.0, pad.1));
    let main_width = (width - pad.0 - pad.1 - sidebar.column().in_frame(width)).max(1.0);
    let strip = Strip {
        left: azul_appkit::frame::tab_row_left_pad(&sidebar),
        right: title.right,
        width: main_width,
        phone,
        content_inset: sidebar.phone_content_inset(),
        band: band.clone(),
    };
    let header = ribbon::ribbon(app, st, &strip);
    // no keyboard room of its own: the frame itself stands on the keyboard (`pads`)
    let body = pages::document_area(app, st, Area { width: main_width, phone, lift: 0.0 });

    let mut frame = AppFrame::create(header, body)
        .with_sidebar(sidebar)
        .with_footer(pages::status_bar(app, st, phone))
        .with_dom_id(ids::FRAME)
        .with_form(if phone { FormFactor::Phone } else { FormFactor::Desktop });
    // where the strip ends: below the band it stands in
    if let Some(band) = band.as_ref() {
        frame = frame.with_rail_top(band.height);
    }
    if let Some(sheet) = ui_sheet::sheet(st, app, phone) {
        frame = frame.with_sheet(sheet);
    }
    padded(frame.dom(), pad)
}

#[cfg(test)]
#[path = "ui_frame_test.rs"]
mod ui_frame_test;
