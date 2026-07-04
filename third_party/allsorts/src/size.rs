//! Definitions of the sizes of binary types.

use std::mem;

pub const U8: usize = mem::size_of::<u8>();
pub const I8: usize = mem::size_of::<i8>();
pub const U16: usize = mem::size_of::<u16>();
pub const I16: usize = mem::size_of::<i16>();
pub const U24: usize = 3;
pub const U32: usize = mem::size_of::<u32>();
pub const I32: usize = mem::size_of::<i32>();
pub const U64: usize = mem::size_of::<u64>();
pub const I64: usize = mem::size_of::<i64>();
