use super::*;
use std::ffi::{CString, c_void};
use std::path::Path;
use std::rc::Rc;
use std::sync::Once;
use std::ptr::NonNull;

use objc2::{
    class, msg_send, msg_send_id, sel, ClassType
};
use objc2_foundation::{
    NSString, NSArray, NSObject, 
    MainThreadMarker
};
use objc2_core_foundation::{
    CFString, 
    base::CFType,
    geometry::{CGRect, CGPoint, CGSize}
};
use objc2_app_kit::{
    NSApplication, NSWindow, NSAlert, NSTextField,
    NSModalResponse, NSAlertStyle
};

// Dynamic library loading for Cocoa framework
static INIT: Once = Once::new();
static mut COCOA_LIB: Option<CocoaFunctions> = None;

struct CocoaFunctions {
    // Handle to dynamically loaded library
    _lib_handle: *mut c_void,
}

impl Drop for CocoaFunctions {
    fn drop(&mut self) {
        if !self._lib_handle.is_null() {
            unsafe {
                dlclose(self._lib_handle);
            }
        }
    }
}

// Ensure we can call dlopen/dlsym/dlclose
#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const std::os::raw::c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}

fn get_cocoa_functions() -> &'static CocoaFunctions {
    unsafe {
        INIT.call_once(|| {
            // Typical flags for dlopen
            const RTLD_NOW: i32 = 2;
            const RTLD_GLOBAL: i32 = 8;

            // Load the Cocoa framework
            let framework_path = CString::new("/System/Library/Frameworks/Cocoa.framework/Cocoa").unwrap();
            let handle = dlopen(framework_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL);
            
            if handle.is_null() {
                panic!("Could not dlopen Cocoa.framework");
            }

            COCOA_LIB = Some(CocoaFunctions {
                _lib_handle: handle,
            });
        });

        COCOA_LIB.as_ref().unwrap()
    }
}

// Ensure macOS UI operations happen on the main thread
fn ensure_main_thread<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    if unsafe { msg_send![class!(NSThread), isMainThread] } {
        f()
    } else {
        let (tx, rx) = std::sync::mpsc::channel();
        
        let block = block2::RcBlock::new(move || {
            let result = f();
            tx.send(result).ok();
        });
        
        unsafe {
            let () = msg_send![class!(NSThread), 
                performFunctionInMainThread: &*block
                waitUntilDone: true];
        }
        
        rx.recv().unwrap()
    }
}

// Convert Rust string to NSString
fn to_ns_string(s: &str) -> objc2::rc::Retained<NSString> {
    unsafe { NSString::from_str(s) }
}

// NSAlert helpers
fn create_alert(title: &str, message: &str, icon: MessageBoxIcon) -> objc2::rc::Retained<NSAlert> {
    let _ = get_cocoa_functions(); // Ensure Cocoa is loaded
    
    let alert_style = match icon {
        MessageBoxIcon::Info => NSAlertStyle::Informational,
        MessageBoxIcon::Warning => NSAlertStyle::Warning,
        MessageBoxIcon::Error => NSAlertStyle::Critical,
        MessageBoxIcon::Question => NSAlertStyle::Informational,
    };

    unsafe {
        let alert: objc2::rc::Retained<NSAlert> = msg_send_id![class!(NSAlert), new];
        
        let ns_title = to_ns_string(title);
        let _: () = msg_send![&alert, setMessageText: &*ns_title];
        
        let ns_message = to_ns_string(message);
        let _: () = msg_send![&alert, setInformativeText: &*ns_message];
        
        let _: () = msg_send![&alert, setAlertStyle: alert_style];
        
        alert
    }
}

fn run_alert(alert: &NSAlert) -> NSModalResponse {
    unsafe {
        let response: NSModalResponse = msg_send![alert, runModal];
        response
    }
}

// Implementation of public functions

pub fn message_box_ok(title: &str, message: &str, icon: MessageBoxIcon) {
    let title = title.to_string();
    let message = message.to_string();
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            let alert = create_alert(&title, &message, icon);
            let button = to_ns_string("OK");
            let _: () = msg_send![&alert, addButtonWithTitle: &*button];
            
            run_alert(&alert);
        }
    });
}

