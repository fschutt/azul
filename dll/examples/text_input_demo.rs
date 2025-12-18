//! Text Input Demo - Demonstrates the new text editing features
//!
//! This example shows:
//! 1. Click-to-place-cursor using hit_test_to_cursor()
//! 2. Text input with automatic cursor scrolling
//! 3. IME support for Japanese/Chinese/Korean input
//! 4. Accessibility support with screen readers
//!
//! Usage:
//! - Click in the text area to place cursor
//! - Type to insert text
//! - Use arrow keys to move cursor
//! - Long text automatically scrolls into view
//! - Try Japanese IME (if available)

use azul_dll::*;

struct TextInputDemo {
    text: String,
}

impl Default for TextInputDemo {
    fn default() -> Self {
        Self {
            text: "Click here and start typing! This is a contenteditable text area with automatic cursor scrolling.".to_string(),
        }
    }
}

impl azul_dll::Layout for TextInputDemo {
    fn layout(&self, _info: LayoutInfo) -> Dom {
        Dom::new_div()
            .with_class("container")
            .with_child(
                Dom::new_div()
                    .with_class("text-input")
                    .with_attribute("contenteditable", "true")
                    .with_attribute("tabindex", "0")
                    // overflow: auto enables automatic scrollbars when text overflows
                    .with_inline_style("overflow: auto")
                    .with_inline_style("max-height: 200px")
                    .with_inline_style("max-width: 400px")
                    .with_inline_style("padding: 10px")
                    .with_inline_style("border: 2px solid #4a90e2")
                    .with_inline_style("border-radius: 4px")
                    .with_inline_style("background: white")
                    .with_child(Dom::text(&self.text))
            )
            .with_child(
                Dom::new_div()
                    .with_class("instructions")
                    .with_inline_style("margin-top: 20px")
                    .with_inline_style("color: #666")
                    .with_child(Dom::text("Instructions:"))
                    .with_child(Dom::text("• Click to place cursor"))
                    .with_child(Dom::text("• Type to insert text"))
                    .with_child(Dom::text("• Arrow keys to move cursor"))
                    .with_child(Dom::text("• Cursor auto-scrolls into view"))
                    .with_child(Dom::text("• Try Japanese/Chinese IME"))
            )
    }
}

fn main() {
    let app = App::new(TextInputDemo::default(), AppConfig::default());
    
    let window_options = WindowCreateOptions {
        state: WindowState {
            title: "Text Input Demo - Azul".into(),
            size: WindowSize {
                dimensions: PhysicalSize::new(600, 400),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };

    app.run(window_options).unwrap();
}
