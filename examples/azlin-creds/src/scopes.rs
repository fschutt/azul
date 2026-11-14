//! Credential scopes: "`scopes` | **only narrows**: for links, requests, app principals, CI. A device
//! credential has none; nodes check grants in the registry" (planning/architecture.md section 5.11
//! "Credential v2"; section 6.8 "App principals": `scopes` (space, permissions); section 5.12: a link
//! credential is "limited to the link's grants", plus `request_upload` and `log_append`).
//!
//! A scope NARROWS: a credential with scopes can do only what some scope names, and never more than the
//! registry and the grants allow; a credential without scopes (a device credential) leaves the decision
//! to the registry. So there is no scope that means "everything", an unknown scope kind is REFUSED at
//! decode (never read as "allow all"), and [`Scopes::narrow`] is an intersection: it can only remove.
//!
//! The kinds are a closed list. The link, request and log scopes name one 16-byte id of the unit that
//! owns it (S16c's link record, S16f's paired request, S16a's comment log); an app principal's scope
//! names a space and the permissions on it ([`AppPerms`]: read, write, manage, the three capabilities
//! of the registry's grants). A later unit adds a kind by a new number here, in a new version of this
//! crate: an older verifier refuses it.
//!
//! [`Scopes`] is canonical by construction: sorted, without duplicates, at most [`MAX_SCOPES`], and at
//! most one app scope per space (the permissions of two are their union). The wire decoder REFUSES any
//! other form, so two encodings of one set never both verify.

use core::fmt;

use azlin_cbor::Value;
use azlin_regbase::SpaceId;

use crate::canon::{bytes_of, id16_of, uint_of, Short};
use crate::error::WireError;

/// The most scopes a credential carries (planning/architecture.md section 1.4 rule 4): an app
/// principal or a link names a handful, never hundreds ("no credential has to list an employee's
/// hundreds of spaces"). est.
pub const MAX_SCOPES: usize = 64;

/// The wire number of each scope kind.
const KIND_LINK_READ: u64 = 1;
const KIND_REQUEST_UPLOAD: u64 = 2;
const KIND_LOG_APPEND: u64 = 3;
const KIND_APP: u64 = 4;

/// One 16-byte reference of a scope: bytes, a `Debug` that prints four of them, no `Default`.
macro_rules! scope_ref {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 16]);

        impl $name {
            /// The number of bytes of the reference.
            pub const LEN: usize = 16;

            /// The reference with these bytes.
            pub const fn from_bytes(bytes: [u8; 16]) -> $name {
                $name(bytes)
            }

            /// The bytes of the reference.
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), Short(&self.0))
            }
        }
    };
}

scope_ref! {
    /// A link: the id of the link record whose grants a link credential is limited to (planning/
    /// architecture.md section 5.12 "Resolving").
    ///
    /// Caller: S16c's glue (the credential a `link_resolve` returns), the nodes' scope check.
    LinkRef
}

scope_ref! {
    /// A paired request a link credential may upload to (planning/architecture.md section 5.12
    /// "request_upload to the paired request").
    ///
    /// Caller: S16f's glue, the nodes' scope check.
    RequestRef
}

scope_ref! {
    /// An append log a link credential may append to: the comment log (section 5.12 "log_append to the
    /// comment log").
    ///
    /// Caller: S16a's and S16c's glue, the nodes' scope check.
    LogRef
}

/// The permissions of an app principal on one space: read, write and manage, the three capabilities of
/// a registry grant (S9g's `GrantCaps`). A non-empty set of bits; a bit this build does not know is
/// refused at decode.
///
/// Implements planning/architecture.md section 6.8 ("`scopes` (space, permissions)").
///
/// Caller: `AppScope` and `ScopedOp::App`; S9b (the app principal's registered scopes), S9g.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AppPerms(u8);

impl AppPerms {
    /// Read what the space holds.
    pub const READ: AppPerms = AppPerms(1);
    /// Write to the space.
    pub const WRITE: AppPerms = AppPerms(2);
    /// Manage the space (its grants and options).
    pub const MANAGE: AppPerms = AppPerms(4);

