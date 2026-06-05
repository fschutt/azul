//! Dynamic loading for X11 and related libraries (Xlib, EGL, xkbcommon,
//! GTK3 IM, XRender). Types such as `Xkb`, `Gtk3Im`, and `GtkIMContext`
//! are re-exported by the Wayland subsystem.

use std::{
    ffi::{c_char, c_void, CStr, CString},
    rc::Rc,
};

use super::defines::*;
// Re-export types from defines for convenience
pub use super::defines::{Atom, Display, Drawable, Window, XSetWindowAttributes, GC};
use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};
use crate::load_symbol;

/// Wrapper for dlopen, dlsym, dlclose.
pub struct Library {
    handle: *mut c_void,
    name: String,
}

impl DynamicLibraryTrait for Library {
    fn load(name: &str) -> Result<Self, DlError> {
        let c_name = CString::new(name).unwrap();
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY) };
        if handle.is_null() {
            let error = unsafe { CStr::from_ptr(libc::dlerror()).to_string_lossy() };
            Err(DlError::LibraryNotFound {
                name: name.to_string(),
                tried: vec![name.to_string()],
                suggestion: format!("dlopen failed: {}", error),
            })
        } else {
            Ok(Self {
                handle,
                name: name.to_string(),
            })
        }
    }

    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError> {
        let c_name = CString::new(name).unwrap();
        let sym = libc::dlsym(self.handle, c_name.as_ptr());
        if sym.is_null() {
            Err(DlError::SymbolNotFound {
                symbol: name.to_string(),
                library: self.name.clone(),
                suggestion: "Symbol not found in library".to_string(),
            })
        } else {
            assert_eq!(
                std::mem::size_of::<T>(),
                std::mem::size_of::<*mut c_void>(),
                "get_symbol: size mismatch between target type and pointer"
            );
            Ok(std::mem::transmute_copy(&sym))
        }
    }

    fn unload(&mut self) {
        if !self.handle.is_null() {
            unsafe { libc::dlclose(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        self.unload();
    }
}

/// Dynamically loaded Xlib functions
pub struct Xlib {
    _lib: Library,
    pub XOpenDisplay: XOpenDisplay,
    pub XCloseDisplay: XCloseDisplay,
    pub XDefaultScreen: XDefaultScreen,
    pub XRootWindow: XRootWindow,
    pub XCreateWindow: XCreateWindow,
    pub XCreateSimpleWindow: XCreateSimpleWindow,
    pub XMapWindow: XMapWindow,
    pub XStoreName: XStoreName,
    pub XInternAtom: XInternAtom,
    pub XSetWMProtocols: XSetWMProtocols,
    pub XSelectInput: XSelectInput,
    pub XPending: XPending,
    pub XNextEvent: XNextEvent,
    pub XFilterEvent: XFilterEvent,
    pub XLookupString: XLookupString,
    pub XMoveResizeWindow: XMoveResizeWindow,
    pub XMoveWindow: XMoveWindow,
    pub XDestroyWindow: XDestroyWindow,
    pub XSendEvent: XSendEvent,
    pub XCreateGC: XCreateGC,
    pub XFreeGC: XFreeGC,
    pub XSetForeground: XSetForeground,
    pub XFillRectangle: XFillRectangle,
    pub XClearWindow: XClearWindow,
    pub XDrawString: XDrawString,
    pub XFlush: XFlush,
    pub XSync: XSync,
    pub XConnectionNumber: XConnectionNumber,
    pub XSetLocaleModifiers: XSetLocaleModifiers,
    pub XOpenIM: XOpenIM,
    pub XCloseIM: XCloseIM,
    pub XCreateIC: XCreateIC,
    pub XDestroyIC: XDestroyIC,
    pub XSetICValues: XSetICValues,
    pub XGetIMValues: XGetIMValues,
    pub XVaCreateNestedList: XVaCreateNestedList,
    pub XmbLookupString: XmbLookupString,
    pub Xutf8LookupString: Xutf8LookupString,
    pub XSetICFocus: XSetICFocus,
    pub XUnsetICFocus: XUnsetICFocus,
    pub XGetInputFocus: XGetInputFocus,
    pub XGetErrorText: XGetErrorText,
    pub XSetErrorHandler: XSetErrorHandler,
    pub XChangeProperty: XChangeProperty,
    pub XChangeWindowAttributes: XChangeWindowAttributes,
    pub XGetWindowProperty: XGetWindowProperty,
    pub XFree: XFree,
    pub XResizeWindow: XResizeWindow,
    pub XUnmapWindow: XUnmapWindow,
    pub XCreateFontCursor: XCreateFontCursor,
    pub XDefineCursor: XDefineCursor,
    pub XFreeCursor: XFreeCursor,
    pub XDisplayWidth: XDisplayWidth,
    pub XDisplayHeight: XDisplayHeight,
    pub XDisplayWidthMM: XDisplayWidthMM,
    pub XDisplayHeightMM: XDisplayHeightMM,
    // ARGB visual / colormap functions
    pub XCreateColormap: XCreateColormap,
    pub XDefaultVisual: XDefaultVisual,
    pub XDefaultColormap: XDefaultColormap,
    pub XDefaultDepth: XDefaultDepth,
    pub XMatchVisualInfo: XMatchVisualInfo,
    pub XFreeColormap: XFreeColormap,
    // XImage functions for CPU rendering
    pub XCreateImage: XCreateImage,
    pub XPutImage: XPutImage,
    pub XDestroyImage: XDestroyImage,
    // XI2 generic-event cookie data + extension query (libX11)
    pub XGetEventData: XGetEventData,
    pub XFreeEventData: XFreeEventData,
    pub XQueryExtension: XQueryExtension,
}

impl Xlib {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libX11.so.6", "libX11.so"])?;
        Ok(Rc::new(Self {
            XOpenDisplay: load_symbol!(lib, _, "XOpenDisplay"),
            XCloseDisplay: load_symbol!(lib, _, "XCloseDisplay"),
            XDefaultScreen: load_symbol!(lib, _, "XDefaultScreen"),
            XRootWindow: load_symbol!(lib, _, "XRootWindow"),
            XCreateWindow: load_symbol!(lib, _, "XCreateWindow"),
            XCreateSimpleWindow: load_symbol!(lib, _, "XCreateSimpleWindow"),
            XMapWindow: load_symbol!(lib, _, "XMapWindow"),
            XStoreName: load_symbol!(lib, _, "XStoreName"),
            XInternAtom: load_symbol!(lib, _, "XInternAtom"),
            XSetWMProtocols: load_symbol!(lib, _, "XSetWMProtocols"),
            XSelectInput: load_symbol!(lib, _, "XSelectInput"),
            XPending: load_symbol!(lib, _, "XPending"),
            XNextEvent: load_symbol!(lib, _, "XNextEvent"),
            XFilterEvent: load_symbol!(lib, _, "XFilterEvent"),
            XLookupString: load_symbol!(lib, _, "XLookupString"),
            XMoveResizeWindow: load_symbol!(lib, _, "XMoveResizeWindow"),
            XMoveWindow: load_symbol!(lib, _, "XMoveWindow"),
            XDestroyWindow: load_symbol!(lib, _, "XDestroyWindow"),
            XSendEvent: load_symbol!(lib, _, "XSendEvent"),
            XCreateGC: load_symbol!(lib, _, "XCreateGC"),
            XFreeGC: load_symbol!(lib, _, "XFreeGC"),
            XSetForeground: load_symbol!(lib, _, "XSetForeground"),
            XFillRectangle: load_symbol!(lib, _, "XFillRectangle"),
            XClearWindow: load_symbol!(lib, _, "XClearWindow"),
            XDrawString: load_symbol!(lib, _, "XDrawString"),
            XFlush: load_symbol!(lib, _, "XFlush"),
            XSync: load_symbol!(lib, _, "XSync"),
            XConnectionNumber: load_symbol!(lib, _, "XConnectionNumber"),
            XSetLocaleModifiers: load_symbol!(lib, _, "XSetLocaleModifiers"),
            XOpenIM: load_symbol!(lib, _, "XOpenIM"),
            XCloseIM: load_symbol!(lib, _, "XCloseIM"),
            XCreateIC: load_symbol!(lib, _, "XCreateIC"),
            XDestroyIC: load_symbol!(lib, _, "XDestroyIC"),
            XSetICValues: load_symbol!(lib, _, "XSetICValues"),
            XGetIMValues: load_symbol!(lib, _, "XGetIMValues"),
            XVaCreateNestedList: load_symbol!(lib, _, "XVaCreateNestedList"),
            XmbLookupString: load_symbol!(lib, _, "XmbLookupString"),
            Xutf8LookupString: load_symbol!(lib, _, "Xutf8LookupString"),
            XSetICFocus: load_symbol!(lib, _, "XSetICFocus"),
            XUnsetICFocus: load_symbol!(lib, _, "XUnsetICFocus"),
            XGetInputFocus: load_symbol!(lib, _, "XGetInputFocus"),
            XGetErrorText: load_symbol!(lib, _, "XGetErrorText"),
            XSetErrorHandler: load_symbol!(lib, _, "XSetErrorHandler"),
            XChangeProperty: load_symbol!(lib, _, "XChangeProperty"),
            XChangeWindowAttributes: load_symbol!(lib, _, "XChangeWindowAttributes"),
            XGetWindowProperty: load_symbol!(lib, _, "XGetWindowProperty"),
            XFree: load_symbol!(lib, _, "XFree"),
            XResizeWindow: load_symbol!(lib, _, "XResizeWindow"),
            XUnmapWindow: load_symbol!(lib, _, "XUnmapWindow"),
            XCreateFontCursor: load_symbol!(lib, _, "XCreateFontCursor"),
            XDefineCursor: load_symbol!(lib, _, "XDefineCursor"),
            XFreeCursor: load_symbol!(lib, _, "XFreeCursor"),
            XDisplayWidth: load_symbol!(lib, _, "XDisplayWidth"),
            XDisplayHeight: load_symbol!(lib, _, "XDisplayHeight"),
            XDisplayWidthMM: load_symbol!(lib, _, "XDisplayWidthMM"),
            XDisplayHeightMM: load_symbol!(lib, _, "XDisplayHeightMM"),
            // ARGB visual / colormap functions
            XCreateColormap: load_symbol!(lib, _, "XCreateColormap"),
            XDefaultVisual: load_symbol!(lib, _, "XDefaultVisual"),
            XDefaultColormap: load_symbol!(lib, _, "XDefaultColormap"),
            XDefaultDepth: load_symbol!(lib, _, "XDefaultDepth"),
            XMatchVisualInfo: load_symbol!(lib, _, "XMatchVisualInfo"),
            XFreeColormap: load_symbol!(lib, _, "XFreeColormap"),
            // XImage functions for CPU rendering
            XCreateImage: load_symbol!(lib, _, "XCreateImage"),
            XPutImage: load_symbol!(lib, _, "XPutImage"),
            XDestroyImage: load_symbol!(lib, _, "XDestroyImage"),
            XGetEventData: load_symbol!(lib, _, "XGetEventData"),
            XFreeEventData: load_symbol!(lib, _, "XFreeEventData"),
            XQueryExtension: load_symbol!(lib, _, "XQueryExtension"),
            _lib: lib,
        }))
    }
}

/// Dynamically loaded XInput2 (libXi) functions — touch + pen/tablet feed.
pub struct Xi {
    _lib: Library,
    pub XIQueryVersion: XIQueryVersion,
    pub XISelectEvents: XISelectEvents,
    pub XIQueryDevice: XIQueryDevice,
    pub XIFreeDeviceInfo: XIFreeDeviceInfo,
}

impl Xi {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libXi.so.6", "libXi.so"])?;
        Ok(Rc::new(Self {
            XIQueryVersion: load_symbol!(lib, _, "XIQueryVersion"),
            XISelectEvents: load_symbol!(lib, _, "XISelectEvents"),
            XIQueryDevice: load_symbol!(lib, _, "XIQueryDevice"),
            XIFreeDeviceInfo: load_symbol!(lib, _, "XIFreeDeviceInfo"),
            _lib: lib,
        }))
    }
}

