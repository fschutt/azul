//! `Ctx`: what an envelope wraps, as a typed value with a strict grammar
//! (planning/architecture.md section 5.5: "`ctx` | what is wrapped (`space/<id>/epoch/<n>`);
//! also the associated data"; build unit S15, lane W1-S15b).
//!
//! The context is part of the AEAD's associated data and of the signature, and `open` demands
//! the EXACT context the caller expects, so an envelope made for one thing can never be
//! presented as another (another space, another epoch, another kind). The grammar is P-3 of the
//! brief (DRIFT X-DELTA-W1S15B-6: the plan gives only the first of the forms):
//!
//! ```text
//! space/<id>/epoch/<n>     a space key, epoch n
//! group/<id>/epoch/<n>     a group key, epoch n
//! member/<id>/key          a member key (wrapped to a device)
//! chain/<id>/epoch/<n>     epoch n of a space, sealed under epoch n+1 (the epoch chain)
//! seal/<id>/epoch/<n>      an account's seal key, epoch n
//! escrow/<id>              a member key wrapped to the escrow key
//! recovery/<id>/<method>   a member key under a recovery path: code, prf or share
//! ```
//!
//! `<id>` is 32 lowercase hex digits (the id of the space, group, member or account), `<n>` is a
//! plain decimal epoch from 1 to 4,294,967,295 without sign or leading zero. A context is at
//! most 128 bytes, lowercase letters, digits and `/` only, no empty segment; anything else is
//! refused by [`Ctx::parse`], so there is exactly one text per value (the signature covers it).
//!
//! ```
//! use azlin_keys::envelope::Ctx;
//! use azlin_keys::{Epoch, SpaceId};
//!
//! let space = SpaceId::from_bytes([0xab; 16]);
//! let ctx = Ctx::space_epoch(&space, Epoch::new(3).unwrap());
//! assert_eq!(ctx.as_str(), "space/abababababababababababababababab/epoch/3");
//! assert_eq!(Ctx::parse(ctx.as_str()), Ok(ctx));
//! assert!(Ctx::parse("Space/abababababababababababababababab/epoch/3").is_err());
//! ```

use core::fmt;
use core::str::FromStr;

use crate::ids::{parse_hex, AccountId, Epoch, GroupId, MemberId, SpaceId};

/// The longest context, in bytes.
///
/// Caller: [`Ctx::parse`], the wire decoder.
pub const MAX_CTX_LEN: usize = 128;

/// What kind of thing a context names.
///
/// Caller: S15c2 (`validate_row` compares the kind of a `reg_wrap` row with the context's
/// kind), [`Ctx::kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CtxKind {
    /// `space/<id>/epoch/<n>`.
    Space,
    /// `group/<id>/epoch/<n>`.
    Group,
    /// `member/<id>/key`.
    Member,
    /// `chain/<id>/epoch/<n>`.
    Chain,
    /// `seal/<id>/epoch/<n>`.
    Seal,
    /// `escrow/<id>`.
    Escrow,
    /// `recovery/<id>/<method>`.
    Recovery,
}

impl CtxKind {
    /// The first segment of the context text.
    pub const fn word(self) -> &'static str {
        match self {
            CtxKind::Space => "space",
            CtxKind::Group => "group",
            CtxKind::Member => "member",
            CtxKind::Chain => "chain",
            CtxKind::Seal => "seal",
            CtxKind::Escrow => "escrow",
            CtxKind::Recovery => "recovery",
        }
    }
}

/// The recovery paths of section 5.5 that wrap a member key: the recovery code (algorithm 3),
/// the passkey PRF (algorithm 4) and a contact's share (algorithm 5).
///
/// Caller: S15d (`Ctx::recovery` for each of its wraps).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecoveryMethod {
    /// The recovery code (`recovery/<id>/code`).
    Code,
    /// The passkey PRF (`recovery/<id>/prf`).
    Prf,
    /// A contact's Shamir share (`recovery/<id>/share`).
    Share,
}

impl RecoveryMethod {
    /// The last segment of the context text.
    pub const fn word(self) -> &'static str {
        match self {
            RecoveryMethod::Code => "code",
            RecoveryMethod::Prf => "prf",
            RecoveryMethod::Share => "share",
        }
    }

    /// The method with this word; `None` for every other text.
    pub fn from_word(word: &str) -> Option<RecoveryMethod> {
        match word {
            "code" => Some(RecoveryMethod::Code),
            "prf" => Some(RecoveryMethod::Prf),
            "share" => Some(RecoveryMethod::Share),
            _ => None,
        }
    }
}

