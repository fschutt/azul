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

use std::{cell::RefCell, collections::BTreeMap, sync::Mutex};

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

/// The resources [`keep`] was given last (on any thread), for a worker's [`Voice::adopt`].
static KEPT: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

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
    if let Ok(mut kept) = KEPT.lock() {
        *kept = sources.to_vec();
    }
}

/// A thread's language, for a worker thread that writes words itself (a printout laid out off
/// the UI thread): [`Voice::here`] on the UI thread, [`Voice::adopt`] on the worker, and the
/// worker's [`t`] says what the UI thread's would. (A worker's answers for the UI stay keys or
/// [`Text`]s, said by the UI thread.) Two voices are equal when they speak one language: a
/// printout made in another language is another printout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    locale: String,
}

impl Voice {
    /// This thread's language ([`locale`]).
    #[must_use]
    pub fn here() -> Self {
        Self { locale: locale() }
    }

    /// This thread says its words as the voice's thread does: the app's resources (the ones
    /// kept last, once per thread) in the voice's language.
    pub fn adopt(&self) {
        if LOCALIZER.with(|l| l.borrow().is_none()) {
            let kept = KEPT.lock().map(|k| k.clone()).unwrap_or_default();
            if !kept.is_empty() {
                keep(&kept);
            }
        }
        set_locale(&self.locale);
    }
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
                    Arg::Word { key, fallback } => {
                        FluentArg::String(AzString::from(word(key, fallback)))
                    }
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
    // The words of the arguments first: a word argument is a message of its own.
    let args = fmt_args(args);
    LOCALIZER.with(|l| match l.borrow().as_ref() {
        Some(localizer) => localizer
            .translate(locale.as_str(), key, args)
            .as_str()
            .to_string(),
        None => key.to_string(),
    })
}

/// The message `key` in the language of the layout pass, `fallback` when the resources have
/// none (an [`Arg::Word`], a kit word outside a window).
fn word(key: &str, fallback: &str) -> String {
    let said = t(key);
    if said == key {
        fallback.to_string()
    } else {
        said
    }
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
    word(&named_key(app, what, name), name)
}

/// The key [`named`] looks `name` up by: `<app>-<what>-<name>` (`azdrive-category-view`) - for
/// a worker thread's [`Arg::word`], which has no resources.
#[must_use]
pub fn named_key(app: &str, what: &str, name: &str) -> String {
    format!("{}-{what}-{}", slug(app, false), slug(name, true))
}

/// An app's own sentence of the kit's pages (the About page's summary): the message
/// `<app>-<what>` of the app's resources (`azdrive-about-summary`) in the language of the
/// layout pass, else `fallback` (the words the app gave the kit).
#[must_use]
pub fn app_word(app: &str, what: &str, fallback: &str) -> String {
    word(&format!("{}-{what}", slug(app, false)), fallback)
}

/// `n` with its digits grouped by threes as the language of the layout pass groups them
/// (`kit-number-group-separator`: 1,234,567 in English, 1.234.567 in German). Fluent formats a
/// number without grouping, so a count shown big is handed to a message as this text (and as
/// the number, for the plural form).
#[must_use]
pub fn grouped(n: u64) -> String {
    let separator = word("kit-number-group-separator", ",");
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * separator.len());
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(&separator);
        }
        out.push(c);
    }
    out
}

/// A number written with a decimal point (`1.5 TB`, `7.5`) with the decimal mark of the
/// language of the layout pass (`kit-number-decimal-separator`: 1.5 in English, 1,5 in German):
/// a point between two digits. For a text without grouped digits.
#[must_use]
pub fn decimal(text: &str) -> String {
    let mark = word("kit-number-decimal-separator", ".");
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let digit = |at: Option<usize>| {
            at.and_then(|at| chars.get(at))
                .is_some_and(char::is_ascii_digit)
        };
        if c == '.' && digit(i.checked_sub(1)) && digit(Some(i + 1)) {
            out.push_str(&mark);
        } else {
            out.push(c);
        }
    }
    out
}

/// `cents` of `currency` (`EUR`) as the language of the layout pass writes money (`kit-money`,
/// its digits [`grouped`], its cents after the decimal mark): `EUR 1,234.50` in English,
/// `1.234,50 EUR` in German.
#[must_use]
pub fn money(cents: u64, currency: &str) -> String {
    let amount = format!(
        "{}{}{:02}",
        grouped(cents / 100),
        word("kit-number-decimal-separator", "."),
        cents % 100
    );
    let said = t_args(
        "kit-money",
        &[
            ("amount", Arg::from(amount.as_str())),
            ("currency", Arg::from(currency)),
        ],
    );
    if said == "kit-money" {
        format!("{currency} {amount}")
    } else {
        said
    }
}

/// How a date is written ([`date_text`]): the styles the apps' views, lists and headers use, a
/// message of the kit's each (`kit-date-style-*`), the names the kit's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateStyle {
    /// Wednesday, 30 September 2026.
    DayLong,
    /// September 2026.
    MonthYear,
    /// 30 September 2026.
    Date,
    /// 30 September.
    DayMonth,
    /// Wednesday 30 September.
    WeekdayDayMonth,
    /// Wednesday.
    Weekday,
    /// Wed 30.
    ShortWeekdayDay,
    /// Wed 30 Sep.
    ShortDate,
    /// 30 Sep.
    DayShortMonth,
    /// 30 (a range's first day in its month: 28 - 30 September).
    DayOnly,
}

impl DateStyle {
    fn key(self) -> &'static str {
        match self {
            DateStyle::DayLong => "kit-date-style-day-long",
            DateStyle::MonthYear => "kit-date-style-month-year",
            DateStyle::Date => "kit-date-style-date",
            DateStyle::DayMonth => "kit-date-style-day-month",
            DateStyle::WeekdayDayMonth => "kit-date-style-weekday-day-month",
            DateStyle::Weekday => "kit-date-style-weekday",
            DateStyle::ShortWeekdayDay => "kit-date-style-short-weekday-day",
            DateStyle::ShortDate => "kit-date-style-short-date",
            DateStyle::DayShortMonth => "kit-date-style-day-short-month",
            DateStyle::DayOnly => "kit-date-style-day-only",
        }
    }
}

/// The weekdays' and months' words of the kit's resources (azul-pim names the same ids:
/// `dates::weekday_message_id`, `month_message_id`, `month_short_message_id`).
const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The date `year`-`month`-`day` (`weekday` from Monday: 0 to 6) in `style`, as the language of
/// the layout pass writes it: `Wednesday, 30 September 2026`, `Mittwoch, 30. September 2026`.
#[must_use]
pub fn date_text(style: DateStyle, year: i32, month: u32, day: u32, weekday: u32) -> String {
    let weekday = WEEKDAYS[(weekday % 7) as usize];
    let month_name = MONTHS[(month.clamp(1, 12) - 1) as usize];
    let short_weekday = t(&format!("kit-weekday-short-{}", &weekday[..3]));
    let short_month = t(&format!("kit-month-short-{}", &month_name[..3]));
    t_args(
        style.key(),
        &[
            ("weekday", Arg::from(t(&format!("kit-weekday-{weekday}")))),
            ("wd", Arg::from(short_weekday)),
            ("day", Arg::from(day)),
            ("month", Arg::from(t(&format!("kit-month-{month_name}")))),
            ("mon", Arg::from(short_month)),
            ("year", Arg::from(year)),
        ],
    )
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
                    Arg::Word { key, fallback } => {
                        FmtValue::Str(AzString::from(word(key, fallback)))
                    }
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
