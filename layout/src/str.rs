// extra string functions intended for C development

use std::fmt;

use azul_css::{AzString, StringVec};

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

impl strfmt::DisplayStr for FmtValue {
    fn display_str(&self, f: &mut strfmt::Formatter<'_, '_>) -> strfmt::Result<()> {
        use strfmt::DisplayStr;
        match self {
            FmtValue::Bool(v) => format!("{v:?}").display_str(f),
            FmtValue::Uchar(v) => v.display_str(f),
            FmtValue::Schar(v) => v.display_str(f),
            FmtValue::Ushort(v) => v.display_str(f),
            FmtValue::Sshort(v) => v.display_str(f),
            FmtValue::Uint(v) => v.display_str(f),
            FmtValue::Sint(v) => v.display_str(f),
            FmtValue::Ulong(v) => v.display_str(f),
            FmtValue::Slong(v) => v.display_str(f),
            FmtValue::Isize(v) => v.display_str(f),
            FmtValue::Usize(v) => v.display_str(f),
            FmtValue::Float(v) => v.display_str(f),
            FmtValue::Double(v) => v.display_str(f),
            FmtValue::Str(v) => v.as_str().display_str(f),
            FmtValue::StrVec(sv) => {
                "[".display_str(f)?;
                for (i, s) in sv.as_ref().iter().enumerate() {
                    if i != 0 {
                        ", ".display_str(f)?;
                    }
                    s.as_str().display_str(f)?;
                }
                "]".display_str(f)?;
                Ok(())
            }
        }
    }
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
    pub value: FmtValue,
}

azul_css::impl_vec!(
    FmtArg,
    FmtArgVec,
    FmtArgVecDestructor,
    FmtArgVecDestructorType
);
azul_css::impl_vec_clone!(FmtArg, FmtArgVec, FmtArgVecDestructor);
azul_css::impl_vec_debug!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialeq!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialord!(FmtArg, FmtArgVec);

pub fn fmt_string(format: AzString, args: FmtArgVec) -> String {
    use strfmt::Format;
    let format_map = args
        .iter()
        .map(|a| (a.key.clone().into_library_owned_string(), a.value.clone()))
        .collect();
    match format.as_str().format(&format_map) {
        Ok(o) => o,
        Err(e) => format!("{}", e),
    }
}