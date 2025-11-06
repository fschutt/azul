use objc::runtime::{Class, Object};
use objc_foundation::{INSArray, INSObject, INSString, NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};
use std::mem::transmute;
use {
    clipboard_metadata::ClipboardContentType,
    errors::{ClipboardError, MacOsError},
    Clipboard,
};

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

pub struct MacOsClipboard {
    pasteboard: Id<Object>,
}

impl Clipboard for MacOsClipboard {
    type Output = Self;

    fn new() -> Result<Self::Output, ClipboardError> {
        let cls = Class::get("NSPasteboard").ok_or(MacOsError::PasteboardNotFound)?;
        let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
        if pasteboard.is_null() {
            return Err(MacOsError::NullPasteboard.into());
        }
        let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
        Ok(MacOsClipboard {
            pasteboard: pasteboard,
        })
    }

    /// # **WARNING**: Unimplemented, use `get_string_contents`
    fn get_contents(&self) -> Result<(Vec<u8>, ClipboardContentType), ClipboardError> {
        Err(ClipboardError::Unimplemented)
    }

    fn get_string_contents(&self) -> Result<String, ClipboardError> {
        let string_class: Id<NSObject> = {
            let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
            unsafe { transmute(cls) }
        };

        let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
        let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();

        let string_array: Id<NSArray<NSString>> = unsafe {
            let obj: *mut NSArray<NSString> =
                msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
            if obj.is_null() {
                return Err(MacOsError::ReadObjectsForClassesNull.into());
            }
            Id::from_ptr(obj)
        };

        if string_array.count() == 0 {
            Err(MacOsError::ReadObjectsForClassesEmpty.into())
        } else {
            Ok(string_array[0].as_str().to_owned())
        }
    }

    /// # **WARNING**: Unimplemented, use `get_string_contents`
    fn set_contents(
        &self,
        contents: Vec<u8>,
        _: ClipboardContentType,
    ) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unimplemented)
    }

    fn set_string_contents(&self, contents: String) -> Result<(), ClipboardError> {
        let string_array = NSArray::from_vec(vec![NSString::from_str(&contents)]);
        let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
        let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
        return if success {
            Ok(())
        } else {
            Err(MacOsError::PasteWriteObjectsError.into())
        };
    }
}

// This is a convenience function that both cocoa-rs and
// glutin define, which seems to depend on the fact that
// `Option::None` has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe { transmute(Class::get(name)) }
}
