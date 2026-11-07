//! What this device trusts, as three facts and nothing else.
//!
//! - The certificate authorities the Azlin apps pin: the iroh relays (n0's and the Azlin nodes')
//!   are served under Let's Encrypt, so a relay's TLS trusts Let's Encrypt's four roots and
//!   nothing else of the ~150 public ones. A forged relay certificate from any other authority is
//!   refused in the handshake. The token server's and the nodes' HTTPS keep the app's roots for
//!   now: their edge certificates rotate among several authorities, and which ones is the
//!   operator's to say ([`AZLIN_TLS_ROOTS_PEM`], empty until then).
//! - Whose traffic a request is ([`TrustClass`], RF39 2c, ENT27-M16): Azlin's - the token
//!   server, the blocks and the nodes of its drives, [`AZLIN_HOST_SUFFIXES`] - or anybody else's.
//!   The two have their own connection pools: Azlin's trusts the pinned authorities once they
//!   are named and follows no setting of a file; a foreign pool follows the app's roots. An Azlin
//!   transport refuses a host that is not Azlin's, a foreign one refuses an Azlin host
//!   ([`admits`]): a drive token or a signed request never rides a pool that was not made for it,
//!   and never goes to a host the service does not run.
//! - Whether a drive is encrypted ([`EncState`], feature `encryption`): one value with one
//!   owner ([`crate::AutoEncrypted`]), read by every app through the policy that names what it
//!   decides. "Could not tell" is a state of its own and never reads as "plain": the bucket's
//!   listing is the least trusted word there is, so it can add to what this device knows (key
//!   files listed make a drive encrypted) and never take from it.

/// Let's Encrypt's roots, by name: X1 (RSA), X2 (ECDSA) and the 2025 generation's YE and YR
/// (n0's relays chain to Root YE, cross-signed by X2). PEM text from letsencrypt.org.
pub const AZLIN_RELAY_ROOTS: [(&str, &str); 4] = [
    ("ISRG Root X1", include_str!("../roots/ISRGRootX1.pem")),
    ("ISRG Root X2", include_str!("../roots/ISRGRootX2.pem")),
    ("ISRG Root YE", include_str!("../roots/ISRGRootYE.pem")),
    ("ISRG Root YR", include_str!("../roots/ISRGRootYR.pem")),
];

/// The hosts that are Azlin's: the token server (`token.azlin.io`, `token-trial.azlin.io`), the
/// blocks and the nodes of its drives (`b1.s3.azlin.io`, `n1.s3.azlin.io`). A name ending in one
/// of these - or the name itself without its dot - is Azlin's ([`is_azlin_host`]).
pub const AZLIN_HOST_SUFFIXES: &[&str] = &[".azlin.io"];

/// The issuing certificate authorities of Azlin's hosts, PEM text, one or more `CERTIFICATE` blocks
/// each - the CURRENT ones and the NEXT (2 to 4 issuing CAs, never a leaf: a rotation must not lock
/// every client out).
///
/// OWNER / OPS INPUT: EMPTY until the operator says which authorities issue the token server's
/// edge certificate and the nodes' (ACME), and how they rotate - nothing is guessed here. Empty:
/// the Azlin class trusts what the app trusts ([`TrustClass::Azlin`]'s pool follows the app's
/// roots), exactly as before. The release procedure is to run the ignored test
/// `release_the_azlin_authorities_are_pinned` (`cargo test -p azul-storage --lib -- --ignored
/// release_`), which is red while the list is empty, and not to ship until it is green. Filled in,
/// the Azlin pool trusts these and nothing else, and a certificate from any other authority fails
/// the handshake with a sentence that names the pin.
pub const AZLIN_TLS_ROOTS_PEM: &[&str] = &[];

/// Whose traffic a transport carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrustClass {
    /// The Azlin service's: the token server and the blocks and nodes of its drives. Its
    /// requests carry a drive token or a signature for one of them.
    Azlin,
    /// Everybody else's: a data source, an OAuth provider, a feed, an address the user typed.
    Foreign,
}

