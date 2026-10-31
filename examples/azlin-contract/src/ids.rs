//! Ids and units on the wire. Each is checked when it is made or read, so a value that cannot
//! be one never reaches a URL, a database or a sum.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A value that is not what it should be: what it should be, and the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue {
    pub what: &'static str,
    pub value: String,
}

impl InvalidValue {
    pub(crate) fn new(what: &'static str, value: impl Into<String>) -> InvalidValue {
        InvalidValue {
            what,
            value: value.into(),
        }
    }
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a valid {}", self.value, self.what)
    }
}

impl std::error::Error for InvalidValue {}

/// An id as it may stand in a URL path: 1 to 128 letters, digits, `_` and `-` (the token
/// server's `d_` / `ck_` prefixes and base32).
fn is_path_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

macro_rules! path_id {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            #[doc = concat!("The ", $what, " `text` names (surrounding blanks dropped), when it can be one.")]
            ///
            /// # Errors
            ///
            /// Empty, longer than 128 bytes, or a byte other than a letter, a digit, `_`, `-`.
            pub fn parse(text: &str) -> Result<$name, InvalidValue> {
                let text = text.trim();
                if is_path_id(text) {
                    Ok($name(text.to_string()))
                } else {
                    Err(InvalidValue::new($what, text))
                }
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidValue;
            fn try_from(text: String) -> Result<$name, InvalidValue> {
                $name::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

path_id!(
    /// A drive (`d_...`).
    DriveId,
    "drive id"
);
path_id!(
    /// A checkout (`ck_...`).
    CheckoutId,
    "checkout id"
);
path_id!(
    /// A drive token's family: one device's line of rotating drive tokens.
    FamilyId,
    "token family id"
);
path_id!(
    /// A member of a drive: one device, or the recovery that waits for its notice.
    MemberId,
    "member id"
);

/// A bucket's name as S3 takes it in a host or a path: 3 to 63 lowercase letters, digits, `-`
/// and `.`, starting and ending with a letter or a digit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BucketName(String);

impl BucketName {
    /// The bucket `text` names, when it can be one.
    ///
    /// # Errors
    ///
    /// A name S3 does not take.
    pub fn parse(text: &str) -> Result<BucketName, InvalidValue> {
        let text = text.trim();
        let bytes = text.as_bytes();
        let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        let ok = (3..=63).contains(&bytes.len())
            && bytes.iter().all(|&b| edge(b) || b == b'-' || b == b'.')
            && edge(bytes[0])
            && edge(bytes[bytes.len() - 1])
            && !text.contains("..");
        if ok {
            Ok(BucketName(text.to_string()))
        } else {
            Err(InvalidValue::new("bucket name", text))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BucketName {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<BucketName, InvalidValue> {
        BucketName::parse(&text)
    }
}

impl From<BucketName> for String {
    fn from(name: BucketName) -> String {
        name.0
    }
}

impl fmt::Display for BucketName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A storage tier, named by its size (`100GB`, `1TB`). People type it in any case and with
/// blanks (` 1 tb `); the id is always the size in capitals without them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TierId(String);

impl TierId {
    /// The tier `text` names, blanks dropped and in capitals.
    ///
    /// # Errors
    ///
    /// Nothing left, more than 32 characters, or a character other than a letter or a digit.
    pub fn parse(text: &str) -> Result<TierId, InvalidValue> {
        let id: String = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_uppercase();
        if !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            Ok(TierId(id))
        } else {
            Err(InvalidValue::new("tier", text))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TierId {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<TierId, InvalidValue> {
        TierId::parse(&text)
    }
}

impl From<TierId> for String {
    fn from(id: TierId) -> String {
        id.0
    }
}

impl fmt::Display for TierId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How many months a checkout pays for: 1, 3, 6, 12 or 24.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Months(u8);

impl Months {
    /// Every period a checkout may pay for, shortest first.
    pub const ALL: [Months; 5] = [Months(1), Months(3), Months(6), Months(12), Months(24)];

    #[must_use]
    pub fn get(self) -> u32 {
        u32::from(self.0)
    }
}

impl TryFrom<u32> for Months {
    type Error = InvalidValue;
    fn try_from(months: u32) -> Result<Months, InvalidValue> {
        Months::ALL
            .into_iter()
            .find(|m| m.get() == months)
            .ok_or_else(|| InvalidValue::new("number of months (1, 3, 6, 12, 24)", months.to_string()))
    }
}

impl From<Months> for u32 {
    fn from(months: Months) -> u32 {
        months.get()
    }
}

impl fmt::Display for Months {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Bytes as the nodes keep them, after compression and encryption: what a quota counts.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct StoredBytes(pub u64);

/// Bytes as the user's files have them, before compression and encryption: what a folder's
/// size shows.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct LogicalBytes(pub u64);

/// A currency's ISO 4217 code: three capital letters (`EUR`, `PLN`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);

impl Currency {
    /// The currency `text` names, in any case.
    ///
    /// # Errors
    ///
    /// Not three letters.
    pub fn parse(text: &str) -> Result<Currency, InvalidValue> {
        let code = text.trim().to_ascii_uppercase();
        if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
            Ok(Currency(code))
        } else {
            Err(InvalidValue::new("currency", text))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Currency {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<Currency, InvalidValue> {
        Currency::parse(&text)
    }
}

impl From<Currency> for String {
    fn from(currency: Currency) -> String {
        currency.0
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An amount in a currency's smallest unit (cents for `EUR`), VAT included where a price has
/// it. The currency travels with the amount, so two amounts in different currencies are never
/// added by accident.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cents {
    pub amount: u64,
    pub currency: Currency,
}

impl Cents {
    /// The sum of two amounts in one currency; `None` when the currencies differ or it
    /// overflows.
    #[must_use]
    pub fn checked_add(&self, other: &Cents) -> Option<Cents> {
        if self.currency != other.currency {
            return None;
        }
        Some(Cents {
            amount: self.amount.checked_add(other.amount)?,
            currency: self.currency.clone(),
        })
    }
}
