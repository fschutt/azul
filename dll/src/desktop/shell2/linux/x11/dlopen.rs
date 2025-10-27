//! Dynamic loading for X11 and related libraries.

use std::{
    ffi::{c_void, CStr, CString},
    rc::Rc,
};

use super::defines::*;
use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};

// Helper for loading symbols and casting them to function pointers
macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

// Wrapper for dlopen, dlsym, dlclose
pub struct Library {
    handle: *mut c_void,
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
            Ok(Self { handle })
        }
    }

    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError> {
        let c_name = CString::new(name).unwrap();
        let sym = libc::dlsym(self.handle, c_name.as_ptr());
        if sym.is_null() {
            Err(DlError::SymbolNotFound {
                symbol: name.to_string(),
                library: "unknown".to_string(),
                suggestion: "Symbol not found in library".to_string(),
            })
        } else {
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
    pub XFlush: XFlush,
    pub XSync: XSync,
    pub XSetLocaleModifiers: XSetLocaleModifiers,
    pub XOpenIM: XOpenIM,
    pub XCloseIM: XCloseIM,
    pub XCreateIC: XCreateIC,
    pub XDestroyIC: XDestroyIC,
    pub XmbLookupString: XmbLookupString,
    pub XSetICFocus: XSetICFocus,
    pub XUnsetICFocus: XUnsetICFocus,
    pub XGetInputFocus: XGetInputFocus,
    pub XGetErrorText: XGetErrorText,
    pub XSetErrorHandler: XSetErrorHandler,
    pub XChangeProperty: XChangeProperty,
    pub XChangeWindowAttributes: XChangeWindowAttributes,
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
            XFlush: load_symbol!(lib, _, "XFlush"),
            XSync: load_symbol!(lib, _, "XSync"),
            XSetLocaleModifiers: load_symbol!(lib, _, "XSetLocaleModifiers"),
            XOpenIM: load_symbol!(lib, _, "XOpenIM"),
            XCloseIM: load_symbol!(lib, _, "XCloseIM"),
            XCreateIC: load_symbol!(lib, _, "XCreateIC"),
            XDestroyIC: load_symbol!(lib, _, "XDestroyIC"),
            XmbLookupString: load_symbol!(lib, _, "XmbLookupString"),
            XSetICFocus: load_symbol!(lib, _, "XSetICFocus"),
            XUnsetICFocus: load_symbol!(lib, _, "XUnsetICFocus"),
            XGetInputFocus: load_symbol!(lib, _, "XGetInputFocus"),
            XGetErrorText: load_symbol!(lib, _, "XGetErrorText"),
            XSetErrorHandler: load_symbol!(lib, _, "XSetErrorHandler"),
            XChangeProperty: load_symbol!(lib, _, "XChangeProperty"),
            XChangeWindowAttributes: load_symbol!(lib, _, "XChangeWindowAttributes"),
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
    pub eglGetError: eglGetError,
    pub eglGetProcAddress: eglGetProcAddress,
    pub eglDestroySurface: eglDestroySurface,
    pub eglDestroyContext: eglDestroyContext,
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
            eglGetError: load_symbol!(lib, _, "eglGetError"),
            eglGetProcAddress: load_symbol!(lib, _, "eglGetProcAddress"),
            eglDestroySurface: load_symbol!(lib, _, "eglDestroySurface"),
            eglDestroyContext: load_symbol!(lib, _, "eglDestroyContext"),
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
        unsafe extern "C" fn(*mut xkb_context, *const i8, u32, u32) -> *mut xkb_keymap,
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