/// The host of `url`, lower case, without brackets, port and trailing dot; `None` when it names
/// none. The user info is not part of it (`https://a@evil.example` is `evil.example`).
///
/// A URL whose authority parsers read differently names no host, so that the class check and the
/// client that connects cannot disagree on where a request goes: a backslash (WHATWG reads it as
/// a slash - `https://evil.example\@token.azlin.io/` goes to `evil.example`, a plain split says
/// `token.azlin.io`), a control character or a space between the scheme and the path, a percent
/// sign in the host, and a port that is no number. The path may hold what it likes.
#[must_use]
pub fn host_of_url(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority
        .chars()
        .any(|c| c == '\\' || c.is_control() || c.is_whitespace())
    {
        return None;
    }
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
            Some(_) => return None,
            None => authority,
        }
    };
    if host.contains('%') {
        return None;
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether `host` (as [`host_of_url`] gives it) is Azlin's: it ends in a suffix of
/// [`AZLIN_HOST_SUFFIXES`] on a label boundary, or is the suffix without its dot.
#[must_use]
pub fn is_azlin_host(host: &str) -> bool {
    AZLIN_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix) || host == suffix.trim_start_matches('.'))
}

/// Whether this is a TEST build (the E2E builds and the tests of the crates that turn on this
/// crate's `testing` or `test-hooks`): a stack on this computer, under any name, may stand in for
/// the service. A shipped build is never one.
#[must_use]
pub const fn test_build() -> bool {
    cfg!(any(feature = "testing", feature = "test-hooks"))
}

/// Whether a transport of `class` may send a request for `url`: `Ok`, or why not, as a sentence
/// (the host only - a URL may hold a password).
///
/// - Azlin: only to an Azlin host. A drive's endpoint and its nodes come from files and answers
///   (`drives.json`, the cache of a node list, a token server's bundle); the transport that
///   carries the drive's signed requests and its tokens does not follow them to a host the
///   service does not run. A TEST build (`test_build`) may reach a stack on this computer.
/// - Foreign: never to an Azlin host. That traffic belongs on the Azlin pool, where the pinned
///   authorities apply; a call site that forgot to say so fails loudly instead of riding the
///   wrong pool.
///
/// # Errors
///
/// The sentence, when `class` may not send to `url`.
pub fn admits(class: TrustClass, url: &str, test_build: bool) -> Result<(), String> {
    let Some(host) = host_of_url(url) else {
        return Err(String::from("the request names no host"));
    };
    match class {
        TrustClass::Azlin if is_azlin_host(&host) || test_build => Ok(()),
        TrustClass::Azlin => Err(format!(
            "{host} is not an Azlin host: the Azlin transport carries drive tokens and signed \
             requests to the service's own hosts only"
        )),
        TrustClass::Foreign if is_azlin_host(&host) => Err(format!(
            "{host} is an Azlin host: it is reached through the Azlin transport, whose pool \
             trusts the pinned authorities, not through a foreign one"
        )),
        TrustClass::Foreign => Ok(()),
    }
}

#[cfg(test)]
mod class_tests {
    use super::*;

    /// The host as the class check reads it: scheme, user info, port, case and the trailing dot
    /// of a fully qualified name do not hide it.
    #[test]
    fn the_host_of_a_url_is_the_host_a_request_goes_to() {
        for (url, host) in [
            ("https://token.azlin.io", "token.azlin.io"),
            ("https://token.azlin.io/v1/drives?x=1#f", "token.azlin.io"),
            ("HTTPS://Token.Azlin.IO:8443/path", "token.azlin.io"),
            ("https://token.azlin.io./v1", "token.azlin.io"),
            ("https://ann:pw@token.azlin.io/v1", "token.azlin.io"),
            ("https://evil.example@token.azlin.io/", "token.azlin.io"),
            ("https://token.azlin.io@evil.example/", "evil.example"),
            ("http://[::1]:8081/x", "::1"),
            ("http://127.0.0.1:9000", "127.0.0.1"),
        ] {
            assert_eq!(host_of_url(url).as_deref(), Some(host), "{url}");
        }
        for url in [
            "",
            "token.azlin.io",
            "https://",
            "https:///x",
            "https://:80",
        ] {
            assert_eq!(host_of_url(url), None, "{url:?}");
        }
    }