/// Dynamically loaded EGL functions
pub struct Egl {
    _lib: Library,
    pub eglGetDisplay: eglGetDisplay,
    pub eglInitialize: eglInitialize,
    pub eglBindAPI: eglBindAPI,
    pub eglChooseConfig: eglChooseConfig,
    pub eglCreateContext: eglCreateContext,
    pub eglCreateWindowSurface: eglCreateWindowSurface,
    pub eglMakeCurrent: eglMakeCurrent,
    pub eglSwapBuffers: eglSwapBuffers,
    pub eglSwapInterval: eglSwapInterval,
    pub eglGetError: eglGetError,
    pub eglGetProcAddress: eglGetProcAddress,
    pub eglDestroySurface: eglDestroySurface,
    pub eglDestroyContext: eglDestroyContext,
    pub eglTerminate: eglTerminate,
}

impl Egl {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libEGL.so.1", "libEGL.so"])?;
        Ok(Rc::new(Self {
            eglGetDisplay: load_symbol!(lib, _, "eglGetDisplay"),
            eglInitialize: load_symbol!(lib, _, "eglInitialize"),
            eglBindAPI: load_symbol!(lib, _, "eglBindAPI"),
            eglChooseConfig: load_symbol!(lib, _, "eglChooseConfig"),
            eglCreateContext: load_symbol!(lib, _, "eglCreateContext"),
            eglCreateWindowSurface: load_symbol!(lib, _, "eglCreateWindowSurface"),
            eglMakeCurrent: load_symbol!(lib, _, "eglMakeCurrent"),
            eglSwapBuffers: load_symbol!(lib, _, "eglSwapBuffers"),
            eglSwapInterval: load_symbol!(lib, _, "eglSwapInterval"),
            eglGetError: load_symbol!(lib, _, "eglGetError"),
            eglGetProcAddress: load_symbol!(lib, _, "eglGetProcAddress"),
            eglDestroySurface: load_symbol!(lib, _, "eglDestroySurface"),
            eglDestroyContext: load_symbol!(lib, _, "eglDestroyContext"),
            eglTerminate: load_symbol!(lib, _, "eglTerminate"),
            _lib: lib,
        }))
    }
}

