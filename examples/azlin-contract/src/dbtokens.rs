//! The eight control databases, who writes each, who holds which token, and the descriptor of a
//! connection (planning/architecture.md 4.1 "Databases: one writer each" and 4.2 "Connections: Turso or
//! plain libsql"). Section 11 asks the tests to parse "the database tokens": azlin-proto's doc tests read
//! the plan's two tables and hold them to [`DB_TABLE`] and [`ACCESS`].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

text_enum! {
    /// The eight databases (4.1): "three environments mean eight databases each".
    pub enum Database {
        Ctl = "ctl",
        Rel = "rel",
        Ops = "ops",
        Accounts = "accounts",
        Biz = "biz",
        Fleet = "fleet",
        Tele = "tele",
        Bulk = "bulk",
    }
}

text_enum! {
    /// Who writes a database (4.1 first table, column "Writer").
    pub enum Writer {
        AzCtl = "AzCtl",
        Payments = "payments",
        /// Nodes, each writing only its own signed rows (`fleet`, `tele`, `bulk`).
        Nodes = "nodes",
    }
}

text_enum! {
    /// What a storage node's token reaches (4.1 first table, column "Node token").
    pub enum NodeToken {
        ReadOnly = "RO",
        NoToken = "none",
        ReadWriteOwnRows = "RW own rows",
    }
}

/// One row of the plan's first table: the database, its writer and what a node's token reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbRow {
    pub db: Database,
    pub writer: Writer,
    pub node_token: NodeToken,
}

/// The first table of 4.1. `tele` is also written by the watcher and the health prober (signed rows of
/// their own): that is [`ACCESS`].
pub const DB_TABLE: &[DbRow] = &[
    DbRow {
        db: Database::Ctl,
        writer: Writer::AzCtl,
        node_token: NodeToken::ReadOnly,
    },
    DbRow {
        db: Database::Rel,
        writer: Writer::AzCtl,
        node_token: NodeToken::ReadOnly,
    },
    DbRow {
        db: Database::Ops,
        writer: Writer::AzCtl,
        node_token: NodeToken::NoToken,
    },
    DbRow {
        db: Database::Accounts,
        writer: Writer::Payments,
        node_token: NodeToken::NoToken,
    },
    DbRow {
        db: Database::Biz,
        writer: Writer::Payments,
        node_token: NodeToken::NoToken,
    },
    DbRow {
        db: Database::Fleet,
        writer: Writer::Nodes,
        node_token: NodeToken::ReadWriteOwnRows,
    },
    DbRow {
        db: Database::Tele,
        writer: Writer::Nodes,
        node_token: NodeToken::ReadWriteOwnRows,
    },
    DbRow {
        db: Database::Bulk,
        writer: Writer::Nodes,
        node_token: NodeToken::ReadWriteOwnRows,
    },
];

/// The holders of tokens (4.1 second table, the first column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Principal {
    Node,
    Watcher,
    HealthProber,
    Payments,
    AzCtl,
    NewServer,
}

impl Principal {
    pub const ALL: &'static [Principal] = &[
        Principal::Node,
        Principal::Watcher,
        Principal::HealthProber,
        Principal::Payments,
        Principal::AzCtl,
        Principal::NewServer,
    ];

    /// The row label of the plan's table.
    #[must_use]
    pub const fn doc_label(self) -> &'static str {
        match self {
            Principal::Node => "node",
            Principal::Watcher => "watcher (leader)",
            Principal::HealthProber => "health prober",
            Principal::Payments => "payments",
            Principal::AzCtl => "AzCtl",
            Principal::NewServer => "new server, before approval",
        }
    }
}

/// What a token reaches (4.1 second table, a cell). "Turso's published API offers no per-table
/// permissions, so a token is read-only or read-write for one whole database" ([`Access::token`]); what
/// separates the writers of a node-written database is the signature on every row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// `-`
    NoToken,
    /// `RO`
    ReadOnly,
    /// `RW`
    ReadWrite,
    /// `RW own`: read-write, and every row it writes is signed by the writer.
    ReadWriteOwnRows,
    /// `RO pages, RW own rows`: the health prober's `tele` token.
    ReadsPagesWritesOwnRows,
}

/// What the database issues for an [`Access`]: read-only or read-write for one whole database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenScope {
    None,
    ReadOnly,
    ReadWrite,
}