    /// The permissions of these bits; `None` for no bit at all and for a bit that is not defined.
    pub const fn new(bits: u8) -> Option<AppPerms> {
        if bits == 0 || bits & !7 != 0 {
            None
        } else {
            Some(AppPerms(bits))
        }
    }

    /// The bits.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether every permission of `other` is in this set.
    pub const fn contains(self, other: AppPerms) -> bool {
        self.0 & other.0 == other.0
    }

    /// The permissions of both.
    pub const fn union(self, other: AppPerms) -> AppPerms {
        AppPerms(self.0 | other.0)
    }

    /// The permissions that are in both; `None` when there is none.
    pub const fn intersection(self, other: AppPerms) -> Option<AppPerms> {
        AppPerms::new(self.0 & other.0)
    }
}

/// An app principal's scope: permissions on one space.
///
/// Implements planning/architecture.md section 6.8.
///
/// Caller: `Scope::App`; S9b and S9g (an app principal's credential is the narrowing of its row).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppScope {
    /// The space.
    pub space: SpaceId,
    /// What the principal may do there.
    pub perms: AppPerms,
}

impl fmt::Debug for AppScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AppScope {{ space: {:?}, perms: {:?} }}",
            Short(self.space.as_bytes()),
            self.perms
        )
    }
}

/// One scope of a credential. The order of the variants is the sort order of a [`Scopes`] list.
///
/// Implements planning/architecture.md section 5.11 (`scopes`: for links, requests, app principals, CI).
///
/// Caller: `Scopes`; S16c's glue builds the link kinds, S9b the app kind.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Read what a link grants.
    LinkRead {
        /// The link.
        link: LinkRef,
    },
    /// Upload to a paired request.
    RequestUpload {
        /// The request.
        request: RequestRef,
    },
    /// Append to a comment log.
    LogAppend {
        /// The log.
        log: LogRef,
    },
    /// An app principal's permissions on a space.
    App(AppScope),
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::LinkRead { link } => write!(f, "LinkRead({link:?})"),
            Scope::RequestUpload { request } => write!(f, "RequestUpload({request:?})"),
            Scope::LogAppend { log } => write!(f, "LogAppend({log:?})"),
            Scope::App(app) => write!(f, "App({app:?})"),
        }
    }
}

/// What an operation asks of a credential's scopes: one value per kind of scope, and `Unscoped` for an
/// operation no scope names (an account-level one: members, devices, policy). A credential WITH scopes
/// never allows an `Unscoped` operation: scopes only narrow.
///
/// Caller: S9g's `ConnRequest::scoped`; the nodes' verb handlers build it from the request.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ScopedOp {
    /// Reading through a link.
    LinkRead {
        /// The link.
        link: LinkRef,
    },
    /// Uploading to a request.
    RequestUpload {
        /// The request.
        request: RequestRef,
    },
    /// Appending to a log.
    LogAppend {
        /// The log.
        log: LogRef,
    },
    /// An app principal using a space with these permissions.
    App {
        /// The space.
        space: SpaceId,
        /// The permissions the operation needs.
        need: AppPerms,
    },
    /// An operation that no scope names.
    Unscoped,
}

/// Why a list of scopes was refused.
///
/// Caller: `Scopes::new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScopesError {
    /// More scopes than [`MAX_SCOPES`].
    #[error("{count} scopes, the most is {}", MAX_SCOPES)]
    TooMany {
        /// How many were offered.
        count: usize,
    },
}

/// The scopes of a credential: sorted, without duplicates, at most [`MAX_SCOPES`], one app scope per
/// space. The field is private; [`Scopes::new`] makes the canonical form, the wire decoder refuses any
/// other.
///
/// Implements planning/architecture.md section 5.11 ("`scopes` only narrows").
///
/// Caller: `CredentialClaims::scopes`; S9b (`GrantFacts::scopes`), S9g (`allows`, `is_subset_of`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scopes(Vec<Scope>);