pub fn message_box_ok_cancel(title: &str, message: &str, icon: MessageBoxIcon, default: OkCancel) -> OkCancel {
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            let alert = create_alert(title, message, icon);
            
            // Add buttons in reverse order as they're positioned from right to left
            let cancel_button = to_ns_string("Cancel");
            let _: () = msg_send![&alert, addButtonWithTitle: &*cancel_button];
            
            let ok_button = to_ns_string("OK");
            let _: () = msg_send![&alert, addButtonWithTitle: &*ok_button];
            
            // Set default button
            let default_button = match default {
                OkCancel::Ok => 1, // Second button (first added)
                OkCancel::Cancel => 0, // First button (second added)
            };
            
            let _: () = msg_send![&alert, setInitialFirstResponder: 
                msg_send_id![&alert, buttons] 
                .objectAtIndex(default_button)];
            
            let response = run_alert(&alert);
            
            // NSAlertFirstButtonReturn is 1000, NSAlertSecondButtonReturn is 1001, etc.
            match response {
                1000 => OkCancel::Cancel, // First button
                1001 => OkCancel::Ok,     // Second button
                _ => OkCancel::Cancel,    // Default to Cancel
            }
        }
    })
}

pub fn message_box_yes_no(title: &str, message: &str, icon: MessageBoxIcon, default: YesNo) -> YesNo {
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            let alert = create_alert(title, message, icon);
            
            // Add buttons in reverse order
            let no_button = to_ns_string("No");
            let _: () = msg_send![&alert, addButtonWithTitle: &*no_button];
            
            let yes_button = to_ns_string("Yes");
            let _: () = msg_send![&alert, addButtonWithTitle: &*yes_button];
            
            // Set default button
            let default_button = match default {
                YesNo::Yes => 1, // Second button (first added)
                YesNo::No => 0,  // First button (second added)
            };
            
            let _: () = msg_send![&alert, setInitialFirstResponder: 
                msg_send_id![&alert, buttons] 
                .objectAtIndex(default_button)];
            
            let response = run_alert(&alert);
            
            match response {
                1000 => YesNo::No,  // First button
                1001 => YesNo::Yes, // Second button
                _ => YesNo::No,     // Default to No
            }
        }
    })
}

pub fn message_box_yes_no_cancel(title: &str, message: &str, icon: MessageBoxIcon, default: YesNoCancel) -> YesNoCancel {
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            let alert = create_alert(title, message, icon);
            
            // Add buttons in reverse order
            let cancel_button = to_ns_string("Cancel");
            let _: () = msg_send![&alert, addButtonWithTitle: &*cancel_button];
            
            let no_button = to_ns_string("No");
            let _: () = msg_send![&alert, addButtonWithTitle: &*no_button];
            
            let yes_button = to_ns_string("Yes");
            let _: () = msg_send![&alert, addButtonWithTitle: &*yes_button];
            
            // Set default button
            let default_button = match default {
                YesNoCancel::Yes => 2,    // Third button (first added)
                YesNoCancel::No => 1,     // Second button (second added)
                YesNoCancel::Cancel => 0, // First button (third added)
            };
            
            let _: () = msg_send![&alert, setInitialFirstResponder: 
                msg_send_id![&alert, buttons] 
                .objectAtIndex(default_button)];
            
            let response = run_alert(&alert);
            
            match response {
                1000 => YesNoCancel::Cancel, // First button
                1001 => YesNoCancel::No,     // Second button
                1002 => YesNoCancel::Yes,    // Third button
                _ => YesNoCancel::Cancel,    // Default to Cancel
            }
        }
    })
}

pub fn input_box(title: &str, message: &str, default: Option<&str>) -> Option<String> {
    let title = title.to_string();
    let message = message.to_string();
    let default_str = default.map(|s| s.to_string());
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            // Create alert with text field
            let alert = create_alert(&title, &message, MessageBoxIcon::Info);
            
            // Add buttons
            let cancel_button = to_ns_string("Cancel");
            let _: () = msg_send![&alert, addButtonWithTitle: &*cancel_button];
            
            let ok_button = to_ns_string("OK");
            let _: () = msg_send![&alert, addButtonWithTitle: &*ok_button];
            
            // Add text field
            let _: () = msg_send![&alert, setShowsHelp: false];
            
            // Create and configure text field
            let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(200.0, 24.0));
            let text_field: objc2::rc::Retained<NSTextField> = {
                let mtm = MainThreadMarker::new().unwrap();
                NSTextField::alloc(mtm).initWithFrame(frame)
            };
            
            if let Some(default_text) = default_str {
                let ns_default = to_ns_string(&default_text);
                let _: () = msg_send![&text_field, setStringValue: &*ns_default];
            }
            
            // Set secure text entry for password fields
            if default.is_none() {
                let _: () = msg_send![&text_field, setSecure: true];
            }
            
            let _: () = msg_send![&alert, setAccessoryView: &*text_field];
            
            // Show alert and get response
            let response = run_alert(&alert);
            
            if response == 1001 {  // OK button
                let value: objc2::rc::Retained<NSString> = msg_send_id![&text_field, stringValue];
                Some(value.to_string())
            } else {
                None
            }
        }
    })
}