    /// A URL that parsers read differently names no host: WHATWG takes a backslash for a slash
    /// (the first of these goes to `evil.example` in a browser's parser, and a plain split of the
    /// authority says `token.azlin.io`), a control character or a space splits a name, a percent
    /// sign is no part of one, and a port is a number.
    #[test]
    fn a_url_that_parsers_read_differently_names_no_host() {
        for url in [
            "https://evil.example\\@token.azlin.io/",
            "https://token.azlin.io\\@evil.example/",
            "https://token.azlin.io\\.evil.example/",
            "https://token.azlin.io\t.evil.example/",
            "https://evil.example\n@token.azlin.io/",
            "https://evil.example\r@token.azlin.io/",
            "https://evil.example token.azlin.io/",
            "https://evil.example%40token.azlin.io/",
            "https://evil.example%2etoken.azlin.io/",
            "https://token.azlin.io:80x/",
            "https://token.azlin.io:-1/",
        ] {
            assert_eq!(host_of_url(url), None, "{url:?}");
        }
        // The password of the user info may hold what the host may not, and a path or a query
        // what an authority may not: the host is the same in every parser.
        assert_eq!(
            host_of_url("https://ann:p%40ss@cloud.example/dav").as_deref(),
            Some("cloud.example")
        );
        assert_eq!(
            host_of_url("https://cloud.example/dav/My Documents/a\\b?q=a b#x y").as_deref(),
            Some("cloud.example")
        );
        // Spaces around the whole address are not in it (the token server trims its base).
        assert_eq!(
            host_of_url("  https://token.azlin.io/ \n").as_deref(),
            Some("token.azlin.io")
        );
        // Not even a test build lets such a request through.
        for test_build in [false, true] {
            for class in [TrustClass::Azlin, TrustClass::Foreign] {
                let why = admits(class, "https://evil.example\\@token.azlin.io/", test_build)
                    .unwrap_err();
                assert!(why.contains("names no host"), "{why}");
            }
        }
    }

    /// A name is Azlin's when it ends in the suffix on a label boundary - not when it merely
    /// contains it.
    #[test]
    fn an_azlin_host_ends_in_the_suffix_on_a_label_boundary() {
        for host in [
            "azlin.io",
            "token.azlin.io",
            "b1.s3.azlin.io",
            "n1.s3-trial.azlin.io",
        ] {
            assert!(is_azlin_host(host), "{host}");
        }
        for host in [
            "evilazlin.io",
            "azlin.io.evil.example",
            "token.azlin.io.evil.example",
            "azlin.iox",
            "azlin.com",
            "127.0.0.1",
            "localhost",
            "",
        ] {
            assert!(!is_azlin_host(host), "{host}");
        }
    }

    /// An Azlin transport carries the service's own hosts only; a stack on this computer is for
    /// a test build.
    #[test]
    fn an_azlin_transport_goes_to_azlin_hosts_only_and_a_test_build_to_a_stack_too() {
        assert!(admits(TrustClass::Azlin, "https://token.azlin.io/v1", false).is_ok());
        assert!(admits(TrustClass::Azlin, "https://b1.s3.azlin.io/d-1/key", false).is_ok());
        for url in [
            "https://evil.example/d-1/key",
            "https://azlin.io.evil.example/",
            "http://127.0.0.1:9000/d-1/key",
            "http://localhost:8081",
        ] {
            let why = admits(TrustClass::Azlin, url, false).unwrap_err();
            assert!(why.contains("not an Azlin host"), "{url}: {why}");
            assert!(admits(TrustClass::Azlin, url, true).is_ok(), "{url}");
        }
        assert!(admits(TrustClass::Azlin, "no url", false).is_err());
    }

    /// A foreign transport never carries Azlin's traffic - in any build.
    #[test]
    fn a_foreign_transport_refuses_an_azlin_host_and_takes_every_other() {
        for test_build in [false, true] {
            let why =
                admits(TrustClass::Foreign, "https://token.azlin.io/v1", test_build).unwrap_err();
            assert!(why.contains("Azlin host"), "{why}");
            assert!(admits(TrustClass::Foreign, "https://b1.s3.azlin.io/", test_build).is_err());
            for url in [
                "https://api.dropbox.com/2/files",
                "https://cloud.example/dav",
                "http://127.0.0.1:8080/x",
                "https://azlin.io.evil.example/",
            ] {
                assert!(
                    admits(TrustClass::Foreign, url, test_build).is_ok(),
                    "{url}"
                );
            }
        }
    }

    /// The message of a refusal names the host and nothing of the URL's password or query.
    #[test]
    fn a_refusal_names_the_host_and_never_the_secret_of_the_url() {
        let why = admits(
            TrustClass::Azlin,
            "https://ann:hunter2@evil.example/x?token=t0k3n",
            false,
        )
        .unwrap_err();
        assert!(why.contains("evil.example"), "{why}");
        assert!(!why.contains("hunter2") && !why.contains("t0k3n"), "{why}");
    }

