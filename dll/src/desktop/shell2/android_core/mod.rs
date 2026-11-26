//! The parts of the Android shell that need no Android: what to present and what to tell
//! `ANativeWindow_lock`, what the event loop does in a given lifecycle state, the arithmetic between
//! the display's density and the layout's pixels, the queues between the Java UI thread and the
//! loop thread.
//!
//! `shell2/android` itself is compiled for `target_os = "android"` only, so nothing in it can be
//! tested where the tests run. Everything here uses `std` and nothing else and is compiled on every
//! host, so a Linux or macOS runner (or `rustc --test` on one file) runs the tests; `shell2/android`
//! calls into it.

pub mod bars;
pub mod ime;
pub mod keys;
pub mod lifecycle;
pub mod metrics;
pub mod present;
pub mod touch;
pub mod ui_bridge;
