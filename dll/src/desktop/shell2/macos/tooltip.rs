//! macOS tooltip: a borderless panel with a wrapping text field.
//!
//! AppKit's own tooltips (`NSView.toolTip`, `addToolTipRect:owner:userData:`) are
//! shown only by AppKit's hover tracking, after its own delay, for rects registered
//! in advance; there is no "show this tooltip now, for this box". Azul's tooltips
//! are shown by the engine - the title hover delay, `show_tooltip_at`, the
//! accessibility action - and its nodes are not views, so the tooltip is this panel.
//! AppKit still does the measuring: the text field wraps the text at the tooltip
//! width and reports the size it needs. The panel is then placed by the rule every
//! floating surface follows (`azul_layout::placement`): below its anchor, above it
//! where there is no room, inside the work area of the screen the anchor is on.
//!
//! Used by `macos/mod.rs`, created on the first tooltip.

use azul_core::geom::{LogicalRect, LogicalSize};
use azul_layout::placement::{
    floating_frame, screen_for_anchor, tooltip_text_width, FloatingPlacement,
};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBorderlessWindowMask, NSColor, NSFont, NSPanel, NSTextField, NSUtilityWindowMask,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

/// Space between the panel's left and right edges and the text, in points.
const TEXT_INSET_X: f64 = 5.0;
/// Space between the panel's top and bottom edges and the text, in points.
const TEXT_INSET_Y: f64 = 3.0;
/// How tall the text may grow when no screen is known, in points.
const FALLBACK_MAX_TEXT_HEIGHT: f64 = 600.0;

/// Wrapper for a macOS tooltip panel
pub(super) struct TooltipWindow {
    /// Tooltip panel (borderless utility window)
    panel: Retained<NSPanel>,
    /// The wrapping label showing the text; it also measures it
    text_field: Retained<NSTextField>,
    /// Is tooltip currently visible
    is_visible: bool,
}

