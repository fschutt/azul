//! macOS tooltip implementation using NSPanel.
//!
//! This module provides a native macOS tooltip system using NSPanel
//! with NSTextField for displaying tooltip text. The tooltip follows
//! the macOS style guidelines with proper shadow and appearance.
//!
//! Architecture:
//! - TooltipWindow: Wrapper around NSPanel and NSTextField
//! - Lifecycle: Create once, show/hide as needed
//! - Positioning: Uses NSWindow setFrameTopLeftPoint for absolute positioning
//!
//! Used by `macos/mod.rs` for hover tooltip display via lazy initialization.

use azul_core::{geom::LogicalPosition, resources::DpiScaleFactor};
use objc2::{msg_send_id, rc::Retained, runtime::ProtocolObject, sel};
use objc2_app_kit::{
    NSBorderlessWindowMask, NSColor, NSPanel, NSTextField, NSUtilityWindowMask, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

/// Approximate width per character in points (rough heuristic)
const POINTS_PER_CHAR: f64 = 7.0;
/// Horizontal padding added to text width
const TEXT_PADDING_H: f64 = 10.0;
/// Minimum tooltip width in points
const TOOLTIP_MIN_WIDTH: f64 = 50.0;
/// Maximum tooltip width in points
const TOOLTIP_MAX_WIDTH: f64 = 400.0;
/// Tooltip height in points
const TOOLTIP_HEIGHT: f64 = 25.0;
/// Text field horizontal inset from panel edge
const TEXT_FIELD_INSET_X: f64 = 5.0;
/// Text field vertical inset from panel edge
const TEXT_FIELD_INSET_Y: f64 = 2.0;
/// Fallback screen height when no main screen is available
const FALLBACK_SCREEN_HEIGHT: f64 = 1080.0;

/// Wrapper for a macOS tooltip panel
pub struct TooltipWindow {
    /// Tooltip panel (borderless utility window)
    panel: Retained<NSPanel>,
    /// Text field for displaying tooltip text
    text_field: Retained<NSTextField>,
    /// Main thread marker for AppKit operations
    mtm: MainThreadMarker,
    /// Is tooltip currently visible
    is_visible: bool,
}

impl TooltipWindow {
    /// Create a new tooltip window
    ///
    /// Creates an NSPanel with NSTextField for tooltip display.
    /// The panel is initially hidden and can be shown with `show()`.
    pub fn new(mtm: MainThreadMarker) -> Result<Self, String> {
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

            // Set background color to tooltip yellow
            let bg_color = NSColor::colorWithRed_green_blue_alpha(
                1.0,  // Red
                1.0,  // Green
                0.85, // Blue (slightly yellow)
                0.95, // Alpha
            );
            panel.setBackgroundColor(Some(&bg_color));

            // Create text field for tooltip text
            let text_field = NSTextField::initWithFrame(
                mtm.alloc(),
                NSRect::new(NSPoint::new(5.0, 5.0), NSSize::new(190.0, 20.0)),
            );
            text_field.setBezeled(false);
            text_field.setDrawsBackground(false);
            text_field.setEditable(false);
            text_field.setSelectable(false);

            // Set text color to black
            let text_color = NSColor::blackColor();
            text_field.setTextColor(Some(&text_color));

            // Add text field to panel's content view
            if let Some(content_view) = panel.contentView() {
                content_view.addSubview(&text_field);
            }

            Ok(Self {
                panel,
                text_field,
                mtm,
                is_visible: false,
            })
        }
    }

    /// Show tooltip with text at the given position
    ///
    /// If tooltip is already visible, updates text and position.
    /// Position is in logical coordinates (macOS AppKit uses points, not pixels).
    pub fn show(
        &mut self,
        text: &str,
        position: LogicalPosition,
        _dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
        // Convert text to NSString
        let ns_text = NSString::from_str(text);
        unsafe { self.text_field.setStringValue(&ns_text) };

        // Calculate text size for proper panel sizing
        // Rough heuristic: POINTS_PER_CHAR points per character
        let text_width = text.chars().count() as f64 * POINTS_PER_CHAR + TEXT_PADDING_H;
        let text_width = text_width.min(TOOLTIP_MAX_WIDTH).max(TOOLTIP_MIN_WIDTH);

        // Update panel and text field sizes
        let panel_frame =
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(text_width, TOOLTIP_HEIGHT));
        unsafe { self.panel.setFrame_display(panel_frame, false) };

        let text_field_frame = NSRect::new(
            NSPoint::new(TEXT_FIELD_INSET_X, TEXT_FIELD_INSET_Y),
            NSSize::new(
                text_width - TEXT_FIELD_INSET_X * 2.0,
                TOOLTIP_HEIGHT - TEXT_FIELD_INSET_Y * 2.0,
            ),
        );
        unsafe { self.text_field.setFrame(text_field_frame) };

        // Get screen height for Y-axis flipping
        // macOS uses bottom-left origin, so we need to flip Y
        let screen_height =
            if let Some(main_screen) = unsafe { objc2_app_kit::NSScreen::mainScreen(self.mtm) } {
                unsafe { main_screen.frame() }.size.height
            } else {
                FALLBACK_SCREEN_HEIGHT
            };

        // Use logical coordinates directly — macOS AppKit works in points, not pixels
        let screen_x = position.x as f64;
        let screen_y = screen_height - position.y as f64 - TOOLTIP_HEIGHT;

        // Set panel position (top-left point)
        unsafe {
            self.panel
                .setFrameTopLeftPoint(NSPoint::new(screen_x, screen_y + TOOLTIP_HEIGHT));

            // Show panel
            self.panel.orderFront(None);
        }
        self.is_visible = true;

        Ok(())
    }

    /// Hide the tooltip
    ///
    /// Removes the tooltip panel from screen without destroying it.
    /// Can be shown again with `show()`.
    pub fn hide(&mut self) -> Result<(), String> {
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
    pub fn is_visible(&self) -> bool {
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
