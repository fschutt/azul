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

use azul_core::{geom::LogicalPosition, resources::DpiScaleFactor};
use objc2::{msg_send_id, rc::Retained, runtime::ProtocolObject, sel};
use objc2_app_kit::{
    NSBorderlessWindowMask, NSColor, NSPanel, NSTextField, NSUtilityWindowMask, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

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
    /// Position is in logical coordinates (will be converted to screen coordinates).
    pub fn show(
        &mut self,
        text: &str,
        position: LogicalPosition,
        dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
        unsafe {
            // Convert text to NSString
            let ns_text = NSString::from_str(text);
            self.text_field.setStringValue(&ns_text);

            // Calculate text size for proper panel sizing
            let text_width = text.len() as f64 * 7.0 + 10.0; // Rough estimate: 7px per char
            let text_width = text_width.min(400.0).max(50.0); // Clamp between 50-400px
            let text_height = 25.0;

            // Update panel and text field sizes
            let panel_frame =
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(text_width, text_height));
            self.panel.setFrame_display(panel_frame, false);

            let text_field_frame = NSRect::new(
                NSPoint::new(5.0, 2.0),
                NSSize::new(text_width - 10.0, text_height - 4.0),
            );
            self.text_field.setFrame(text_field_frame);

            // Convert position to screen coordinates
            // macOS uses bottom-left origin, so we need to flip Y
            let screen_height =
                if let Some(main_screen) = objc2_app_kit::NSScreen::mainScreen(self.mtm) {
                    main_screen.frame().size.height
                } else {
                    1080.0 // Fallback
                };

            let physical_pos = position.to_physical(dpi_factor.inner.get());
            let screen_x = physical_pos.x as f64;
            let screen_y = screen_height - physical_pos.y as f64 - text_height;

            // Set panel position (top-left point)
            self.panel
                .setFrameTopLeftPoint(NSPoint::new(screen_x, screen_y + text_height));

            // Show panel
            self.panel.orderFront(None);
            self.is_visible = true;

            Ok(())
        }
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
