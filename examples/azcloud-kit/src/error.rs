//! Why an account, state-folder, sync or share call failed, as one type the apps can tell apart:
//! the token server's refusal ([`TokenError`]: a drive token this device must sign in again for,
//! no answer, ...), the bucket's ([`DriveError`]), the keyring's ([`KeyringError`]), a file of
//! this computer, or anything else as a sentence - with what was being done in front of it
//! ([`Context`]). The sentences name entries, keys and files, never a secret's value.

use std::fmt;

use azul_storage::{keyring::KeyringError, DriveError};

use crate::token::TokenError;

/// Why an account, sync or share call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudError {
    /// The token server could not help.
    Token(TokenError),
    /// The bucket (or a folder of this computer seen as a drive) could not help.
    Drive(DriveError),
    /// The keyring could not help (none on this system, refused, failed).
    Keyring(KeyringError),
    /// A file of this computer: the state folder, a synced folder.
    Io(String),
    /// Anything else, as a sentence: a key that cannot be one, a damaged index, a plan stopped
    /// as a mass delete.
    Failed(String),
    /// What was being done, and why it failed.
    Context {
        what: String,
        error: Box<CloudError>,
    },
}

/// A result of the kit's account, state, sync and share calls.
pub type CloudResult<T> = Result<T, CloudError>;

impl CloudError {
    /// The sentence `text` as an error.
    #[must_use]
    pub fn failed(text: impl Into<String>) -> CloudError {
        CloudError::Failed(text.into())
    }

    /// This error with `what` (what was being done) in front.
    #[must_use]
    pub fn context(self, what: impl fmt::Display) -> CloudError {
        CloudError::Context {
            what: what.to_string(),
            error: Box::new(self),
        }
    }

    /// The error under every [`CloudError::Context`].
    #[must_use]
    pub fn root(&self) -> &CloudError {
        match self {
            CloudError::Context { error, .. } => error.root(),
            other => other,
        }
    }

    /// The token server refused this device's drive token (revoked, reused, unknown): retrying
    /// does not help, signing in to (joining) the drive again does.
    #[must_use]
    pub fn is_sign_in(&self) -> bool {
        matches!(self.root(), CloudError::Token(TokenError::SignIn(_)))
    }
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudError::Token(e) => write!(f, "{e}"),
            CloudError::Drive(e) => write!(f, "{e}"),
            CloudError::Keyring(e) => write!(f, "{e}"),
            CloudError::Io(e) | CloudError::Failed(e) => write!(f, "{e}"),
            CloudError::Context { what, error } => write!(f, "{what}: {error}"),
        }
    }
}

impl std::error::Error for CloudError {}

impl From<TokenError> for CloudError {
    fn from(e: TokenError) -> Self {
        CloudError::Token(e)
    }
}

impl From<DriveError> for CloudError {
    fn from(e: DriveError) -> Self {
        CloudError::Drive(e)
    }
}

impl From<KeyringError> for CloudError {
    fn from(e: KeyringError) -> Self {
        CloudError::Keyring(e)
    }
}

impl From<std::io::Error> for CloudError {
    fn from(e: std::io::Error) -> Self {
        CloudError::Io(e.to_string())
    }
}

/// What was being done, in front of the error of any result whose error is a [`CloudError`] or
/// turns into one.
pub trait Context<T> {
    /// The error with `what` in front.
    fn context(self, what: impl fmt::Display) -> CloudResult<T>;
    /// The error with what `what` says in front (made only when there is an error).
    fn with_context<D: fmt::Display>(self, what: impl FnOnce() -> D) -> CloudResult<T>;
}

impl<T, E: Into<CloudError>> Context<T> for Result<T, E> {
    fn context(self, what: impl fmt::Display) -> CloudResult<T> {
        self.map_err(|e| e.into().context(what))
    }

    fn with_context<D: fmt::Display>(self, what: impl FnOnce() -> D) -> CloudResult<T> {
        self.map_err(|e| e.into().context(what()))
    }
}

/// Returns a [`CloudError::Failed`] with the sentence `format!` makes of the arguments.
macro_rules! fail {
    ($($arg:tt)*) => {
        return Err($crate::error::CloudError::Failed(format!($($arg)*)))
    };
}
pub(crate) use fail;
