---
slug: events/formatting
title: Locale Formatting
language: en
canonical_slug: events/formatting
audience: external
maturity: wip
guide_order: 70
topic_only: false
short_desc: Locale-aware numbers, dates, lists, plurals and collation from inside a callback
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - layout/src/callbacks.rs
  - core/src/icu.rs
default-search-keys:
  - CallbackInfo
  - IcuResult
  - PluralCategory
  - FormatLength
  - format_integer
  - format_date
  - pluralize
  - sort_strings
---

# Locale Formatting

Numbers, dates, lists and plurals differ per locale in ways no format
string survives. These methods do it properly, from inside a callback,
without pulling in an i18n crate of your own.

## Introduction

Every method takes the locale as its first argument - a BCP-47 tag such
as `"en-US"` or `"de-AT"` - rather than reading a global. That is
deliberate: an app that shows one user's document to another, or
formats a report for a locale that is not the UI's, needs both at once.

The date and time functions return `IcuResult` because a locale can be
unknown or a date invalid; the number functions return a `String`
directly.

## Numbers

```rust,ignore
let n = info.format_integer("de-DE".into(), 1_234_567);      // "1.234.567"
let x = info.format_decimal("de-DE".into(), 1234, 2);        // "12,34"
```

`format_decimal()` takes the value as an integer plus a decimal-place
count rather than an `f64`, which keeps money exact: 12.34 is
`(1234, 2)` and never becomes 12.339999999999999.

## Dates and times

```rust,ignore
let d = info.format_date("fr-FR".into(), date, FormatLength::Long);
let t = info.format_time("fr-FR".into(), time, /* include_seconds */ false);
let dt = info.format_datetime("fr-FR".into(), datetime, FormatLength::Short);
```

`FormatLength` picks how much the locale spells out, from numeric-short
to fully written. Let it decide field order - month-day versus day-month
is a locale property, not a formatting choice.

`get_current_time()` returns the current `Instant` through the same
clock the framework uses, which is the one to read if you want a
timestamp that agrees with the animation and timer subsystems.

## Plurals

English has two plural forms. Arabic has six, Polish three with
different rules, and "if n == 1" is wrong in most of the world:

```rust,ignore
let s = info.pluralize(
    locale, count,
    "no files".into(), "one file".into(), "two files".into(),
    "{} files".into(), "{} files".into(), "{} files".into(),
);
```

You supply every CLDR category - zero, one, two, few, many, other - and
the locale selects. For English only `one` and `other` are ever chosen,
so filling the rest with the same string costs nothing and makes the
call correct in Polish too. `get_plural_category()` returns the chosen
category if you would rather branch yourself.

## Lists

```rust,ignore
let s = info.format_list(locale, items, ListType::And);
```

"A, B and C" in English, "A, B et C" in French, and the serial comma
handled per locale. `ListType` also covers *or* and unit lists.

## Sorting and comparing

Alphabetical order is not code-point order: `ä` sorts with `a` in
German and after `z` in Swedish.

```rust,ignore
let sorted = info.sort_strings(locale, names);
let ord = info.compare_strings(locale, a, b);   // -1, 0, 1
let same = info.strings_equal(locale, a, b);
```

`strings_equal()` is locale-aware equality, not byte equality, so it
matches strings that differ only in how they are composed - which is
the difference between a search box that finds `café` typed two ways
and one that does not.

## More methods

**Numbers** - `format_integer`, `format_decimal`.

**Dates and times** - `format_date`, `format_time`, `format_datetime`,
`get_current_time`.

**Plurals and lists** - `pluralize`, `get_plural_category`,
`format_list`.

**Collation** - `sort_strings`, `compare_strings`, `strings_equal`.

**Other** - `get_icu_localizer` returns the underlying localizer handle
for code that wants to hold one rather than pass a locale each time;
`get_string_contents(node_id)` reads a node's localized string content.