/// The canonical form of a list: sorted, without duplicates, the app scopes of one space merged (the
/// union of their permissions: an operation is allowed when ANY scope allows it, so merging changes
/// nothing a credential can do).
fn normalize(mut scopes: Vec<Scope>) -> Vec<Scope> {
    scopes.sort();
    scopes.dedup();
    let mut out: Vec<Scope> = Vec::with_capacity(scopes.len());
    for scope in scopes {
        if let Scope::App(app) = scope {
            if let Some(Scope::App(last)) = out.last_mut() {
                if last.space == app.space {
                    last.perms = last.perms.union(app.perms);
                    continue;
                }
            }
        }
        out.push(scope);
    }
    out
}

/// Whether `scope` lets an operation `op` through.
fn covers(scope: &Scope, op: &ScopedOp) -> bool {
    match (scope, op) {
        (Scope::LinkRead { link: a }, ScopedOp::LinkRead { link: b }) => a == b,
        (Scope::RequestUpload { request: a }, ScopedOp::RequestUpload { request: b }) => a == b,
        (Scope::LogAppend { log: a }, ScopedOp::LogAppend { log: b }) => a == b,
        (Scope::App(app), ScopedOp::App { space, need }) => {
            app.space == *space && app.perms.contains(*need)
        }
        _ => false,
    }
}

/// Whether everything `inner` allows, `outer` allows too.
fn scope_within(inner: &Scope, outer: &Scope) -> bool {
    match (inner, outer) {
        (Scope::App(a), Scope::App(b)) => a.space == b.space && b.perms.contains(a.perms),
        (Scope::LinkRead { .. }, Scope::LinkRead { .. })
        | (Scope::RequestUpload { .. }, Scope::RequestUpload { .. })
        | (Scope::LogAppend { .. }, Scope::LogAppend { .. }) => inner == outer,
        _ => false,
    }
}

/// The scope that allows exactly what both allow, if there is one.
fn meet(a: &Scope, b: &Scope) -> Option<Scope> {
    match (a, b) {
        (Scope::App(x), Scope::App(y)) if x.space == y.space => {
            x.perms.intersection(y.perms).map(|perms| {
                Scope::App(AppScope {
                    space: x.space,
                    perms,
                })
            })
        }
        (Scope::LinkRead { .. }, Scope::LinkRead { .. })
        | (Scope::RequestUpload { .. }, Scope::RequestUpload { .. })
        | (Scope::LogAppend { .. }, Scope::LogAppend { .. })
            if a == b =>
        {
            Some(*a)
        }
        _ => None,
    }
}

impl Scopes {
    /// The scopes that allow nothing: the result of narrowing two lists that share nothing. (A
    /// credential never carries it: `verify_credential` refuses present-and-empty scopes.)
    pub const fn empty() -> Scopes {
        Scopes(Vec::new())
    }

    /// The scopes of these entries, in the canonical form (sorted, without duplicates, the app scopes
    /// of a space merged). More than [`MAX_SCOPES`] entries are refused, never truncated.
    ///
    /// Caller: S9b (the grant of an app principal), S16c's glue (a link credential), the tests.
    pub fn new(scopes: Vec<Scope>) -> Result<Scopes, ScopesError> {
        if scopes.len() > MAX_SCOPES {
            return Err(ScopesError::TooMany {
                count: scopes.len(),
            });
        }
        Ok(Scopes(normalize(scopes)))
    }

    /// The scopes in canonical order.
    pub fn as_slice(&self) -> &[Scope] {
        &self.0
    }

    /// How many scopes there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether some scope lets `op` through. Nothing is allowed that no scope names: an `Unscoped`
    /// operation, and any operation of another link, request, log or space, is refused.
    ///
    /// Caller: S9g (`check_connection`, step 9: "credential scopes against `scoped`").
    pub fn allows(&self, op: &ScopedOp) -> bool {
        self.0.iter().any(|scope| covers(scope, op))
    }

    /// The scopes that allow exactly what BOTH lists allow: an intersection. It can only remove:
    /// `a.narrow(b)` is within `a` and within `b`, equal to `b.narrow(a)`, and `a.narrow(a)` is `a`.
    ///
    /// Caller: S9g (an app principal's credential is the row's scopes narrowed by the request), S16c's
    /// glue (a link credential narrows the link's grants).
    pub fn narrow(&self, other: &Scopes) -> Scopes {
        let mut out = Vec::new();
        for a in &self.0 {
            for b in &other.0 {
                if let Some(m) = meet(a, b) {
                    out.push(m);
                }
            }
        }
        Scopes(normalize(out))
    }

