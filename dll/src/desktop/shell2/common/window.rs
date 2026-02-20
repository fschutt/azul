//! Platform window module.
//!
//! The old `PlatformWindow` (V1) trait has been removed. Window lifecycle
//! methods (`poll_event`, `present`, `is_open`, `close`, `request_redraw`)
//! are now inherent methods on each platform's window struct. Event processing
//! is handled by `PlatformWindow` in `event.rs`.
