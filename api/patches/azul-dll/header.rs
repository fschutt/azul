//! Public API for Azul
//!
//! A single function can have multiple implementations depending on whether it is
//! compiled for the Rust-desktop target, the Rust-wasm target or the C API.
//!
//! For now, the crate simply re-exports azul_core and calls the c_api functions

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![allow(dead_code)]
#![allow(unused_imports)]

extern crate azul_core;
#[cfg(target_arch = "wasm32")]
extern crate azul_web as azul_impl;
#[cfg(not(target_arch = "wasm32"))]
extern crate azul_desktop as azul_impl;

use core::ffi::c_void;
use std::{path::PathBuf, vec::Vec, string::String, time::Duration};
use azul_impl::{
    css::{self, *},
    dom::Dom,
    callbacks::{RefAny, LayoutInfo, Callback, CallbackInfo, GlCallbackInfo, GlCallbackReturn, IFrameCallbackInfo, IFrameCallbackReturn},
    window::WindowCreateOptions,
    resources::{RawImage, RawImageFormat, FontId, TextId, ImageId},
    app::{App, AppConfig}
};