//! The client's hello: which app, which version, which contract - sent with every request to
//! the token server and the nodes, so the services can tell an app that is too old to
//! understand them from one that sent something wrong.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{ids::InvalidValue, CONTRACT_VERSION};

/// The header the hello travels in.
pub const CLIENT_HEADER: &str = "x-azlin-client";

/// An app's name in the hello: 1 to 32 lowercase letters, digits and `-` (`azdrive`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AppName(String);

impl AppName {
    /// The app `text` names, in lowercase.
    ///
    /// # Errors
    ///
    /// Empty, longer than 32 bytes, or a character other than a letter, a digit, `-`.
    pub fn parse(text: &str) -> Result<AppName, InvalidValue> {
        let name = text.trim().to_ascii_lowercase();
        let ok = !name.is_empty()
            && name.len() <= 32
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if ok {
            Ok(AppName(name))
        } else {
            Err(InvalidValue::new("app name", text))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AppName {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<AppName, InvalidValue> {
        AppName::parse(&text)
    }
}

impl From<AppName> for String {
    fn from(name: AppName) -> String {
        name.0
    }
}

impl fmt::Display for AppName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `major.minor.patch`, ordered as versions are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    /// `0.3.0`; a pre-release or build suffix (`-beta.1`, `+abc`) is left out.
    ///
    /// # Errors
    ///
    /// Not three dot-separated numbers.
    pub fn parse(text: &str) -> Result<SemVer, InvalidValue> {
        let bad = || InvalidValue::new("version (major.minor.patch)", text);
        let core = text.trim().split(['-', '+']).next().unwrap_or_default();
        let mut parts = core.split('.');
        let mut number = || -> Result<u32, InvalidValue> {
            let part = parts.next().ok_or_else(bad)?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let version = SemVer {
            major: number()?,
            minor: number()?,
            patch: number()?,
        };
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(version)
    }
}

impl TryFrom<String> for SemVer {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<SemVer, InvalidValue> {
        SemVer::parse(&text)
    }
}

impl From<SemVer> for String {
    fn from(version: SemVer) -> String {
        version.to_string()
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a client says it is: `azdrive/0.3.0; contract=3`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClientHello {
    pub app: AppName,
    pub version: SemVer,
    pub contract: u32,
}

impl ClientHello {
    /// `app` at `version`, speaking this crate's [`CONTRACT_VERSION`].
    #[must_use]
    pub fn new(app: AppName, version: SemVer) -> ClientHello {
        ClientHello {
            app,
            version,
            contract: CONTRACT_VERSION,
        }
    }

    /// The header's value.
    #[must_use]
    pub fn header_value(&self) -> String {
        self.to_string()
    }

    /// The hello of a header's value. Parameters after `contract` are skipped, so a later
    /// client may add some.
    ///
    /// # Errors
    ///
    /// No `app/version`, or no `contract=<n>`.
    pub fn parse(text: &str) -> Result<ClientHello, InvalidValue> {
        let bad = || InvalidValue::new("client hello (app/version; contract=n)", text);
        let mut fields = text.split(';').map(str::trim);
        let (app, version) = fields.next().and_then(|f| f.split_once('/')).ok_or_else(bad)?;
        let app = AppName::parse(app).map_err(|_| bad())?;
        let version = SemVer::parse(version).map_err(|_| bad())?;
        let contract = fields
            .filter_map(|f| f.split_once('='))
            .find(|(key, _)| key.trim() == "contract")
            .and_then(|(_, n)| n.trim().parse::<u32>().ok())
            .ok_or_else(bad)?;
        Ok(ClientHello {
            app,
            version,
            contract,
        })
    }
}

impl TryFrom<String> for ClientHello {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<ClientHello, InvalidValue> {
        ClientHello::parse(&text)
    }
}

impl From<ClientHello> for String {
    fn from(hello: ClientHello) -> String {
        hello.to_string()
    }
}

impl fmt::Display for ClientHello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}; contract={}", self.app, self.version, self.contract)
    }
}

/// The oldest contract the services answer: the first release's. It goes up only once every
/// app in use speaks a newer one.
pub const MIN_CONTRACT: u32 = 3;

/// The oldest version of each app the services answer, besides [`MIN_CONTRACT`]: a release with
/// a fault that harms the drive is shut out here. None yet.
pub const MIN_VERSIONS: &[(&str, SemVer)] = &[];

/// What a client too old for a service is told, with `426 client_too_old`: the contract to speak
/// and, when its app has one, the version to reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientTooOld {
    pub min_contract: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<SemVer>,
}

/// The oldest clients a service answers: a contract, and per app a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMinimum {
    pub contract: u32,
    pub versions: Vec<(AppName, SemVer)>,
}

impl ClientMinimum {
    /// [`MIN_CONTRACT`] and [`MIN_VERSIONS`], what the services enforce.
    #[must_use]
    pub fn current() -> ClientMinimum {
        ClientMinimum {
            contract: MIN_CONTRACT,
            versions: MIN_VERSIONS
                .iter()
                .filter_map(|(app, version)| Some((AppName::parse(app).ok()?, *version)))
                .collect(),
        }
    }

    /// Whether a request whose `x-azlin-client` header says `header` is refused, and with what:
    /// a hello that does not read (an app from before the hello's format), one of an older
    /// contract, one below its app's version. No header is no app (a webhook, a page, an
    /// operator's tool) and is answered.
    #[must_use]
    pub fn refuses(&self, header: Option<&str>) -> Option<ClientTooOld> {
        let header = header?;
        let too_old = |min_version| {
            Some(ClientTooOld {
                min_contract: self.contract,
                min_version,
            })
        };
        let Ok(hello) = ClientHello::parse(header) else {
            return too_old(None);
        };
        let floor = self
            .versions
            .iter()
            .find(|(app, _)| *app == hello.app)
            .map(|(_, version)| *version);
        if hello.contract < self.contract || floor.is_some_and(|min| hello.version < min) {
            return too_old(floor);
        }
        None
    }
}

/// [`ClientMinimum::refuses`] of the [`ClientMinimum::current`] minimum.
#[must_use]
pub fn client_too_old(header: Option<&str>) -> Option<ClientTooOld> {
    ClientMinimum::current().refuses(header)
}
