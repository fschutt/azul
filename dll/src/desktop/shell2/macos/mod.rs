//! macOS implementation using AppKit/Cocoa.
//!
//! This module implements the PlatformWindow trait for macOS.
//!
//! Note: macOS uses static linking for system frameworks (standard approach).
//! No dynamic loading needed - frameworks are always present and properly versioned.

// TODO: Implement in Phase 2

#[derive(Debug, Clone, Copy)]
pub struct MacOSWindow {
    // TODO: AppKit window handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        // TODO: Add tests in Phase 2
    }
}