impl Access {
    /// Every access, longest label first (so a cell reads as the longest label it starts with).
    pub const ALL: &'static [Access] = &[
        Access::ReadsPagesWritesOwnRows,
        Access::ReadWriteOwnRows,
        Access::ReadWrite,
        Access::ReadOnly,
        Access::NoToken,
    ];

    /// How the plan's table writes the access (a cell may go on in parentheses).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Access::NoToken => "-",
            Access::ReadOnly => "RO",
            Access::ReadWrite => "RW",
            Access::ReadWriteOwnRows => "RW own",
            Access::ReadsPagesWritesOwnRows => "RO pages, RW own rows",
        }
    }

    #[must_use]
    pub const fn token(self) -> TokenScope {
        match self {
            Access::NoToken => TokenScope::None,
            Access::ReadOnly => TokenScope::ReadOnly,
            Access::ReadWrite | Access::ReadWriteOwnRows | Access::ReadsPagesWritesOwnRows => {
                TokenScope::ReadWrite
            }
        }
    }
}

use Access::{NoToken as No, ReadOnly as Ro, ReadWrite as Rw, ReadWriteOwnRows as Own};

/// The plan's second table: row = [`Principal::ALL`], column = [`Database::ALL`] (ctl, rel, ops,
/// accounts, biz, fleet, tele, bulk).
pub const ACCESS: [[Access; 8]; 6] = [
    // node
    [Ro, Ro, No, No, No, Own, Own, Own],
    // watcher (leader)
    [Ro, No, No, No, No, Own, Own, No],
    // health prober
    [Ro, No, No, No, No, No, Access::ReadsPagesWritesOwnRows, No],
    // payments
    [No, No, No, Rw, Rw, No, No, No],
    // AzCtl
    [Rw, Rw, Rw, Ro, Ro, Ro, Ro, Ro],
    // new server, before approval: only the `fleet` token in its `node.json`
    [No, No, No, No, No, Rw, No, No],
];

/// What `principal`'s token reaches in `db`.
#[must_use]
pub fn access(principal: Principal, db: Database) -> Access {
    // the enums are fieldless with the table's order (a test holds the two to each other)
    ACCESS[principal as usize][db as usize]
}

// ---- the descriptor of a connection (4.2) -------------------------------------------------------

/// `type` of a descriptor (4.2): "`libsql`, with `turso` and `sqld` as presets of it; or `node-local` on
/// a one-machine home cluster (2.21)".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DescriptorType {
    #[serde(rename = "libsql")]
    Libsql,
    #[serde(rename = "turso")]
    Turso,
    #[serde(rename = "sqld")]
    Sqld,
    #[serde(rename = "node-local")]
    NodeLocal,
}

/// `auth` of a descriptor: "a Turso or sqld JWT (`bearer`), or `basic` with user and password". The
/// secret travels SEALED to the node the document is for (4.1 "tokens sealed to each node"), so the
/// values here are sealed text, never a plain secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Auth {
    /// The JWT, sealed to this node.
    #[serde(rename = "bearer")]
    Bearer(String),
    #[serde(rename = "basic")]
    Basic {
        user: String,
        /// The password, sealed to this node.
        #[serde(rename = "password")]
        password_sealed: String,
    },
}

/// `roots` of a descriptor: "`system`, `turso`, a CA file, or a pinned hash" (the last two are an object
/// here: the plan does not say how they are written, X-DELTA-W1S5-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Roots {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "turso")]
    Turso,
    #[serde(rename = "ca_file")]
    CaFile(String),
    /// The hash of the pinned certificate, lowercase hex.
    #[serde(rename = "pinned_hash")]
    PinnedHash(String),
}

/// One database's descriptor, inside the signed endpoints document (4.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DbDescriptor {
    #[serde(rename = "type")]
    pub kind: DescriptorType,
    /// `libsql://` or `https://host:port` (a `node-local` one is a `file:` path).
    pub url: String,
    pub auth: Option<Auth>,
    pub roots: Option<Roots>,
    /// The read-only fallbacks for a grace period (4.1 "Rotating tokens and moving databases"); none when
    /// the key is absent.
    #[serde(default)]
    pub fallback: Vec<String>,
}

