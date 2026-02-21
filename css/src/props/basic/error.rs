/// Simple "invalid value" error, used for basic parsing failures
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of InvalidValueErr with String.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InvalidValueErrOwned {
    pub value: String,
}

impl<'a> InvalidValueErr<'a> {
    pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned { value: self.0.to_string() }
    }
}

impl InvalidValueErrOwned {
    pub fn to_shared<'a>(&'a self) -> InvalidValueErr<'a> {
        InvalidValueErr(&self.value)
    }
}