    /// Whether everything these scopes allow, `other` allows too: every scope of this list is within a
    /// scope of `other` (an app scope is within one of the same space whose permissions include it).
    ///
    /// Caller: S9g (an app principal's credential whose scopes are not within its row's is refused
    /// `AppScopeWidened`).
    pub fn is_subset_of(&self, other: &Scopes) -> bool {
        self.0
            .iter()
            .all(|inner| other.0.iter().any(|outer| scope_within(inner, outer)))
    }

    /// The wire form: an array of scopes, each an array `[kind, ...]`.
    pub(crate) fn to_value(&self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|scope| match scope {
                    Scope::LinkRead { link } => {
                        Value::Array(vec![uint_of(KIND_LINK_READ), bytes_of(link.as_bytes())])
                    }
                    Scope::RequestUpload { request } => Value::Array(vec![
                        uint_of(KIND_REQUEST_UPLOAD),
                        bytes_of(request.as_bytes()),
                    ]),
                    Scope::LogAppend { log } => {
                        Value::Array(vec![uint_of(KIND_LOG_APPEND), bytes_of(log.as_bytes())])
                    }
                    Scope::App(app) => Value::Array(vec![
                        uint_of(KIND_APP),
                        bytes_of(app.space.as_bytes()),
                        uint_of(u64::from(app.perms.bits())),
                    ]),
                })
                .collect(),
        )
    }

    /// The scopes of a wire array, STRICTLY: every scope of a kind this build knows (else
    /// `UnknownScope`), strictly ascending (no duplicate), one app scope per space, at most
    /// [`MAX_SCOPES`].
    pub(crate) fn from_values(items: &[Value]) -> Result<Scopes, WireError> {
        if items.len() > MAX_SCOPES {
            return Err(WireError::TooMany);
        }
        let mut scopes: Vec<Scope> = Vec::with_capacity(items.len());
        for item in items {
            scopes.push(scope_from_value(item)?);
        }
        for pair in scopes.windows(2) {
            match pair {
                [a, b] if a < b => {}
                _ => return Err(WireError::NotCanonical),
            }
            if let [Scope::App(a), Scope::App(b)] = pair {
                if a.space == b.space {
                    return Err(WireError::NotCanonical);
                }
            }
        }
        Ok(Scopes(scopes))
    }
}

/// One scope of a wire array.
fn scope_from_value(item: &Value) -> Result<Scope, WireError> {
    let Value::Array(parts) = item else {
        return Err(WireError::Malformed);
    };
    match parts.as_slice() {
        [Value::Uint(KIND_LINK_READ), id] => Ok(Scope::LinkRead {
            link: LinkRef::from_bytes(id16_of(id)?),
        }),
        [Value::Uint(KIND_REQUEST_UPLOAD), id] => Ok(Scope::RequestUpload {
            request: RequestRef::from_bytes(id16_of(id)?),
        }),
        [Value::Uint(KIND_LOG_APPEND), id] => Ok(Scope::LogAppend {
            log: LogRef::from_bytes(id16_of(id)?),
        }),
        [Value::Uint(KIND_APP), space, Value::Uint(bits)] => {
            let bits = u8::try_from(*bits).map_err(|_| WireError::UnknownScope)?;
            let perms = AppPerms::new(bits).ok_or(WireError::UnknownScope)?;
            Ok(Scope::App(AppScope {
                space: SpaceId::from_bytes(id16_of(space)?),
                perms,
            }))
        }
        [Value::Uint(KIND_LINK_READ | KIND_REQUEST_UPLOAD | KIND_LOG_APPEND | KIND_APP), ..] => {
            Err(WireError::Malformed)
        }
        [Value::Uint(_), ..] => Err(WireError::UnknownScope),
        _ => Err(WireError::Malformed),
    }
}
