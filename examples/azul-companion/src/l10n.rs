//! The phone's words, English and German: `resources/en.ftl` and `resources/de.ftl` (Fluent's `key = value` lines,
//! `{ $name }` for what a screen fills in, one line per key). They are read once into a [`Strings`] the app owns and
//! hands to the screens; no global holds them. A key a screen asks for and a language lacks shows as `[key]` (a test
//! of the sources makes sure that never ships).

use std::collections::BTreeMap;

use crate::env::Lang;

/// The English words.
pub const EN: &str = include_str!("../resources/en.ftl");
/// The German words.
pub const DE: &str = include_str!("../resources/de.ftl");

/// A resource that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtlError {
    pub line: usize,
    pub message: String,
}

impl core::fmt::Display for FtlError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// The keys and words of one resource.
///
/// # Errors
/// The first line that is no `key = value`, a key twice, an empty value.
pub fn parse_ftl(source: &str) -> Result<BTreeMap<String, String>, FtlError> {
    let mut words = BTreeMap::new();
    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        let fail = |message: &str| FtlError { line, message: message.to_string() };
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        if raw.starts_with(char::is_whitespace) {
            return Err(fail("a value goes on in the next line: one line for each key"));
        }
        let Some((key, value)) = raw.split_once('=') else {
            return Err(fail("this is no key = value line"));
        };
        let (key, value) = (key.trim(), value.trim());
        let good_key = key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !good_key {
            return Err(fail("a key is lower-case letters, digits and dashes, and starts with a letter"));
        }
        if value.is_empty() {
            return Err(fail("the key has no words"));
        }
        if words.insert(key.to_string(), value.to_string()).is_some() {
            return Err(FtlError { line, message: format!("the key {key} is there twice") });
        }
    }
    Ok(words)
}

/// Walks a template: for each `{ $name }` calls `on_name(name, start, end)` with the byte range of the placeholder.
fn placeholder_spans(template: &str) -> Vec<(String, usize, usize)> {
    let bytes = template.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'$' {
            let name_start = j + 1;
            let mut k = name_start;
            while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_' || bytes[k] == b'-') {
                k += 1;
            }
            let name_end = k;
            while k < bytes.len() && bytes[k] == b' ' {
                k += 1;
            }
            if name_end > name_start && k < bytes.len() && bytes[k] == b'}' {
                spans.push((template[name_start..name_end].to_string(), i, k + 1));
                i = k + 1;
                continue;
            }
        }
        i += 1;
    }
    spans
}

/// `template` with each `{ $name }` replaced by the value `args` gives for `name` (a name with no value is shown as
/// `?name`).
#[must_use]
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut from = 0;
    for (name, start, end) in placeholder_spans(template) {
        out.push_str(&template[from..start]);
        match args.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('?');
                out.push_str(&name);
            }
        }
        from = end;
    }
    out.push_str(&template[from..]);
    out
}

/// The names a template asks for (`{ $name }`), in order.
#[must_use]
pub fn placeholders(template: &str) -> Vec<String> {
    placeholder_spans(template).into_iter().map(|(name, _, _)| name).collect()
}

/// The words of both languages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strings {
    en: BTreeMap<String, String>,
    de: BTreeMap<String, String>,
}

impl Strings {
    /// The words the app ships with.
    #[must_use]
    pub fn builtin() -> Strings {
        // the resources are checked by a test: a broken one is a bug found there, so a broken one here reads as empty
        Strings::from_sources(EN, DE).unwrap_or_else(|_| Strings { en: BTreeMap::new(), de: BTreeMap::new() })
    }

    /// The words of these two resources.
    ///
    /// # Errors
    /// A resource that cannot be read.
    pub fn from_sources(en: &str, de: &str) -> Result<Strings, FtlError> {
        Ok(Strings { en: parse_ftl(en)?, de: parse_ftl(de)? })
    }

    fn table(&self, lang: Lang) -> &BTreeMap<String, String> {
        match lang {
            Lang::En => &self.en,
            Lang::De => &self.de,
        }
    }

    /// The words for `key`.
    #[must_use]
    pub fn t(&self, lang: Lang, key: &str) -> String {
        self.table(lang).get(key).cloned().unwrap_or_else(|| format!("[{key}]"))
    }

    /// The words for `key` with its `{ $name }`s filled in.
    #[must_use]
    pub fn tf(&self, lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
        fill(&self.t(lang, key), args)
    }

    /// Every key of a language, in order.
    #[must_use]
    pub fn keys(&self, lang: Lang) -> Vec<&str> {
        self.table(lang).keys().map(String::as_str).collect()
    }

    /// The raw template of a key (`None`: no such key).
    #[must_use]
    pub fn template(&self, lang: Lang, key: &str) -> Option<&str> {
        self.table(lang).get(key).map(String::as_str)
    }
}

#[cfg(test)]
#[path = "l10n_test.rs"]
mod l10n_test;
