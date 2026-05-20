//! Manager types responsible for stateful input and UI concerns.
//!
//! This module collects managers for accessibility, clipboard, drag-and-drop,
//! focus/cursor, gestures, hover, scroll state, selection, text editing,
//! text input, undo/redo, and virtual views. These managers are consumed
//! primarily by `layout/src/window.rs` and `layout/src/event_determination.rs`.

pub mod a11y;
pub mod biometric;
pub mod changeset;
pub mod clipboard;
pub mod drag_drop;
pub mod file_drop;
pub mod focus_cursor;
pub mod geolocation;
pub mod gesture;
pub mod gpu_state;
pub mod hover;
pub mod permission;
pub mod virtual_view;
pub mod scroll_into_view;
pub mod scroll_state;
pub mod selection;
pub mod text_edit;
pub mod text_input;
pub mod undo_redo;
