use std::fmt;

/// Error returned by the `.run()` function
///
/// If the `.run()` function would panic, that would need `T` to
/// implement `Debug`, which is not necessary if we just return an error.
#[derive(Debug)]
pub enum RuntimeError {
    /// Error indexing into internal BTreeMap - wrong window ID
    WindowIndexError,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RuntimeError::*;
        match self {
            WindowIndexError => write!(f, "Invalid window index"),
        }
    }
}