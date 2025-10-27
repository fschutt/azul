//! Dynamic loading for X11 and EGL libraries.

use super::defines::*;
use crate::desktop::shell2::common::{dlopen::load_first_available, DlError, DynamicLibrary};
use std::{mem, rc::Rc};

// Helper for loading symbols and transpiling them to function pointers
macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

/// Dynamically loaded Xlib functions
pub struct Xlib {
    _lib: Box<dyn DynamicLibrary>,
    pub XOpenDisplay: XOpenDisplay,
    pub XCloseDisplay: XCloseDisplay,
    pub XDefaultScreen: XDefaultScreen,
    pub XRootWindow: XRootWindow,
    pub XCreateWindow: XCreateWindow,
    pub XMapWindow: XMapWindow,
    pub XStoreName: XStoreName,
    pub XInternAtom: XInternAtom,
    pub XSetWMProtocols: XSetWMProtocols,
    pub XSelectInput: XSelectInput,
    pub XPending: XPending,
    pub XNextEvent: XNextEvent,
    pub XLookupString: XLookupString,
    pub XMoveResizeWindow: XMoveResizeWindow,
    pub XDestroyWindow: XDestroyWindow,
    pub XSendEvent: XSendEvent,
    pub XCreateGC: XCreateGC,
    pub XFreeGC: XFreeGC,
    pub XSetForeground: XSetForeground,
    pub XDrawString: XDrawString,
    pub XFlush: XFlush,
}

impl Xlib {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available(&["libX11.so.6", "libX11.so"])?;
        Ok(Rc::new(Self {
            XOpenDisplay: load_symbol!(lib, XOpenDisplay, "XOpenDisplay"),
            XCloseDisplay: load_symbol!(lib, XCloseDisplay, "XCloseDisplay"),
            XDefaultScreen: load_symbol!(lib, XDefaultScreen, "XDefaultScreen"),
            XRootWindow: load_symbol!(lib, XRootWindow, "XRootWindow"),
            XCreateWindow: load_symbol!(lib, XCreateWindow, "XCreateWindow"),
            XMapWindow: load_symbol!(lib, XMapWindow, "XMapWindow"),
            XStoreName: load_symbol!(lib, XStoreName, "XStoreName"),
            XInternAtom: load_symbol!(lib, XInternAtom, "XInternAtom"),
            XSetWMProtocols: load_symbol!(lib, XSetWMProtocols, "XSetWMProtocols"),
            XSelectInput: load_symbol!(lib, XSelectInput, "XSelectInput"),
            XPending: load_symbol!(lib, XPending, "XPending"),
            XNextEvent: load_symbol!(lib, XNextEvent, "XNextEvent"),
            XLookupString: load_symbol!(lib, XLookupString, "XLookupString"),
            XMoveResizeWindow: load_symbol!(lib, XMoveResizeWindow, "XMoveResizeWindow"),
            XDestroyWindow: load_symbol!(lib, XDestroyWindow, "XDestroyWindow"),
            XSendEvent: load_symbol!(lib, XSendEvent, "XSendEvent"),
            XCreateGC: load_symbol!(lib, XCreateGC, "XCreateGC"),
            XFreeGC: load_symbol!(lib, XFreeGC, "XFreeGC"),
            XSetForeground: load_symbol!(lib, XSetForeground, "XSetForeground"),
            XDrawString: load_symbol!(lib, XDrawString, "XDrawString"),
            XFlush: load_symbol!(lib, XFlush, "XFlush"),
            _lib: Box::from(lib),
        }))
    }
}

/// Dynamically loaded EGL functions
pub struct Egl {
    _lib: Box<dyn DynamicLibrary>,
    pub eglGetDisplay: eglGetDisplay,
    pub eglInitialize: eglInitialize,
    pub eglBindAPI: eglBindAPI,
    pub eglChooseConfig: eglChooseConfig,
    pub eglCreateContext: eglCreateContext,
    pub eglCreateWindowSurface: eglCreateWindowSurface,
    pub eglMakeCurrent: eglMakeCurrent,
    pub eglSwapBuffers: eglSwapBuffers,
    pub eglGetError: eglGetError,
}

impl Egl {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available(&["libEGL.so.1", "libEGL.so"])?;
        Ok(Rc::new(Self {
            eglGetDisplay: load_symbol!(lib, eglGetDisplay, "eglGetDisplay"),
            eglInitialize: load_symbol!(lib, eglInitialize, "eglInitialize"),
            eglBindAPI: load_symbol!(lib, eglBindAPI, "eglBindAPI"),
            eglChooseConfig: load_symbol!(lib, eglChooseConfig, "eglChooseConfig"),
            eglCreateContext: load_symbol!(lib, eglCreateContext, "eglCreateContext"),
            eglCreateWindowSurface: load_symbol!(lib, eglCreateWindowSurface, "eglCreateWindowSurface"),
            eglMakeCurrent: load_symbol!(lib, eglMakeCurrent, "eglMakeCurrent"),
            eglSwapBuffers: load_symbol!(lib, eglSwapBuffers, "eglSwapBuffers"),
            eglGetError: load_symbol!(lib, eglGetError, "eglGetError"),
            _lib: Box::from(lib),
        }))
    }
}
