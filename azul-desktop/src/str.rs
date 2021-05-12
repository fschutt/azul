// extra string functions intended for C development

use azul_css::{AzString, StringVec};
use std::fmt;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum FmtValue {
    Bool(bool),
    Uchar(u8),
    Schar(i8),
    Ushort(u16),
    Sshort(i16),
    Uint(u32),
    Sint(i32),
    Ulong(u64),
    Slong(i64),
    Isize(isize),
    Usize(usize),
    Float(f32),
    Double(f64),
    Str(AzString),
    StrVec(StringVec),
}

impl fmt::Display for FmtValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FmtValue::Bool(v) => v.fmt(f),
            FmtValue::Uchar(v) => v.fmt(f),
            FmtValue::Schar(v) => v.fmt(f),
            FmtValue::Ushort(v) => v.fmt(f),
            FmtValue::Sshort(v) => v.fmt(f),
            FmtValue::Uint(v) => v.fmt(f),
            FmtValue::Sint(v) => v.fmt(f),
            FmtValue::Ulong(v) => v.fmt(f),
            FmtValue::Slong(v) => v.fmt(f),
            FmtValue::Isize(v) => v.fmt(f),
            FmtValue::Usize(v) => v.fmt(f),
            FmtValue::Float(v) => v.fmt(f),
            FmtValue::Double(v) => v.fmt(f),
            FmtValue::Str(v) => v.as_str().fmt(f),
            FmtValue::StrVec(sv) => {
                use std::fmt::Debug;
                let vec: Vec<&str> = sv.as_ref().iter().map(|s| s.as_str()).collect();
                vec.fmt(f)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FmtArg {
    pub key: AzString,
    pub value: FmtValue
}

impl_vec!(FmtArg, FmtArgVec, FmtArgVecDestructor);
impl_vec_clone!(FmtArg, FmtArgVec, FmtArgVecDestructor);
impl_vec_debug!(FmtArg, FmtArgVec);
impl_vec_partialeq!(FmtArg, FmtArgVec);
impl_vec_partialord!(FmtArg, FmtArgVec);

pub fn fmt_string(format: AzString, args: FmtArgVec) -> String {
    use strfmt::Format;
    let format_map = args.iter().map(|a| (a.key.clone().into_library_owned_string(), a.value.clone())).collect();
    match format.as_str().format(&format_map) {
        Ok(o) => o,
        Err(e) => format!("{}", e),
    }
}