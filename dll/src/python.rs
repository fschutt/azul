#![allow(non_snake_case)]

#[macro_use]
extern crate alloc;
extern crate azul_core;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub mod extra;
pub mod str;

pub mod azul_impl {
    #[cfg(target_arch = "wasm32")]
    pub use self::web::*;
    #[cfg(not(target_arch = "wasm32"))]
    pub use super::desktop::*;
}

use core::{ffi::c_void, mem};

use pyo3::{exceptions::PyException, prelude::*, types::*, PyObjectProtocol};
type GLuint = u32;
type AzGLuint = GLuint;
type GLint = i32;
type AzGLint = GLint;
type GLint64 = i64;
type AzGLint64 = GLint64;
type GLuint64 = u64;
type AzGLuint64 = GLuint64;
type GLenum = u32;
type AzGLenum = GLenum;
type GLintptr = isize;
type AzGLintptr = GLintptr;
type GLboolean = u8;
type AzGLboolean = GLboolean;
type GLsizeiptr = isize;
type AzGLsizeiptr = GLsizeiptr;
type GLvoid = c_void;
type AzGLvoid = GLvoid;
type GLbitfield = u32;
type AzGLbitfield = GLbitfield;
type GLsizei = i32;
type AzGLsizei = GLsizei;
type GLclampf = f32;
type AzGLclampf = GLclampf;
type GLfloat = f32;
type AzGLfloat = GLfloat;
type AzF32 = f32;
type AzU16 = u16;
type AzU32 = u32;
type AzScanCode = u32;

use pyo3::{PyGCProtocol, PyTraverseError, PyVisit};
