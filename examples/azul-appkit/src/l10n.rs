//! An app's words through azul's localization (doc/guide/en/architecture/localization.md).
//!
//! - **The resources:** [`register`] gives the engine (`AppConfig::fluent_locales`) one Fluent
//!   resource per language - appkit's own words ([`APPKIT_EN`], [`APPKIT_DE`]), then the app's
//!   (its `resources/en.ftl`, `resources/de.ftl`) and anything it adds (azcloud-kit's error
//!   table) - and keeps them on this thread for the text that is no DOM text node.
//! - **DOM text:** [`tr`] marks a key (`AzString::tr`): the layout pass translates it into the
//!   window's language. [`span`] is a key with arguments (`with_fluent_args`), [`text_dom`] a
//!   [`Text`].
//! - **A widget's words** (a button's, a ribbon's, a segmented control's label): [`label`] says
//!   a key here, in the language of the layout pass - widgets measure, wrap and join their
//!   labels (a ribbon's two-line label, "Copy options" of a split button), which would take a
//!   key apart - and passes plain words (an app not localized yet) as they are.
//! - **Text that is no DOM text node** (a window title, a menu entry, an accessible name, a
//!   tooltip, a placeholder): [`t`] / [`t_args`] / [`t_text`] in the language of the layout
//!   pass, which an app's layout callback names first ([`begin_layout`] - asking for it makes
//!   the layout depend on the language, so a switch builds it again); in a callback
//!   [`translate`] / [`translate_text`] (the engine's `CallbackInfo::translate`).
//! - **The language:** the engine's (the system's), or the Language setting / `--language`
//!   (`ui` applies it with `CallbackInfo::set_locale`).

use std::{cell::RefCell, collections::BTreeMap};

use azul::{
    fluent::{FluentArg, FluentArgKV, FluentLocalizerHandle},
    fmt::{FmtArg, FmtValue},
    prelude::*,
    str::{String as AzString, StringPair},
    vec::{DomVec, FluentArgKVVec, FmtArgVec, StringPairVec, StringVec},
};

pub use crate::phrase::{Arg, Part, Phrase, Text};

/// appkit's own words (the settings page, the About box), in English.
pub const APPKIT_EN: &str = include_str!("../resources/en.ftl");
/// appkit's own words in German.
pub const APPKIT_DE: &str = include_str!("../resources/de.ftl");

thread_local! {
    /// The app's resources ([`register`]), for [`t`].
    static LOCALIZER: RefCell<Option<FluentLocalizerHandle>> = const { RefCell::new(None) };
    /// The language of the layout pass ([`begin_layout`]).
    static LOCALE: RefCell<String> = RefCell::new(String::from("en-US"));
}

/// The resources per language: appkit's first, then `resources` in their order, each
/// language's joined into one resource; English first (the engine's fallback).
#[must_use]
pub fn sources(resources: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut by_tag: BTreeMap<String, String> = BTreeMap::new();
    by_tag.insert(String::from("en"), APPKIT_EN.to_string());
    by_tag.insert(String::from("de"), APPKIT_DE.to_string());
    for (tag, source) in resources {
        let joined = by_tag.entry((*tag).to_string()).or_default();
        if !joined.is_empty() && !joined.ends_with('\n') {
            joined.push('\n');
        }
        joined.push_str(source);
    }
    let mut out: Vec<(String, String)> = by_tag.into_iter().collect();
    out.sort_by_key(|(tag, _)| tag.as_str() != "en");
    out
}

/// Gives the engine the app's resources ([`sources`]: appkit's, then `resources`, e.g.
/// `[("en", include_str!("../resources/en.ftl")), ("de", ...)]`) and keeps them on this
/// thread for [`t`]. Called once where the app builds its `AppConfig`.
pub fn register(config: &mut AppConfig, resources: &[(&str, &str)]) {
    let sources = sources(resources);
    config.fluent_locales = StringPairVec::from_vec(
        sources
            .iter()
            .map(|(tag, source)| StringPair::create(tag.as_str(), source.as_str()))
            .collect(),
    );
    keep(&sources);
}

