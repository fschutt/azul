//! Roles and what they allow: the one role vocabulary of a space and the capabilities it maps to
//! (planning/architecture.md section 5.4 `reg_grant.role`, section 7.2 "Resource roles"; build unit
//! S16, lane W1-S16b).
//!
//! THE ONLY ROLE TABLE of the engine (COMMON-W1R R14): the registry sees a grant as capabilities
//! (`GrantCaps { read, write, manage }`), and the SPACES-APPLY glue maps [`Caps`] to it; no other crate
//! keeps a second table of what a role may do.
//!
//! | role | read | write | manage |
//! |---|---|---|---|
//! | `manager` | yes | yes | yes |
//! | `editor` | yes | yes | no |
//! | `viewer` | yes | no | no |
//! | `uploader` | no | yes | no |
//!
//! An `uploader` is write-only: it can put objects into the space and cannot read any (it gets no
//! wrap of the space key). The capabilities of several grants are UNIONED (a viewer through one grant
//! and an uploader through another can read and write): drift row X-DELTA-W1S16B-8.

use core::fmt;

use azlin_seams::mutant;

/// The role of a grant: the one role vocabulary of a space.
///
/// Implements planning/architecture.md section 5.4 ("`role` (**manager, editor, viewer, uploader**: the
/// one role vocabulary)") and section 7.2 (space: manager, editor, viewer, uploader; conversation:
/// manager, member).
///
/// Caller: `Grant::role`, `SpaceOp::Grant`, `role_caps`; S9 (`reg_grant.role`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceRole {
    /// Reads, writes and manages the space: grants, revokes, rekeys, switches the history mode.
    Manager,
    /// Reads and writes.
    Editor,
    /// Reads.
    Viewer,
    /// Writes only: cannot read.
    Uploader,
}

words!(
    SpaceRole,
    "space role",
    [
        Manager => "manager",
        Editor => "editor",
        Viewer => "viewer",
        Uploader => "uploader",
    ]
);

/// The read bit.
const READ_BIT: u8 = 0b001;
/// The write bit.
const WRITE_BIT: u8 = 0b010;
/// The manage bit.
const MANAGE_BIT: u8 = 0b100;

/// What a principal may do in a space: read, write, manage. A set of three bits; no arithmetic on it
/// escapes (the engine reads it with `can_read`, `can_write` and `can_manage`, and combines two with
/// [`union`]).
///
/// Implements planning/architecture.md section 5.4 (a role is what a grant gives) and section 7.2
/// (space roles map to grants).
///
/// Caller: `role_caps`, `access` (what a node checks per connection), `charge`; the SPACES-APPLY glue
/// (maps it to the registry's `GrantCaps`, COMMON-W1R R14).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Caps(u8);

impl Caps {
    /// No capability at all.
    pub const NONE: Caps = Caps(0);
    /// The capability to read.
    pub const READ: Caps = Caps(READ_BIT);
    /// The capability to write.
    pub const WRITE: Caps = Caps(WRITE_BIT);
    /// The capability to manage: grant, revoke, rekey, switch the history mode.
    pub const MANAGE: Caps = Caps(MANAGE_BIT);

    /// Whether the principal may read.
    pub const fn can_read(self) -> bool {
        self.0 & READ_BIT != 0
    }

    /// Whether the principal may write.
    pub const fn can_write(self) -> bool {
        self.0 & WRITE_BIT != 0
    }

    /// Whether the principal may manage the space.
    pub const fn can_manage(self) -> bool {
        self.0 & MANAGE_BIT != 0
    }

    /// Whether no capability is held.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every capability of `other` is held.
    pub const fn contains(self, other: Caps) -> bool {
        self.0 & other.0 == other.0
    }

    /// The capabilities held in both (the read-only limit of an archived space).
    pub const fn intersect(self, other: Caps) -> Caps {
        Caps(self.0 & other.0)
    }
}

/// `Caps(R|W|M)`, `Caps(R)` or `Caps(-)`.
impl fmt::Debug for Caps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Caps(")?;
        if self.is_empty() {
            f.write_str("-")?;
        } else {
            let mut first = true;
            for (held, letter) in [
                (self.can_read(), "R"),
                (self.can_write(), "W"),
                (self.can_manage(), "M"),
            ] {
                if held {
                    if !first {
                        f.write_str("|")?;
                    }
                    f.write_str(letter)?;
                    first = false;
                }
            }
        }
        f.write_str(")")
    }
}

/// The capabilities of a role: manager = read, write and manage; editor = read and write; viewer =
/// read; uploader = write only (it cannot read).
///
/// Implements planning/architecture.md section 5.4 (the one role vocabulary) and section 7.2 (resource
/// roles, mapped to grants).
///
/// Caller: `Grant::caps`, `access`, `charge`, `admit` (what the signer's grants allow); the
/// SPACES-APPLY glue (the only role-to-capability table of the engine, COMMON-W1R R14).
pub fn role_caps(role: SpaceRole) -> Caps {
    match role {
        SpaceRole::Manager => Caps(READ_BIT | WRITE_BIT | MANAGE_BIT),
        SpaceRole::Editor => Caps(READ_BIT | WRITE_BIT),
        SpaceRole::Viewer => {
            if mutant!("spaces_viewer_writes") {
                Caps(READ_BIT | WRITE_BIT)
            } else {
                Caps(READ_BIT)
            }
        }
        SpaceRole::Uploader => {
            if mutant!("spaces_uploader_reads") {
                Caps(READ_BIT | WRITE_BIT)
            } else {
                Caps(WRITE_BIT)
            }
        }
    }
}

/// The capabilities of two grants together: every capability either has. A viewer and an uploader
/// together read and write (drift row X-DELTA-W1S16B-8: "uploader ... its capabilities when combined
/// with viewer are not stated; smallest answer: capabilities union").
///
/// Implements planning/architecture.md section 5.4 (a member may hold several grants: directly and
/// through groups) and section 7.2.
///
/// Caller: `access` (the effective capabilities of a principal), `admit` (the signer's capabilities),
/// `charge`.
pub fn union(a: Caps, b: Caps) -> Caps {
    if mutant!("spaces_union_is_intersection") {
        return Caps(a.0 & b.0);
    }
    Caps(a.0 | b.0)
}