/// Why a text is not a context.
///
/// Caller: [`Ctx::parse`], `EnvelopeError::BadCtx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CtxError {
    /// More than [`MAX_CTX_LEN`] bytes.
    TooLong,
    /// A character other than a lowercase letter, a digit or `/` (an uppercase letter, a dot, a
    /// space, anything outside ASCII).
    BadCharacter,
    /// Two slashes in a row, or a slash at the start or the end.
    EmptySegment,
    /// The first segment is none of the seven kinds.
    UnknownKind,
    /// The kind is right and the segments are not: too many, too few, or a fixed word (`epoch`,
    /// `key`) in the wrong place.
    BadShape,
    /// The id is not exactly 32 lowercase hex digits.
    BadId,
    /// The epoch is not a plain decimal number from 1 to 4,294,967,295.
    BadEpoch,
    /// The recovery method is none of `code`, `prf`, `share`.
    BadMethod,
}

impl fmt::Display for CtxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CtxError::TooLong => "the context is longer than 128 bytes",
            CtxError::BadCharacter => "the context holds a character that is not allowed",
            CtxError::EmptySegment => "the context has an empty segment",
            CtxError::UnknownKind => "the context names an unknown kind",
            CtxError::BadShape => "the context has the wrong segments for its kind",
            CtxError::BadId => "the context holds an id that is not 32 lowercase hex digits",
            CtxError::BadEpoch => "the context holds an epoch that is not a plain decimal number",
            CtxError::BadMethod => "the context names an unknown recovery method",
        })
    }
}

impl std::error::Error for CtxError {}

/// What an envelope wraps. Made only by the typed constructors or by [`Ctx::parse`], so every
/// value is in the grammar above, and equal values are equal texts.
///
/// Caller: INT-S15-1, S15c1 (`space_epoch`, `group_epoch`, `member_key`, `chain`), S15c2
/// (`validate_row` reads `kind`, `id_bytes`, `epoch`), S15d (`recovery`), S17 (`seal`,
/// `escrow`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ctx {
    text: String,
    kind: CtxKind,
    id: [u8; 16],
    epoch: Option<Epoch>,
    method: Option<RecoveryMethod>,
}

impl Ctx {
    /// `space/<id>/epoch/<n>`: the key of a space, epoch `epoch`.
    pub fn space_epoch(space: &SpaceId, epoch: Epoch) -> Ctx {
        Ctx {
            text: format!("space/{space}/epoch/{epoch}"),
            kind: CtxKind::Space,
            id: *space.as_bytes(),
            epoch: Some(epoch),
            method: None,
        }
    }

    /// `group/<id>/epoch/<n>`: the key of a group, epoch `epoch`.
    pub fn group_epoch(group: &GroupId, epoch: Epoch) -> Ctx {
        Ctx {
            text: format!("group/{group}/epoch/{epoch}"),
            kind: CtxKind::Group,
            id: *group.as_bytes(),
            epoch: Some(epoch),
            method: None,
        }
    }

    /// `member/<id>/key`: a member's key, wrapped to one of the member's devices.
    pub fn member_key(member: &MemberId) -> Ctx {
        Ctx {
            text: format!("member/{member}/key"),
            kind: CtxKind::Member,
            id: *member.as_bytes(),
            epoch: None,
            method: None,
        }
    }

    /// `chain/<id>/epoch/<n>`: epoch `epoch` of a space, sealed under epoch n+1 (the epoch
    /// chain of section 5.5).
    pub fn chain(space: &SpaceId, epoch: Epoch) -> Ctx {
        Ctx {
            text: format!("chain/{space}/epoch/{epoch}"),
            kind: CtxKind::Chain,
            id: *space.as_bytes(),
            epoch: Some(epoch),
            method: None,
        }
    }

    /// `seal/<id>/epoch/<n>`: the seal key of an account, epoch `epoch` (the key under which
    /// the team log's `sealed` fields are encrypted).
    pub fn seal(account: &AccountId, epoch: Epoch) -> Ctx {
        Ctx {
            text: format!("seal/{account}/epoch/{epoch}"),
            kind: CtxKind::Seal,
            id: *account.as_bytes(),
            epoch: Some(epoch),
            method: None,
        }
    }