/// Why a descriptor or a document of descriptors is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    /// The text is not the JSON of a descriptor document (the reader's own words).
    Json(String),
    UnknownDatabase(String),
    MissingDatabase(&'static str),
    BadUrl(&'static str),
    AuthRoots(&'static str),
    BadFallback(&'static str),
}

impl DescriptorError {
    /// The stable name the fixtures record.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            DescriptorError::Json(_) => "json",
            DescriptorError::UnknownDatabase(_) => "unknown_database",
            DescriptorError::MissingDatabase(_) => "missing_database",
            DescriptorError::BadUrl(_) => "bad_url",
            DescriptorError::AuthRoots(_) => "auth_roots",
            DescriptorError::BadFallback(_) => "bad_fallback",
        }
    }
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DescriptorError::Json(why) => write!(f, "not the JSON of a descriptor document: {why}"),
            DescriptorError::UnknownDatabase(name) => {
                write!(f, "`{name}` is not one of the eight databases")
            }
            DescriptorError::MissingDatabase(name) => {
                write!(f, "the database `{name}` has no descriptor")
            }
            DescriptorError::BadUrl(name) => write!(
                f,
                "`{name}`: the url is not `libsql://` or `https://` (or `file:` for node-local)"
            ),
            DescriptorError::AuthRoots(name) => write!(
                f,
                "`{name}`: a connection needs `auth` and `roots`, and node-local has neither"
            ),
            DescriptorError::BadFallback(name) => write!(
                f,
                "`{name}`: a fallback url that is not `libsql://` or `https://`"
            ),
        }
    }
}

impl std::error::Error for DescriptorError {}

/// The object of descriptors, read so that a name twice is refused (serde's map would keep the last). Read
/// it with the JSON reader of the caller (`serde_json::from_str::<DescriptorDocument>`) and call
/// [`DescriptorDocument::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorDocument(pub BTreeMap<String, DbDescriptor>);

impl<'de> Deserialize<'de> for DescriptorDocument {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<DescriptorDocument, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = DescriptorDocument;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of database descriptors")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<DescriptorDocument, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((name, descriptor)) = map.next_entry::<String, DbDescriptor>()? {
                    if out.insert(name, descriptor).is_some() {
                        return Err(serde::de::Error::custom("a database named twice"));
                    }
                }
                Ok(DescriptorDocument(out))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

fn remote_url(url: &str) -> bool {
    url.strip_prefix("libsql://")
        .or_else(|| url.strip_prefix("https://"))
        .is_some_and(|rest| !rest.is_empty())
}

impl DbDescriptor {
    /// Checks what the JSON types cannot: the url scheme for the type, and `auth` and `roots` present
    /// exactly where the type has them.
    ///
    /// # Errors
    ///
    /// See [`DescriptorError`].
    pub fn check(&self, db: Database) -> Result<(), DescriptorError> {
        let name = db.name();
        match self.kind {
            DescriptorType::NodeLocal => {
                if !self.url.starts_with("file:") || self.url.len() == "file:".len() {
                    return Err(DescriptorError::BadUrl(name));
                }
                if self.auth.is_some() || self.roots.is_some() {
                    return Err(DescriptorError::AuthRoots(name));
                }
            }
            _ => {
                if !remote_url(&self.url) {
                    return Err(DescriptorError::BadUrl(name));
                }
                if self.auth.is_none() || self.roots.is_none() {
                    return Err(DescriptorError::AuthRoots(name));
                }
            }
        }
        if self.fallback.iter().any(|u| !remote_url(u)) {
            return Err(DescriptorError::BadFallback(name));
        }
        Ok(())
    }
}

impl DescriptorDocument {
    /// All eight descriptors, each valid for its database, and no other name.
    ///
    /// # Errors
    ///
    /// See [`DescriptorError`].
    pub fn validate(self) -> Result<BTreeMap<Database, DbDescriptor>, DescriptorError> {
        let mut out = BTreeMap::new();
        for (name, descriptor) in self.0 {
            let Some(db) = Database::parse(&name) else {
                return Err(DescriptorError::UnknownDatabase(name));
            };
            descriptor.check(db)?;
            out.insert(db, descriptor);
        }
        for db in Database::ALL {
            if !out.contains_key(db) {
                return Err(DescriptorError::MissingDatabase(db.name()));
            }
        }
        Ok(out)
    }
}
