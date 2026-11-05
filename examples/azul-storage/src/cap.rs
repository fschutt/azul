//! What a probe of this computer could not answer, and what the check that asked does about it
//! (ARCH28 RF49 / AR5a-9).
//!
//! A probe of the platform - the free space of a disk, whether a folder can be made private -
//! cannot always answer: Windows has no `statvfs` here yet, a network share may not say, a
//! sandbox may refuse. A probe that returns `None` or `0` lets its caller substitute the
//! permissive value without anyone deciding to: the disk-reserve guard was off on Windows for
//! exactly that reason (`free_space(..).unwrap_or(u64::MAX)`), and nothing said so.
//!
//! A probe returns [`Cap`] instead, which has no `Default` and no `unwrap_or`: the check that
//! asked matches it, and what it does when the answer is [`Cap::Unknown`] is a [`Posture`] written
//! next to the check, where a reader of the check sees it - stop ([`Posture::FailClosed`]) or go
//! on and say what was not checked ([`Posture::Degrade`]).

/// What a probe answered: the value, or why it could not say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Cap<T> {
    Known(T),
    /// The probe could not answer; why, as a sentence a report can carry.
    Unknown(&'static str),
}

impl<T> Cap<T> {
    /// The answer through `f`; an unknown stays unknown.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Cap<U> {
        match self {
            Cap::Known(value) => Cap::Known(f(value)),
            Cap::Unknown(why) => Cap::Unknown(why),
        }
    }

    /// Whether the probe could answer.
    #[must_use]
    pub fn is_known(&self) -> bool {
        matches!(self, Cap::Known(_))
    }
}

/// What a check does when its probe is [`Cap::Unknown`]. Chosen per check and written next to it,
/// never inside the probe and never as a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posture {
    /// Without the answer the step the check guards does not happen.
    FailClosed,
    /// The step happens anyway and the report says what was not checked: the sentence.
    Degrade(&'static str),
}