/// Dynamically loaded xkbcommon functions
pub struct Xkb {
    _lib: Library,
    pub xkb_context_new: unsafe extern "C" fn(flags: u32) -> *mut xkb_context,
    pub xkb_context_unref: unsafe extern "C" fn(context: *mut xkb_context),
    pub xkb_keymap_new_from_names:
        unsafe extern "C" fn(*mut xkb_context, *const xkb_rule_names, u32) -> *mut xkb_keymap,
    pub xkb_keymap_new_from_string:
        unsafe extern "C" fn(*mut xkb_context, *const c_char, u32, u32) -> *mut xkb_keymap,
    pub xkb_keymap_unref: unsafe extern "C" fn(keymap: *mut xkb_keymap),
    pub xkb_state_new: unsafe extern "C" fn(keymap: *mut xkb_keymap) -> *mut xkb_state,
    pub xkb_state_unref: unsafe extern "C" fn(state: *mut xkb_state),
    pub xkb_state_update_mask:
        unsafe extern "C" fn(*mut xkb_state, u32, u32, u32, u32, u32, u32) -> u32,
    pub xkb_state_key_get_one_sym: unsafe extern "C" fn(*mut xkb_state, u32) -> u32,
    pub xkb_state_key_get_utf8: unsafe extern "C" fn(*mut xkb_state, u32, *mut i8, usize) -> i32,
}

