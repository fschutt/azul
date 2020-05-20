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
extern crate azul_css;
extern crate azul_native_style;
#[cfg(target_arch = "wasm32")]
extern crate azul_web;
#[cfg(not(target_arch = "wasm32"))]
extern crate azul_desktop;

use core::ffi::c_void;
use azul_core::dom::Dom;
use azul_core::callbacks::{RefAny, LayoutInfo};
use azul_css::Css;
use azul_core::window::WindowCreateOptions;
#[cfg(not(target_arch = "wasm32"))]
use azul_desktop::app::{App, AppConfig};
#[cfg(target_arch = "wasm32")]
use azul_web::app::{App, AppConfig};