//! Words of an app's Fluent resources, kept as what they are - a key and its arguments - until
//! they are shown: the window says them in its language (azul's localization, `l10n`). Plain
//! Rust: a model that words a notice, a header line or a row returns a [`Phrase`], and its tests
//! check the key and the arguments, not a language.
//!
//! A [`Text`] is several in a row, with plain text between them (a file's name, a server's own
//! sentence): `"3 files failed: " + <the error's message>`.

/// A value of a message's argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Str(String),
    /// A count: Fluent picks the plural form by it (`{ $count -> [one] ... *[other] ... }`).
    Int(i64),
}

impl From<&str> for Arg {
    fn from(text: &str) -> Arg {
        Arg::Str(text.to_string())
    }
}

impl From<String> for Arg {
    fn from(text: String) -> Arg {
        Arg::Str(text)
    }
}

impl From<&String> for Arg {
    fn from(text: &String) -> Arg {
        Arg::Str(text.clone())
    }
}

impl From<i32> for Arg {
    fn from(count: i32) -> Arg {
        Arg::Int(i64::from(count))
    }
}

impl From<i64> for Arg {
    fn from(count: i64) -> Arg {
        Arg::Int(count)
    }
}

impl From<u32> for Arg {
    fn from(count: u32) -> Arg {
        Arg::Int(i64::from(count))
    }
}

impl From<usize> for Arg {
    fn from(count: usize) -> Arg {
        Arg::Int(i64::try_from(count).unwrap_or(i64::MAX))
    }
}

impl From<u64> for Arg {
    fn from(count: u64) -> Arg {
        Arg::Int(i64::try_from(count).unwrap_or(i64::MAX))
    }
}

impl std::fmt::Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arg::Str(text) => f.write_str(text),
            Arg::Int(count) => write!(f, "{count}"),
        }
    }
}

/// A message of the app's resources and its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phrase {
    pub key: String,
    pub args: Vec<(String, Arg)>,
}

impl Phrase {
    /// The message `key` without arguments.
    #[must_use]
    pub fn new(key: &str) -> Phrase {
        Phrase {
            key: key.to_string(),
            args: Vec::new(),
        }
    }

    /// With the argument `name`.
    #[must_use]
    pub fn arg(mut self, name: &str, value: impl Into<Arg>) -> Phrase {
        self.args.push((name.to_string(), value.into()));
        self
    }

    /// The value of the argument `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arg> {
        self.args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

/// One part of a [`Text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text as it is (a name, a server's own words).
    Plain(String),
    Phrase(Phrase),
}

/// Words to show: phrases of the app's resources and plain text, in a row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Text {
    pub parts: Vec<Part>,
}

impl Text {
    /// Text as it is.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Text {
        Text {
            parts: vec![Part::Plain(text.into())],
        }
    }

    /// The message `key` without arguments.
    #[must_use]
    pub fn key(key: &str) -> Text {
        Text::from(Phrase::new(key))
    }

    /// `other` after this one.
    #[must_use]
    pub fn then(mut self, other: impl Into<Text>) -> Text {
        self.parts.extend(other.into().parts);
        self
    }

    /// Nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|part| match part {
            Part::Plain(text) => text.is_empty(),
            Part::Phrase(_) => false,
        })
    }

    /// The keys of its phrases, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::Phrase(phrase) => Some(phrase.key.as_str()),
                Part::Plain(_) => None,
            })
            .collect()
    }

    /// The phrase `key` of it.
    #[must_use]
    pub fn phrase(&self, key: &str) -> Option<&Phrase> {
        self.parts.iter().find_map(|part| match part {
            Part::Phrase(phrase) if phrase.key == key => Some(phrase),
            _ => None,
        })
    }
}

impl From<Phrase> for Text {
    fn from(phrase: Phrase) -> Text {
        Text {
            parts: vec![Part::Phrase(phrase)],
        }
    }
}

/// Plain words (a name, a server's own sentence) as they are.
impl From<String> for Text {
    fn from(text: String) -> Text {
        Text::plain(text)
    }
}

/// Plain words as they are.
impl From<&str> for Text {
    fn from(text: &str) -> Text {
        Text::plain(text)
    }
}

/// A log line's or a terminal's rendering: plain text as it is, a phrase as its key and
/// arguments (`kit-save-failed(detail=disk full)`) - never shown in a window.
impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.parts {
            match part {
                Part::Plain(text) => f.write_str(text)?,
                Part::Phrase(phrase) => {
                    f.write_str(&phrase.key)?;
                    if !phrase.args.is_empty() {
                        let args: Vec<String> = phrase
                            .args
                            .iter()
                            .map(|(name, value)| format!("{name}={value}"))
                            .collect();
                        write!(f, "({})", args.join(", "))?;
                    }
                }
            }
        }
        Ok(())
    }
}