    /// The pins are the operator's to name (empty until then; an empty list means "the app's
    /// roots", never "no check at all"). Whatever is named is a few certificates - the current
    /// issuing authorities and the next - and each is PEM text.
    #[test]
    fn whatever_the_operator_pins_is_a_few_certificates() {
        assert!(
            AZLIN_TLS_ROOTS_PEM.len() <= 8,
            "the pins of a rotation, not a bundle"
        );
        for pem in AZLIN_TLS_ROOTS_PEM {
            assert!(
                pem.contains("BEGIN CERTIFICATE"),
                "a pin that is no certificate"
            );
        }
    }

    /// THE RELEASE GATE of the pins (ENT27-M16): ignored in a normal run, because the list is the
    /// operator's to name and is empty until then. The release procedure runs it (`cargo test -p
    /// azul-storage --lib -- --ignored release_`) and does not ship while it fails: a build whose
    /// Azlin pool follows the whole public PKI is not the build the service's trust model asks for.
    #[test]
    #[ignore = "release gate: red until the operator has named the issuing authorities (AZLIN_TLS_ROOTS_PEM)"]
    fn release_the_azlin_authorities_are_pinned() {
        assert!(
            AZLIN_TLS_ROOTS_PEM.len() >= 2,
            "AZLIN_TLS_ROOTS_PEM names {} authorities: the operator names the CURRENT issuing \
             authorities of the token server's and the nodes' certificates and the NEXT, so that a \
             rotation locks no client out",
            AZLIN_TLS_ROOTS_PEM.len()
        );
    }
}

#[cfg(feature = "encryption")]
pub use enc_state::{EncState, IndexHome, Op, RestoreKind, SyncKind, Via, Why};

#[cfg(feature = "encryption")]
mod enc_state {
    use crate::crypto::KeyId;