pub fn save_file_dialog(title: &str, path: &str, filter_patterns: &[&str], description: &str) -> Option<String> {
    let title = title.to_string();
    let path = path.to_string();
    let filter_patterns: Vec<String> = filter_patterns.iter().map(|&s| s.to_string()).collect();
    let description = description.to_string();
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            // Create save panel
            let save_panel: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSSavePanel), savePanel];
            
            // Configure panel
            let ns_title = to_ns_string(&title);
            let _: () = msg_send![&save_panel, setTitle: &*ns_title];
            
            // Set initial directory if provided
            if !path.is_empty() {
                if let Some(dir) = Path::new(&path).parent() {
                    if let Some(dir_str) = dir.to_str() {
                        let ns_dir = to_ns_string(dir_str);
                        let url: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSURL), fileURLWithPath: &*ns_dir];
                        let _: () = msg_send![&save_panel, setDirectoryURL: &*url];
                    }
                }
                
                // Set default filename
                if let Some(filename) = Path::new(&path).file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        let ns_filename = to_ns_string(filename_str);
                        let _: () = msg_send![&save_panel, setNameFieldStringValue: &*ns_filename];
                    }
                }
            }
            
            // Setup file type filtering
            if !filter_patterns.is_empty() {
                let allowed_types: Vec<objc2::rc::Retained<NSString>> = filter_patterns
                    .iter()
                    .map(|p| {
                        // Extract extension from pattern (*.ext -> ext)
                        let ext = p.trim_start_matches("*.");
                        to_ns_string(ext)
                    })
                    .collect();
                
                let ns_array: objc2::rc::Retained<NSArray<NSString>> = 
                    NSArray::from_retained_slice(&allowed_types);
                let _: () = msg_send![&save_panel, setAllowedFileTypes: &*ns_array];
                
                let _: () = msg_send![&save_panel, setAllowsOtherFileTypes: false];
            }
            
            // Show panel and get result
            let mtm = MainThreadMarker::new().unwrap();
            let app: objc2::rc::Retained<NSApplication> = 
                NSApplication::sharedApplication(mtm);
            let response: NSModalResponse = msg_send![&save_panel, runModal];
            
            if response == NSModalResponseOK {
                let url: objc2::rc::Retained<NSObject> = msg_send_id![&save_panel, URL];
                let path: objc2::rc::Retained<NSString> = msg_send_id![&url, path];
                Some(path.to_string())
            } else {
                None
            }
        }
    })
}

pub fn open_file_dialog(title: &str, path: &str, filter_patterns: &[&str], description: &str, 
                    allow_multi: bool) -> Option<Vec<String>> {
    let title = title.to_string();
    let path = path.to_string();
    let filter_patterns: Vec<String> = filter_patterns.iter().map(|&s| s.to_string()).collect();
    let description = description.to_string();
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            // Create open panel
            let open_panel: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSOpenPanel), openPanel];
            
            // Configure panel
            let ns_title = to_ns_string(&title);
            let _: () = msg_send![&open_panel, setTitle: &*ns_title];
            let _: () = msg_send![&open_panel, setCanChooseFiles: true];
            let _: () = msg_send![&open_panel, setCanChooseDirectories: false];
            let _: () = msg_send![&open_panel, setAllowsMultipleSelection: allow_multi];
            
            // Set initial directory if provided
            if !path.is_empty() {
                let ns_path = to_ns_string(&path);
                let url: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSURL), fileURLWithPath: &*ns_path];
                let _: () = msg_send![&open_panel, setDirectoryURL: &*url];
            }
            
            // Setup file type filtering
            if !filter_patterns.is_empty() {
                let allowed_types: Vec<objc2::rc::Retained<NSString>> = filter_patterns
                    .iter()
                    .map(|p| {
                        // Extract extension from pattern (*.ext -> ext)
                        let ext = p.trim_start_matches("*.");
                        to_ns_string(ext)
                    })
                    .collect();
                
                let ns_array: objc2::rc::Retained<NSArray<NSString>> = 
                    NSArray::from_retained_slice(&allowed_types);
                let _: () = msg_send![&open_panel, setAllowedFileTypes: &*ns_array];
            }
            
            // Show panel and get result
            let mtm = MainThreadMarker::new().unwrap();
            let _app: objc2::rc::Retained<NSApplication> = 
                NSApplication::sharedApplication(mtm);
            let response: NSModalResponse = msg_send![&open_panel, runModal];
            
            if response == NSModalResponseOK {
                let urls: objc2::rc::Retained<NSArray<NSObject>> = msg_send_id![&open_panel, URLs];
                let count: usize = msg_send![&urls, count];
                
                let mut files = Vec::with_capacity(count);
                for i in 0..count {
                    let url: objc2::rc::Retained<NSObject> = msg_send_id![&urls, objectAtIndex: i];
                    let path: objc2::rc::Retained<NSString> = msg_send_id![&url, path];
                    files.push(path.to_string());
                }
                
                Some(files)
            } else {
                None
            }
        }
    })
}

