---
slug: architecture/localization
title: Localization
language: en
canonical_slug: architecture/localization
audience: external
maturity: mature
guide_order: 43
topic_only: false
short_desc: Translating your application into multiple languages using Fluent
prerequisites: [dom, architecture]
default-search-keys:
  - Fluent
  - Localization
  - Localizer
  - Translation
  - AzString
---

# Localization

Azul has native integration with Mozilla's [Fluent](https://projectfluent.org/) localization. 
Azul traverses the DOM and applies translations automatically based on the active locale 
before the UI is laid out and rendered. 

When your translation string requires parameters (like `"Welcome back, { $userName }"`), 
you can attached the argument values in the DOM, so that they are localized according to 
the correct pluralization rules (one `apple`, many `apples`).

## Translatable Strings

To mark strings as translatable, you need to use the `String::tr` function, which sets a
special "flag" marker on the string internally to mark it as a "translation" string instead
of a regular one.

Additionally, the `Dom` node has `set_fluent_args` and `with_fluent_args` to attach `I32`, 
`F32` or `String` arguments to your localization, i.e.:

```rust
let text = Dom::create_p_with_text(String::tr("welcome-greeting"))
.with_fluent_args(vec![
    FluentArgKV {
        key: "userName".into(),
        value: FluentArg::String("Alice".into()),
    }
]);
```

## Startup

Assuming you have `.ftl` files like this:

```fluent
# resources/en.ftl
welcome-message = Welcome back, { $userName }!
button-save = Save Changes
```

```fluent
# resources/de.ftl
welcome-message = Willkommen zurück, { $userName }!
button-save = Änderungen speichern
```

You then add these resources to your `AppConfig` before creating the app:

```rust
let mut app_config = AppConfig::default();

let mut locales = StringPairVec::new();
locales.push(StringPair::new(
    "en".into(),
    include_str!("resources/en.ftl").into()
));
locales.push(StringPair::new(
    "de".into(),
    include_str!("resources/de.ftl").into()
));

app_config.fluent_locales = locales;
```

### Localizing XML

If you are using XML parsing, you can mark strings for translation using the `data-l10n` attribute. 
Any node with `data-l10n` will have its text content resolved as a localization key. You can also 
pass formatting arguments directly via `data-l10n-<arg>` attributes:

```xml
<!-- This will lookup "hello_key" and pass "Alice" as the userName argument -->
<!-- Result: <p>Welcome back, Alice!</p> -->
<p data-l10n="welcome-message" data-l10n-userName="Alice"></p>
```

## Fallback Languages

The localization system supports fallback chains. If a translation 
key is missing in the current locale (e.g. `fr-CA`), Azul will automatically 
search the fallback locale (e.g. `fr`), and finally the global default 
fallback behavior (which can be configured to either render the raw translation 
key, or an empty string) if the key is missing entirely.

## Changing Locale

You can change the active locale dynamically during runtime via `CallbackInfo::set_locale` in any 
event callback. Under normal circumstances this does not cause a full refresh, only the strings are
re-localized. However, the `LayoutCallbackInfo::is_rtl` functions can be used to mark a dependence
of the `Dom` structure on the RTL-ness, for example to adjust for different layouts.

```rust
fn on_click(data: RefAny, info: CallbackInfo) -> Update {
    info.set_locale("de".into()); // triggers re-localization, but not relayout
    Update::DoNothing // set to RefreshDom to force re-layout
}
```

## Translation Completeness

In large applications, it is easy to forget to translate a string. Azul provides a 
built-in checking utility to ensure all keys used in your code have translations 
in all loaded locales.

You can request a `TranslationCompletenessReport` from the `AppConfig` after 
initialization, or check it during your test suite.

```rust
let report = app_config.check_translations();
if !report.missing_keys.is_empty() {
    println!("Warning: Missing translations for keys: {:?}", report.missing_keys);
}
```

You can then create a test to assert the complete translation of all strings
in your CI pipeline.