    /// How this device came to take a drive for an encrypted one.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Via {
        /// The drive key is in this device's keyring. Sticky: a bucket that hides its key files
        /// cannot turn this device back to plaintext.
        KeyInKeyring,
        /// The drives file records that this device found the drive encrypted. The key may be
        /// gone from the keyring since (a reinstall, a wiped keychain): the drive is locked then,
        /// never plain.
        Record,
        /// Only the bucket's listing of `.azlin/keys/` says so - the least trusted word. It makes
        /// a drive encrypted and locked for a device without a key; it never makes one plain.
        KeyFilesListed,
    }

    /// Why a drive that is not encrypted is not.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Why {
        /// A bucket without a file of the drive: a new drive waiting for its encryption.
        New,
        /// A bucket with files and no key anywhere this device can see: a drive made before the
        /// encryption, or a hostile bucket that hides its key files. The two cannot be told
        /// apart from here (no authenticated source says "plain"), so it takes no file.
        HiddenOrAbsent,
    }

    /// What a call asks of a drive.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Op {
        Read,
        Write,
        Delete,
    }

    /// Whether the drive is encrypted, as this device knows it. The value of "could not tell"
    /// is [`EncState::Undecided`]: it is never read as "plain", and the policies below say what
    /// each thing that depends on the answer does meanwhile.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum EncState {
        /// Nothing was asked yet, or the last ask failed (a keyring that could not be read, no
        /// answer from the bucket, a drive locked for want of its key).
        Undecided,
        /// The drive is encrypted. `key_id` is the id of the key it opened with.
        Encrypted { key_id: Option<KeyId>, via: Via },
        /// A setup of this device that stopped halfway (the key is kept, the device is not
        /// enrolled yet): the same setup call finishes it. Not a locked drive.
        SetupPending,
        /// The drive is not encrypted. It reads as its bucket lists and takes no write, ever.
        Unencrypted(Why),
    }

    /// What a sync of a drive does in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum SyncKind {
        /// Through the drive's names (its index).
        Named,
        /// None may start: the drive takes no file (the drive's own refusal says why).
        Refused,
        /// The drive is not opened yet: open it, then ask again.
        NotYet,
    }

    /// What "Restore as of" does in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RestoreKind {
        /// From the drive's metadata repository: the files put back, as one new commit.
        Files,
        /// The drive could not be opened (nothing is decided): say so. NEVER the bucket's
        /// restore - it rolls every object of the bucket back, an encrypted drive's included.
        OpenFirst,
        /// The drive is not encrypted (yet): it has no history to restore from.
        EncryptFirst,
    }

    /// Where a drive's full-text index lives in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum IndexHome {
        /// In the drive's own cache folder beside its drive index (the plain text of an
        /// encrypted drive's files never lies in the shared cache).
        DriveIndex,
        /// In the cache every drive shares.
        SharedCache,
        /// Nowhere yet: the drive is not known to be either.
        NotYet,
    }

    impl EncState {
        /// Whether a call decided it (anything but [`EncState::Undecided`]).
        #[must_use]
        pub const fn is_decided(self) -> bool {
            match self {
                EncState::Undecided => false,
                EncState::Encrypted { .. } | EncState::SetupPending | EncState::Unencrypted(_) => {
                    true
                }
            }
        }

        /// Whether the drive waits for its encryption to be set up by the app (a new drive, or
        /// a stopped setup): it takes no file until then.
        #[must_use]
        pub const fn needs_encryption(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(Why::New) => true,
                EncState::Undecided
                | EncState::Encrypted { .. }
                | EncState::Unencrypted(Why::HiddenOrAbsent) => false,
            }
        }

        /// Whether "Encrypt this drive" is offered: the drive is known not to be encrypted (a new
        /// one, a stopped setup, a plain one made before the encryption). Not for an encrypted
        /// drive - its setup refuses with "encrypted already" - and not before the drive is
        /// decided: it may be an encrypted one whose keys this device cannot see (AZ-K9).
        #[must_use]
        pub const fn offers_encrypt(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(_) => true,
                EncState::Undecided | EncState::Encrypted { .. } => false,
            }
        }

        /// Whether the drive goes through `op`. `plain_reads` is whether this app reads a drive
        /// that is not encrypted as its bucket lists (AzDrive does, to offer "Encrypt this
        /// drive"; the bridge and AzMail do not). Nothing goes through an undecided drive, and
        /// a drive that is not encrypted takes no write and no delete.
        #[must_use]
        pub const fn admits(self, op: Op, plain_reads: bool) -> bool {
            match self {
                EncState::Encrypted { .. } => true,
                EncState::Undecided => false,
                EncState::SetupPending | EncState::Unencrypted(_) => match op {
                    Op::Read => plain_reads,
                    Op::Write | Op::Delete => false,
                },
            }
        }

        /// Whether the drive has a drive index on this computer, and so its names come from it
        /// and its upkeep (recompression, repacking, totals) applies: an encrypted drive.
        #[must_use]
        pub const fn has_drive_index(self) -> bool {
            match self {
                EncState::Encrypted { .. } => true,
                EncState::Undecided | EncState::SetupPending | EncState::Unencrypted(_) => false,
            }
        }

        /// Whether a listing of the drive may be kept on this computer: its names would lie in
        /// the cache in the clear unless the bucket's listing is all there is to them.
        #[must_use]
        pub const fn may_cache_listing(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(_) => true,
                EncState::Undecided | EncState::Encrypted { .. } => false,
            }
        }

        /// Whether the changes of a pass are index commits: an Azlin drive's are, unless it is
        /// known not to be encrypted (what the drive's state is asked to permit).
        #[must_use]
        pub const fn sends_index_commits(self) -> bool {
            match self {
                EncState::Undecided | EncState::Encrypted { .. } => true,
                EncState::SetupPending | EncState::Unencrypted(_) => false,
            }
        }

        /// What a sync of the drive does.
        #[must_use]
        pub const fn sync_kind(self) -> SyncKind {
            match self {
                EncState::Encrypted { .. } => SyncKind::Named,
                EncState::SetupPending | EncState::Unencrypted(_) => SyncKind::Refused,
                EncState::Undecided => SyncKind::NotYet,
            }
        }

        /// What "Restore as of" does. The bucket's restore is not one of the answers: it is
        /// never right for a drive that may be encrypted, and no authenticated source says a
        /// drive is not.
        #[must_use]
        pub const fn restore_kind(self) -> RestoreKind {
            match self {
                EncState::Encrypted { .. } => RestoreKind::Files,
                EncState::Undecided => RestoreKind::OpenFirst,
                EncState::SetupPending | EncState::Unencrypted(_) => RestoreKind::EncryptFirst,
            }
        }

        /// Where the drive's full-text index lives.
        #[must_use]
        pub const fn index_home(self) -> IndexHome {
            match self {
                EncState::Encrypted { .. } => IndexHome::DriveIndex,
                EncState::SetupPending | EncState::Unencrypted(_) => IndexHome::SharedCache,
                EncState::Undecided => IndexHome::NotYet,
            }
        }
    }
}