pub fn select_folder_dialog(title: &str, path: &str) -> Option<String> {
    let title = title.to_string();
    let path = path.to_string();
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            // Create open panel
            let open_panel: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSOpenPanel), openPanel];
            
            // Configure panel
            let ns_title = to_ns_string(&title);
            let _: () = msg_send![&open_panel, setTitle: &*ns_title];
            let _: () = msg_send![&open_panel, setCanChooseFiles: false];
            let _: () = msg_send![&open_panel, setCanChooseDirectories: true];
            let _: () = msg_send![&open_panel, setAllowsMultipleSelection: false];
            
            // Set initial directory if provided
            if !path.is_empty() {
                let ns_path = to_ns_string(&path);
                let url: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSURL), fileURLWithPath: &*ns_path];
                let _: () = msg_send![&open_panel, setDirectoryURL: &*url];
            }
            
            // Show panel and get result
            let mtm = MainThreadMarker::new().unwrap();
            let _app: objc2::rc::Retained<NSApplication> = 
                NSApplication::sharedApplication(mtm);
            let response: NSModalResponse = msg_send![&open_panel, runModal];
            
            if response == NSModalResponseOK {
                let url: objc2::rc::Retained<NSObject> = msg_send_id![&open_panel, URL];
                let path: objc2::rc::Retained<NSString> = msg_send_id![&url, path];
                Some(path.to_string())
            } else {
                None
            }
        }
    })
}

pub fn color_chooser_dialog(title: &str, default: DefaultColorValue) -> Option<(String, [u8; 3])> {
    let title = title.to_string();
    let default_owned = match default {
        DefaultColorValue::Hex(hex) => DefaultColorValue::Hex(hex.to_string()),
        DefaultColorValue::RGB(rgb) => DefaultColorValue::RGB(rgb),
    };
    
    ensure_main_thread(move || {
        let _ = get_cocoa_functions();
        
        unsafe {
            // Get default color values
            let default_rgb = match &default_owned {
                DefaultColorValue::Hex(hex) => super::hex_to_rgb(hex),
                DefaultColorValue::RGB(rgb) => *rgb,
            };
            
            // Create color panel
            let color_panel: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSColorPanel), sharedColorPanel];
            let _: () = msg_send![&color_panel, setShowsAlpha: false];
            
            // Set initial color
            let r = default_rgb[0] as f64 / 255.0;
            let g = default_rgb[1] as f64 / 255.0;
            let b = default_rgb[2] as f64 / 255.0;
            
            let color: objc2::rc::Retained<NSObject> = msg_send_id![class!(NSColor), 
                                                   colorWithSRGBRed: r, green: g, blue: b, alpha: 1.0];
            let _: () = msg_send![&color_panel, setColor: &*color];
            
            // Set custom title
            let ns_title = to_ns_string(&title);
            let _: () = msg_send![&color_panel, setTitle: &*ns_title];
            
            // Show panel modally (this is a bit tricky in AppKit)
            let _: () = msg_send![&color_panel, orderFront: std::ptr::null_mut::<NSObject>()];
            
            // Create a custom modal runloop
            let mtm = MainThreadMarker::new().unwrap();
            let app: objc2::rc::Retained<NSApplication> = 
                NSApplication::sharedApplication(mtm);
            let result: NSModalResponse = msg_send![&app, runModalForWindow: &*color_panel];
            
            if result == NSModalResponseOK {
                // Get selected color
                let selected_color: objc2::rc::Retained<NSObject> = msg_send_id![&color_panel, color];
                
                // Get RGB components
                let mut r: f64 = 0.0;
                let mut g: f64 = 0.0;
                let mut b: f64 = 0.0;
                let mut a: f64 = 0.0;
                
                let _: () = msg_send![&selected_color, getRed: &mut r, green: &mut g, blue: &mut b, alpha: &mut a];
                
                let rgb = [
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                ];
                
                let hex = super::rgb_to_hex(&rgb);
                Some((hex, rgb))
            } else {
                None
            }
        }
    })
}