impl Xkb {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libxkbcommon.so.0"])?;
        Ok(Rc::new(Self {
            xkb_context_new: load_symbol!(lib, _, "xkb_context_new"),
            xkb_context_unref: load_symbol!(lib, _, "xkb_context_unref"),
            xkb_keymap_new_from_names: load_symbol!(lib, _, "xkb_keymap_new_from_names"),
            xkb_keymap_new_from_string: load_symbol!(lib, _, "xkb_keymap_new_from_string"),
            xkb_keymap_unref: load_symbol!(lib, _, "xkb_keymap_unref"),
            xkb_state_new: load_symbol!(lib, _, "xkb_state_new"),
            xkb_state_unref: load_symbol!(lib, _, "xkb_state_unref"),
            xkb_state_update_mask: load_symbol!(lib, _, "xkb_state_update_mask"),
            xkb_state_key_get_one_sym: load_symbol!(lib, _, "xkb_state_key_get_one_sym"),
            xkb_state_key_get_utf8: load_symbol!(lib, _, "xkb_state_key_get_utf8"),
            _lib: lib,
        }))
    }
}

/// Dynamically loaded GTK3 IM context functions for IME support
pub struct Gtk3Im {
    _lib: Library,
    pub gtk_im_context_simple_new: unsafe extern "C" fn() -> *mut GtkIMContext,
    pub gtk_im_context_set_cursor_location:
        unsafe extern "C" fn(*mut GtkIMContext, *const GdkRectangle),
    pub gtk_im_context_focus_in: unsafe extern "C" fn(*mut GtkIMContext),
    pub gtk_im_context_focus_out: unsafe extern "C" fn(*mut GtkIMContext),
    pub gtk_im_context_reset: unsafe extern "C" fn(*mut GtkIMContext),
}

// Opaque GTK types
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GtkIMContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GdkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Gtk3Im {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libgtk-3.so.0", "libgtk-3.so"])?;
        Ok(Rc::new(Self {
            gtk_im_context_simple_new: load_symbol!(lib, _, "gtk_im_context_simple_new"),
            gtk_im_context_set_cursor_location: load_symbol!(
                lib,
                _,
                "gtk_im_context_set_cursor_location"
            ),
            gtk_im_context_focus_in: load_symbol!(lib, _, "gtk_im_context_focus_in"),
            gtk_im_context_focus_out: load_symbol!(lib, _, "gtk_im_context_focus_out"),
            gtk_im_context_reset: load_symbol!(lib, _, "gtk_im_context_reset"),
            _lib: lib,
        }))
    }
}

/// Dynamically loaded XRender functions for ARGB visual detection
/// See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
pub struct Xrender {
    _lib: Library,
    pub XRenderFindVisualFormat: XRenderFindVisualFormat,
}

impl Xrender {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available::<Library>(&["libXrender.so.1", "libXrender.so"])?;
        Ok(Rc::new(Self {
            XRenderFindVisualFormat: load_symbol!(lib, _, "XRenderFindVisualFormat"),
            _lib: lib,
        }))
    }
}
