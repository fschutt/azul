//! macOS clipboard integration
//!
//! Uses Cocoa NSPasteboard API via objc bindings

use std::mem::transmute;

use azul_layout::managers::clipboard::ClipboardManager;
use objc::runtime::{Class, Object};
use objc_foundation::{INSArray, INSObject, INSString, NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};

use crate::{log_debug, log_error, log_info, log_warn, log_trace};
use super::super::common::debug_server::LogCategory;

#[macro_use]
use objc::{msg_send, sel, sel_impl};

// Required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

/// Synchronize clipboard manager content to macOS system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// the macOS pasteboard via NSPasteboard API.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    // Check if there's pending content to copy
    if let Some(content) = clipboard_manager.get_copy_content() {
        // Write to pasteboard
        if let Err(e) = write_to_pasteboard(&content.plain_text) {
            log_error!(LogCategory::Resources, "[macOS Clipboard] Failed to write: {:?}", e);
        }
    }

    // Clear the clipboard manager after sync
    clipboard_manager.clear();
}

/// Read content from macOS system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    read_from_pasteboard().ok()
}

/// Write string to macOS pasteboard
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    write_to_pasteboard(text)
}

/// Write string to macOS pasteboard (internal implementation)
fn write_to_pasteboard(text: &str) -> Result<(), ClipboardError> {
    let pasteboard = get_general_pasteboard()?;

    let string_array = NSArray::from_vec(vec![NSString::from_str(text)]);
    let _: usize = unsafe { msg_send![pasteboard, clearContents] };
    let success: bool = unsafe { msg_send![pasteboard, writeObjects: string_array] };

    if success {
        Ok(())
    } else {
        Err(ClipboardError::WriteError)
    }
}

/// Read string from macOS pasteboard
fn read_from_pasteboard() -> Result<String, ClipboardError> {
    let pasteboard = get_general_pasteboard()?;

    let string_class: Id<NSObject> = {
        let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
        unsafe { transmute(cls) }
    };

    let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
    let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();

    let string_array: Id<NSArray<NSString>> = unsafe {
        let obj: *mut NSArray<NSString> =
            msg_send![pasteboard, readObjectsForClasses:&*classes options:&*options];
        if obj.is_null() {
            return Err(ClipboardError::ReadError);
        }
        Id::from_ptr(obj)
    };

    if string_array.count() == 0 {
        Err(ClipboardError::EmptyClipboard)
    } else {
        Ok(string_array[0].as_str().to_owned())
    }
}

/// Get the general pasteboard instance
fn get_general_pasteboard() -> Result<Id<Object>, ClipboardError> {
    let cls = Class::get("NSPasteboard").ok_or(ClipboardError::PasteboardNotFound)?;
    let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
    if pasteboard.is_null() {
        return Err(ClipboardError::NullPasteboard);
    }
    Ok(unsafe { Id::from_ptr(pasteboard) })
}

/// Get class by name
#[inline]
fn class(name: &str) -> *mut Class {
    unsafe { transmute(Class::get(name)) }
}

#[derive(Debug, Copy, Clone)]
pub enum ClipboardError {
    PasteboardNotFound,
    NullPasteboard,
    WriteError,
    ReadError,
    EmptyClipboard,
}