impl TooltipWindow {
    /// Create the tooltip panel, hidden until [`Self::show`].
    pub(super) fn new(mtm: MainThreadMarker) -> Result<Self, String> {
        unsafe {
            // Create panel with utility window style and no title bar
            let style_mask = NSBorderlessWindowMask.0 | NSUtilityWindowMask.0;

            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 30.0)),
                NSWindowStyleMask(style_mask),
                objc2_app_kit::NSBackingStoreType::Buffered,
                false,
            );

            // Configure panel appearance
            panel.setFloatingPanel(true);
            panel.setHidesOnDeactivate(false);
            panel.setOpaque(false);
            panel.setHasShadow(true);
            panel.setLevel(objc2_app_kit::NSPopUpMenuWindowLevel);
            // A tooltip never takes the pointer: under it, the window below
            // would hear the pointer leave the control the tooltip explains,
            // hide it, and show it again as the pointer comes back - a flicker.
            panel.setIgnoresMouseEvents(true);

            // Set background color to tooltip yellow
            let bg_color = NSColor::colorWithRed_green_blue_alpha(
                1.0,  // Red
                1.0,  // Green
                0.85, // Blue (slightly yellow)
                0.95, // Alpha
            );
            panel.setBackgroundColor(Some(&bg_color));

            // A label that wraps its text at its width, in the system's
            // tooltip font.
            let text_field = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
            text_field.setDrawsBackground(false);
            text_field.setSelectable(false);
            text_field.setTextColor(Some(&NSColor::blackColor()));
            text_field.setFont(Some(&NSFont::toolTipsFontOfSize(0.0)));

            if let Some(content_view) = panel.contentView() {
                content_view.addSubview(&text_field);
            }

            Ok(Self {
                panel,
                text_field,
                is_visible: false,
            })
        }
    }

    /// Show the tooltip with `text` for `anchor`, a box in GLOBAL top-down
    /// points (y = 0 at the top of the primary screen) - the caller
    /// (`MacOSWindow::show_tooltip`) converts from window coordinates. If the
    /// tooltip is already visible, its text and place are updated.
    pub(super) fn show(&mut self, text: &str, anchor: LogicalRect) -> Result<(), String> {
        let screen = screen_for_anchor(anchor, &crate::desktop::display::screen_rects());
        let size = self.measure(text, screen);
        let placement = FloatingPlacement::tooltip(anchor);
        let frame = match screen {
            Some(work_area) => floating_frame(&placement, size, work_area),
            None => LogicalRect::new(placement.preferred_origin(size), size),
        };
        let (width, height) = (f64::from(frame.size.width), f64::from(frame.size.height));

        // The text fills the panel less its insets; the panel's content view
        // is bottom-up, so the bottom inset is the y origin.
        unsafe {
            self.text_field.setFrame(NSRect::new(
                NSPoint::new(TEXT_INSET_X, TEXT_INSET_Y),
                NSSize::new(
                    (width - 2.0 * TEXT_INSET_X).max(1.0),
                    (height - 2.0 * TEXT_INSET_Y).max(1.0),
                ),
            ));
        }

        // AppKit takes the frame's bottom-left in bottom-up coordinates,
        // anchored at the bottom of the PRIMARY screen (NSScreen.screens[0]).
        let primary_height = super::primary_screen_height().unwrap_or(0.0);
        let bottom_left = NSPoint::new(
            f64::from(frame.origin.x),
            primary_height - f64::from(frame.origin.y) - height,
        );
        unsafe {
            self.panel
                .setFrame_display(NSRect::new(bottom_left, NSSize::new(width, height)), true);
            self.panel.orderFront(None);
        }
        self.is_visible = true;

        Ok(())
    }

    /// The panel size `text` needs, measured by the text field: as wide as its
    /// longest line up to the tooltip width (less on a narrow screen), as tall
    /// as its lines wrapped at that width (at most the screen's height).
    #[allow(clippy::cast_possible_truncation)] // a tooltip's size in points
    fn measure(&self, text: &str, screen: Option<LogicalRect>) -> LogicalSize {
        unsafe { self.text_field.setStringValue(&NSString::from_str(text)) };
        let screen_width = screen.map_or(f32::INFINITY, |s| s.size.width);
        let max_width = f64::from(tooltip_text_width(screen_width, TEXT_INSET_X as f32));
        let max_height = screen.map_or(FALLBACK_MAX_TEXT_HEIGHT, |s| {
            (f64::from(s.size.height) - 2.0 * TEXT_INSET_Y).max(1.0)
        });
        let (text_width, text_height) = match unsafe { self.text_field.cell() } {
            Some(cell) => {
                // The text on one line, then wrapped at the width it may take.
                let natural = unsafe { cell.cellSize() };
                let width = natural.width.min(max_width).ceil();
                let wrapped = unsafe {
                    cell.cellSizeForBounds(NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(width, max_height),
                    ))
                };
                (width, wrapped.height.min(max_height).ceil())
            }
            None => (max_width, 16.0),
        };
        LogicalSize::new(
            (text_width + 2.0 * TEXT_INSET_X) as f32,
            (text_height + 2.0 * TEXT_INSET_Y) as f32,
        )
    }

    /// Hide the tooltip
    ///
    /// Removes the tooltip panel from screen without destroying it.
    /// Can be shown again with `show()`.
    pub(super) fn hide(&mut self) -> Result<(), String> {
        if !self.is_visible {
            return Ok(());
        }

        unsafe {
            self.panel.orderOut(None);
            self.is_visible = false;
        }

        Ok(())
    }

    /// Check if tooltip is currently visible
    pub(super) fn is_visible(&self) -> bool {
        self.is_visible
    }
}

impl Drop for TooltipWindow {
    fn drop(&mut self) {
        unsafe {
            self.panel.close();
        }
    }
}
