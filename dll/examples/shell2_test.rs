//! Simple test for shell2 window system
//!
//! Tests:
//! - Window creation with OpenGL backend
//! - Window creation with CPU backend (fallback)
//! - Event polling
//! - Basic rendering

use azul_dll::desktop::shell2::{
    common::window::{PlatformWindow, WindowCreateOptions},
    macos::MacOSWindow,
};

fn main() {
    println!("Shell2 Window System Test\n");

    // Test 1: Create OpenGL window
    println!("Test 1: Creating OpenGL window...");
    test_opengl_window();

    // Test 2: Create CPU window (force CPU renderer)
    println!("\nTest 2: Creating CPU window...");
    test_cpu_window();

    println!("\nAll tests completed!");
}

fn test_opengl_window() {
    let options = WindowCreateOptions::default();

    match MacOSWindow::new(options) {
        Ok(mut window) => {
            println!("✓ OpenGL window created successfully");
            println!("  Backend: {:?}", window.get_render_context());
            println!("  State: {:?}", window.get_state().title);

            // Poll a few events
            println!("  Polling events (will timeout after no events)...");
            for i in 0..3 {
                if let Some(event) = window.poll_event() {
                    println!("  Event {}: {:?}", i, event);
                } else {
                    println!("  No events (iteration {})", i);
                }
            }

            // Request redraw
            window.request_redraw();
            let _ = window.present();

            // Close window
            window.close();
            println!("✓ Window closed");
        }
        Err(e) => {
            println!("✗ Failed to create OpenGL window: {}", e);
        }
    }
}

fn test_cpu_window() {
    // Force CPU renderer via environment variable
    std::env::set_var("AZUL_RENDERER", "cpu");

    let options = WindowCreateOptions::default();

    match MacOSWindow::new(options) {
        Ok(mut window) => {
            println!("✓ CPU window created successfully");
            println!("  Backend: {:?}", window.get_render_context());

            // Request redraw
            window.request_redraw();
            let _ = window.present();

            // Close window
            window.close();
            println!("✓ Window closed");
        }
        Err(e) => {
            println!("✗ Failed to create CPU window: {}", e);
        }
    }

    // Reset environment variable
    std::env::remove_var("AZUL_RENDERER");
}