    /// `escrow/<id>`: a member's key, wrapped to the escrow key.
    pub fn escrow(member: &MemberId) -> Ctx {
        Ctx {
            text: format!("escrow/{member}"),
            kind: CtxKind::Escrow,
            id: *member.as_bytes(),
            epoch: None,
            method: None,
        }
    }

    /// `recovery/<id>/<method>`: a member's key, wrapped under a recovery path.
    pub fn recovery(member: &MemberId, method: RecoveryMethod) -> Ctx {
        Ctx {
            text: format!("recovery/{member}/{}", method.word()),
            kind: CtxKind::Recovery,
            id: *member.as_bytes(),
            epoch: None,
            method: Some(method),
        }
    }

    /// The context of this text, or why the text is none. Strict: at most 128 bytes, only
    /// lowercase letters, digits and `/`, no empty segment, and exactly one of the seven forms.
    pub fn parse(text: &str) -> Result<Ctx, CtxError> {
        if text.len() > MAX_CTX_LEN {
            return Err(CtxError::TooLong);
        }
        if !text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'/')
        {
            return Err(CtxError::BadCharacter);
        }
        let parts: Vec<&str> = text.split('/').collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(CtxError::EmptySegment);
        }
        // `*parts.as_slice()` matches the slice place itself, so the elements are plain `&str`.
        match *parts.as_slice() {
            ["space", id, "epoch", n] => Ctx::finish(text, CtxKind::Space, id, Some(n), None),
            ["group", id, "epoch", n] => Ctx::finish(text, CtxKind::Group, id, Some(n), None),
            ["chain", id, "epoch", n] => Ctx::finish(text, CtxKind::Chain, id, Some(n), None),
            ["seal", id, "epoch", n] => Ctx::finish(text, CtxKind::Seal, id, Some(n), None),
            ["member", id, "key"] => Ctx::finish(text, CtxKind::Member, id, None, None),
            ["escrow", id] => Ctx::finish(text, CtxKind::Escrow, id, None, None),
            ["recovery", id, method] => {
                Ctx::finish(text, CtxKind::Recovery, id, None, Some(method))
            }
            [first, ..]
                if !matches!(
                    first,
                    "space" | "group" | "chain" | "seal" | "member" | "escrow" | "recovery"
                ) =>
            {
                Err(CtxError::UnknownKind)
            }
            _ => Err(CtxError::BadShape),
        }
    }

    /// Builds the context of an accepted shape: the id, the epoch and the method are parsed
    /// strictly, so the input text IS the canonical text of the value.
    fn finish(
        text: &str,
        kind: CtxKind,
        id: &str,
        epoch: Option<&str>,
        method: Option<&str>,
    ) -> Result<Ctx, CtxError> {
        let id = parse_hex::<16>(id).map_err(|_| CtxError::BadId)?;
        let epoch = match epoch {
            Some(n) => Some(n.parse::<Epoch>().map_err(|_| CtxError::BadEpoch)?),
            None => None,
        };
        let method = match method {
            Some(word) => Some(RecoveryMethod::from_word(word).ok_or(CtxError::BadMethod)?),
            None => None,
        };
        Ok(Ctx {
            text: text.to_owned(),
            kind,
            id,
            epoch,
            method,
        })
    }

    /// The text of the context, as it is written in the envelope.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The kind of thing the context names.
    pub fn kind(&self) -> CtxKind {
        self.kind
    }

    /// The bytes of the id in the context (a space, group, member or account id, by the kind).
    pub fn id_bytes(&self) -> &[u8; 16] {
        &self.id
    }

    /// The epoch of a `space`, `group`, `chain` or `seal` context; `None` for the others.
    pub fn epoch(&self) -> Option<Epoch> {
        self.epoch
    }

    /// The method of a `recovery` context; `None` for the others.
    pub fn method(&self) -> Option<RecoveryMethod> {
        self.method
    }
}

/// The text of the context.
impl fmt::Display for Ctx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The same as [`Ctx::parse`].
impl FromStr for Ctx {
    type Err = CtxError;

    fn from_str(text: &str) -> Result<Ctx, CtxError> {
        Ctx::parse(text)
    }
}