/// Keeps `sources` for [`t`] on this thread (without an `AppConfig`: a test).
pub fn keep(sources: &[(String, String)]) {
    let localizer = FluentLocalizerHandle::default();
    for (tag, source) in sources {
        let _ = localizer.add_resource(tag.as_str(), source.as_str());
    }
    LOCALIZER.with(|l| *l.borrow_mut() = Some(localizer));
}

/// At the start of an app's layout callback: the window's language for [`t`] in this pass
/// (asking for it makes the layout depend on it: a language switch builds it again).
pub fn begin_layout(info: &LayoutCallbackInfo) {
    let locale = info.get_locale();
    set_locale(locale.as_str());
}

/// The language [`t`] translates into (what [`begin_layout`] reads; a test sets it).
pub fn set_locale(locale: &str) {
    LOCALE.with(|l| *l.borrow_mut() = locale.to_string());
}

/// The language [`t`] translates into.
#[must_use]
pub fn locale() -> String {
    LOCALE.with(|l| l.borrow().clone())
}

/// Whether `text` is a key (`kit-general-theme`): lowercase letters, digits and hyphens, at
/// least one hyphen.
#[must_use]
pub fn is_key(text: &str) -> bool {
    text.contains('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A DOM text in the window's language: the key, which the layout pass translates.
#[must_use]
pub fn tr(key: &str) -> AzString {
    AzString::tr(key)
}

/// A widget's label: a key said in the language of the layout pass ([`t`]), plain words (an
/// app not localized yet) as they are.
#[must_use]
pub fn label(text: &str) -> AzString {
    AzString::from(t_label(text))
}

/// The labels as a `StringVec` (a segmented control's choices, a drop-down's), each a
/// [`label`].
#[must_use]
pub fn labels(texts: &[&str]) -> StringVec {
    StringVec::from_vec(texts.iter().map(|text| label(text)).collect())
}

/// A span of the key `key` with `args` (the layout pass translates it with them).
#[must_use]
pub fn span(key: &str, args: &[(&str, Arg)]) -> Dom {
    let span = Dom::create_span_with_text(tr(key));
    if args.is_empty() {
        span
    } else {
        span.with_fluent_args(fluent_args(args))
    }
}

/// A span of `phrase`.
#[must_use]
pub fn phrase_dom(phrase: &Phrase) -> Dom {
    let args: Vec<(&str, Arg)> = phrase
        .args
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    span(&phrase.key, &args)
}

/// `text` as DOM: a span per part (one part: that span).
#[must_use]
pub fn text_dom(text: &Text) -> Dom {
    let mut spans: Vec<Dom> = text
        .parts
        .iter()
        .map(|part| match part {
            Part::Plain(words) => Dom::create_span_with_text(AzString::from(words.as_str())),
            Part::Phrase(phrase) => phrase_dom(phrase),
        })
        .collect();
    if spans.len() == 1 {
        return spans.remove(0);
    }
    Dom::create_span().with_children(DomVec::from_vec(spans))
}

/// `args` as a DOM node's Fluent arguments.
#[must_use]
pub fn fluent_args(args: &[(&str, Arg)]) -> FluentArgKVVec {
    FluentArgKVVec::from_vec(
        args.iter()
            .map(|(key, value)| FluentArgKV {
                key: AzString::from(*key),
                value: match value {
                    Arg::Str(text) => FluentArg::String(AzString::from(text.as_str())),
                    Arg::Int(count) => FluentArg::I32(
                        i32::try_from(*count).unwrap_or(if *count < 0 { i32::MIN } else { i32::MAX }),
                    ),
                },
            })
            .collect(),
    )
}

/// The message `key` in the language of the layout pass, for text that is no DOM text node
/// (a window title, a menu entry, an accessible name, a tooltip); the key without resources.
#[must_use]
pub fn t(key: &str) -> String {
    t_args(key, &[])
}

/// [`t`] with arguments.
#[must_use]
pub fn t_args(key: &str, args: &[(&str, Arg)]) -> String {
    if LOCALIZER.with(|l| l.borrow().is_none()) {
        // An app that registered nothing: appkit's own words at least.
        keep(&sources(&[]));
    }
    let locale = locale();
    LOCALIZER.with(|l| match l.borrow().as_ref() {
        Some(localizer) => localizer
            .translate(locale.as_str(), key, fmt_args(args))
            .as_str()
            .to_string(),
        None => key.to_string(),
    })
}

/// [`t`] of a [`label`]: a key translated, plain words as they are.
#[must_use]
pub fn t_label(text: &str) -> String {
    if is_key(text) {
        t(text)
    } else {
        text.to_string()
    }
}

/// An app's own word for a thing it keys by its English name (a category of its Options,
/// `View`, which is the category's DOM id and `open_settings`' argument too): the message
/// `<app>-<what>-<name>` of the app's resources (`azdrive-category-view`; the app's and the
/// name's letters and digits, lower case, a hyphen for each run of others) in the language of
/// the layout pass; the name as it is when the app has no such message.
#[must_use]
pub fn named(app: &str, what: &str, name: &str) -> String {
    app_word(app, &format!("{what}-{}", slug(name, true)), name)
}

/// An app's own sentence of the kit's pages (the About page's summary): the message
/// `<app>-<what>` of the app's resources (`azdrive-about-summary`) in the language of the
/// layout pass, else `fallback` (the words the app gave the kit).
#[must_use]
pub fn app_word(app: &str, what: &str, fallback: &str) -> String {
    let key = format!("{}-{what}", slug(app, false));
    let said = t(&key);
    if said == key {
        fallback.to_string()
    } else {
        said
    }
}

/// `text`'s letters and digits in lower case; `hyphens`: a hyphen for each run of others.
fn slug(text: &str, hyphens: bool) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if hyphens && !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// `text` in the language of the layout pass.
#[must_use]
pub fn t_text(text: &Text) -> String {
    let mut out = String::new();
    for part in &text.parts {
        match part {
            Part::Plain(words) => out.push_str(words),
            Part::Phrase(phrase) => out.push_str(&t_phrase(phrase)),
        }
    }
    out
}

/// `phrase` in the language of the layout pass.
#[must_use]
pub fn t_phrase(phrase: &Phrase) -> String {
    let args: Vec<(&str, Arg)> = phrase
        .args
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    t_args(&phrase.key, &args)
}

fn fmt_args(args: &[(&str, Arg)]) -> FmtArgVec {
    FmtArgVec::from_vec(
        args.iter()
            .map(|(key, value)| FmtArg {
                key: AzString::from(*key),
                value: match value {
                    Arg::Str(text) => FmtValue::Str(AzString::from(text.as_str())),
                    Arg::Int(count) => FmtValue::Slong(*count),
                },
            })
            .collect(),
    )
}

/// The message `key` with `args` in a callback: the engine's translation into the window's
/// language (a notification's body, a window title set from a callback).
#[must_use]
pub fn translate(info: &CallbackInfo, key: &str, args: &[(&str, Arg)]) -> String {
    info.translate(AzString::from(key), fluent_args(args))
        .as_str()
        .to_string()
}

/// `text` in a callback, in the window's language.
#[must_use]
pub fn translate_text(info: &CallbackInfo, text: &Text) -> String {
    let mut out = String::new();
    for part in &text.parts {
        match part {
            Part::Plain(words) => out.push_str(words),
            Part::Phrase(phrase) => {
                let args: Vec<(&str, Arg)> = phrase
                    .args
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.clone()))
                    .collect();
                out.push_str(&translate(info, &phrase.key, &args));
            }
        }
    }
    out
}
