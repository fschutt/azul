# Localization review: findings and fixes (2026-09-28)

Branch `wt/localization-review`, based on `90d1864ce` (PR #476 branch
`fix/input-bugs-2026-09-19`, which carries master's localization work).

Reviewed: `60a211ee3` `d1ea2640e` `dd1138624` `d4284f9ac` `be48fcdce`
`e30647040` `80637842e` `23b2a375b`, plus the `flags: u8` byte in `impl_vec!` and
`AzString::tr` / `is_localizable` / `set_localizable`.

**Nothing was compiled or run.** Following the task rules, there was no cargo,
rustc or rust-analyzer. Every "expected failure" below comes from reading the
code. The parent compiles once at the end. See "Least sure to compile".

## TL;DR

The documented feature did not work anywhere in the product:

- Nothing ever gave a window a Fluent localizer, so every `tr()` key rendered as the raw key.
- `set_locale` was a no-op for the text.
- Arguments attached the way the guide shows never reached the text.
- XML `data-l10n` replaced the element with a text node.
- The branch did not build on Windows, Linux or the web backend.
- 9 language bindings lost every Vec helper because of the new `flags` byte.

All of these are fixed, each with a RED test first. The guide's design is kept:
AppConfig `.ftl` sources, `String::tr`, args on the element, in-place
re-localization on `set_locale`, `is_rtl`/`get_locale` as dependency markers,
`fr-CA → fr` fallback and `app_config.check_translations()`.

## Findings by severity

Line numbers refer to the base commit `90d1864ce`.

### Critical: the feature is dead or broken in the product

| # | Finding | Where | Fix |
|---|---|---|---|
| C1 | **No window ever translates.** `LayoutWindow::set_fluent_localizer` has no caller. `AppConfig` has no `fluent_locales` field, although the guide's "Startup" section uses one. `style_user_dom_for` translates only `if let Some(localizer)`, so every `tr()` key renders raw, in every window, on every platform. | `layout/src/window.rs:15600`, `core/src/resources.rs` (AppConfig) | `12a8f6162` |
| C2 | **`set_locale` does nothing to the text.** Several defects stack up. (a) `SetLocale` only calls `set_icu_locale`. (b) Translation reads `system_style.language`, never the chosen locale. (c) Translation *replaces* the key, so the retained DOM has nothing left to re-translate. (d) `set_system_style` runs on every layout pass and resets the ICU locale to the system language. (e) `get_locale()`/`is_rtl()` in `layout()` report the system language. (f) `depends_on_locale` / `depends_on_text_direction` were recorded but never consulted for `set_locale`. | `dll/.../common/event.rs:7536`, `layout/src/window.rs:15591,15644`, `dll/.../common/layout.rs:463` | `7a3bc91c3` |
| C3 | **Arguments never reach the text in the guide's own example.** `create_p_with_text(tr(k)).with_fluent_args(..)` puts the args on the `<p>` and the key in its text child. `translate_node` only read the text node's own (empty) args, so the output was `"Welcome back, {$userName}!"`. | `layout/src/fluent.rs:1004` | `27154b9f0` |
| C4 | **XML `data-l10n` destroys the element.** All three builders did `set_node_type(Text(key))` on the element itself. A `<p>` loses its UA style, selectors and a11y role, and a `<button data-l10n>` stops being a button. The streaming parser then opened that text node as an element and hung the markup's children under a text node. | `core/src/xml.rs:6297`, `layout/src/xml/mod.rs:571` | `c15cba982` |
| C5 | **Stale translations after a rebuild.** `regenerate_layout` fingerprints the *untranslated* DOM and skips cascade, layout and translation when the fingerprint is unchanged. `fingerprint_dom` hashed neither `fluent_args` nor the localizable flag (`AzString` hashes `as_str()` only), so "1 new email" stayed on screen after the count changed. | `core/src/diff.rs:2458`, `dll/.../common/layout.rs:577` | `7d8e914ff` |
| C6 | **A cloned key is no longer a key.** `impl_vec_clone!`'s `clone_self` rebuilt heap and arena buffers with `from_vec`, which sets `flags: 0`. Cloning any `tr()` string therefore dropped the flag, and a cloned DOM (XML kept in the model, component templates) rendered raw keys. | `css/src/macros.rs:929` | `2f1308107` |
| C7 | **The branch does not build on Windows, Linux or web.** (a) `80637842e` added a `known_languages` parameter to `discover_system_style` / `discover`, but Windows `WM_SETTINGCHANGE` still calls `discover_system_style()` with no argument. (b) Linux `rediscovered_style_for` / `dump_discovered_style` still call `discover()` with no argument. (c) `60a211ee3` added four fields to `LayoutCallbackInfoRefData` that the web backend's struct literal lacks. | `dll/.../windows/mod.rs:6849`, `dll/.../linux/system_style.rs:3273,3292`, `dll/src/web/html_render.rs:670` | `17d270a00`, `a2e26af0b` |
| C8 | **9 bindings lost every Vec helper.** The `flags` byte makes a Vec five fields. C#, Java, Kotlin, Node, Ruby, Haskell, Crystal, D and Swift still recognise a Vec by `fields.len() != 4`, so their iterator, array and `len` helpers vanished from the generated code. C++, Lua and conformance were switched to `== 5`, and the IR builder to `4 \| 5`. `bug_classes::vec_category_is_exactly_the_vec_layout` has been red since the byte landed. | `doc/src/codegen/v2/lang_*` (14 sites) | `a5b535574` |

### High

| # | Finding | Where | Fix |
|---|---|---|---|
| H1 | **RTL-ness is lost.** (a) The platforms resolved the OS locale by *exact* id against `known_languages`, so `ar-DZ`, a bare `he`, `ar_EG@calendar=…` and `de_DE.UTF-8` all came out LTR. (b) macOS appearance rediscovery passed `&[]` as the known list, so the first light/dark switch re-resolved every language as LTR, and an Arabic UI whose `layout()` read `is_rtl()` was rebuilt left-to-right. The resolution one-liner existed in 4 copies. | `dll/.../macos/system_style.rs:527,692,984`, windows `:345`, linux `:2611` | `17d270a00` |
| H2 | **The guide's fallback rule was not implemented.** Only the exact locale, an explicit chain and the exact default were tried, so `fr-CA → fr` did not happen. The guide's own setup registers `en`/`de` while every OS reports `en-US`/`de-DE`, so even with C1 fixed the guide's example would have rendered keys. | `layout/src/fluent.rs:734` | `5ded3bc82` |
| H3 | **`layout()`'s ICU helpers ignored `set_locale`, and they declared the wrong dependency.** They read `get_system_style()`, which records `Everything`: any theme or accent change rebuilt the DOM, while the locale dependency was never recorded. | `layout/src/icu.rs:1770-1860` | `299ce70fb` |
| H4 | **`AppConfig::check_translations()` (in the guide) did not exist.** Only `FluentLocalizerHandle::check_translations` existed, on a localizer the app never sees, returning a `Result`. It also `unwrap()`ed a mutex and used `#[cfg(feature = "logging")] log::warn!`: azul-layout has neither that feature nor a `log` dependency, so the `unexpected_cfgs` warning fails `-D warnings`. | `layout/src/fluent.rs:801,806,843` | `7bbf02dac` |

### Medium

| # | Finding | Where | Fix |
|---|---|---|---|
| M1 | **f32 arguments print widened.** `f64::from(0.1f32)` is 0.10000000149011612, and fluent-bundle 0.16 prints `f64::to_string()`. Affects `FluentArg::F32` and `data-l10n-price="0.1"`. | `layout/src/fluent.rs:260` | `bc9f8cc9f` |
| M2 | **Some strings become floats.** `data-l10n-name="Nan"` became `F32(NaN)` and rendered "NaN", because `f32::from_str` accepts nan/inf/infinity in any case, and exponent notation too. | `core/src/xml.rs`, `layout/src/xml/mod.rs` (duplicated) | `7b5acfdc9` (and dedupe `cd489a11f`) |
| M3 | **Doc comments were split from their items.** The commits inserted new items between existing doc comments and their items. `NodeData`, `LayoutCallbackInfoRefData`, `SystemStyle`, `export_to_zip` and `set_icon_provider` each lost their doc to the new neighbour, and `AzString::as_str` lost `#[inline] #[must_use]` (the hot accessor stopped inlining across crates). | several | `002d1fef6` |
| M4 | **Dead writes.** `depends_on_locale`/`depends_on_text_direction` were written once before the layout callback (always `false`) and again after it. | `dll/.../common/layout.rs:502` | `bb2d2f125` |
| M5 | **Unsafe `create` in api.json.** `FluentLocalizerHandle.create`'s fn_body calls `core::slice::from_raw_parts(known_languages.ptr, len)`. A C caller passing an empty slice with a null ptr is undefined behaviour, and `SystemLanguageVecSlice::empty()` *is* null. The body should use `known_languages.as_slice()`, which guards against null. | api.json (not edited, per rules) | **for autofix / parent** |
| M6 | **`add_resource` can reject an app's locale.** When a localizer is built with `known_languages`, `add_resource` silently returns `false` for any locale not in the list. The default list has 20 entries, so `nl`, `pl`, `sv`, or the guide's bare `en`/`de`, would be rejected. The product path now builds the localizer with an empty list, so nothing is rejected. See open question Q6. | `layout/src/fluent.rs:468` | avoided in `12a8f6162` |

### Low / notes (not fixed)

- **Performance:**
  - The localizer is parsed per window (`from_locale_sources` in `set_app_localization`), including popups and menus. That is small next to window setup, but a process-wide cache keyed by the sources would remove it.
  - Translation runs once per DOM rebuild, never per frame. The pre-cascade skip path does not translate.
  - Each translated node locks the bundle mutex and allocates `AzString`s for the locale and the key. `translate` takes owned `AzString`s and could take `&str`.
  - The `flags` byte grows every Vec from 40 to 48 bytes on 64-bit. `NodeData.fluent_args` adds 8 bytes to every node. See Q7.
- **Thread-safety:** `FluentLocalizerHandle` is an `unsafe impl Send + Sync` refcount over mutexes. It is sound as long as it is not copied bitwise across the C API; that is an existing pattern, not new. The `Cell<bool>` fields in `LayoutCallbackInfoRefData` are written only from the synchronous `layout()` call.
- **The web backend (`dll/src/web`) never translates.** It has its own `LayoutWindow`s and no `AppConfig` wiring.
- **The e2e runner has no localizer.** Its XML mounts are never translated, so localization is not testable by e2e scenario yet.
- **The a11y tree is not refreshed by in-place re-localization.** `ChangeNodeText` has the same gap.
- **No platform observes an OS language change on its own.** macOS picks it up only incidentally on an appearance change. Windows filters `WM_SETTINGCHANGE` "intl" out. Linux sees it only on a portal theme change. When a change *is* observed, all platforms now go through the common `adopt_system_style`: a full rebuild if the DOM depends on the locale, else in-place re-localization.
- **macOS reads `NSLocale.currentLocale.localeIdentifier`.** That is the *formatting region* locale, not the UI language (`preferredLanguages[0]`). A German UI on a US region reports `en_US`.
- **The default `known_languages` list lacks some RTL languages.** Pashto, Sindhi, Dhivehi, Yiddish and Sorani are missing. The primary-subtag rule only helps for languages the list names at least once.
- **Existing gaps inside `FluentLocalizerHandle`:** a resource that redefines one id is rejected whole even though Fluent registered its new ids, and an invalid locale silently gets an en-US bundle. Both are pinned by existing autotests and were not touched.

### The 915-line `macos/mod.rs` change (`60a211ee3`)

`git diff -w` shrinks it to 367 lines. Apart from two fields (`locale`,
`is_rtl`) that `e30647040`/`80637842e` later removed again, **all of it is
rustfmt**:

- reflowed calls;
- `unsafe fn f() { unsafe { … }}` re-indented to the rustfmt layout, with identical semantics.

There is no behaviour change and no regression to system-style or appearance detection.

`23b2a375b`'s "Fix macOS system style detection" threaded `known_languages`
into `discover_macos_cli_extras` and the rediscovery. The rediscovery half
introduced H1(b), which is now fixed. The appearance poll, the announcement
path and the per-theme rediscovery cache are unchanged.

## Commits (RED first, then fix)

| Commit | Kind | Test, and the expected failure before its fix |
|---|---|---|
| `002d1fef6` | docs/refactor | none (doc attachment only) |
| `bb2d2f125` | refactor | none (removes dead writes) |
| `cd36cc819` → `2f1308107` | RED → fix | `css corety::localizable_flag_tests::a_cloned_translation_key_is_still_a_translation_key` panics with "clone() dropped the localizable flag": `is_localizable()` is `false`, expected `true`. |
| `29a6bdbfa` → `bc9f8cc9f` | RED → fix | `layout fluent::tests::a_fractional_f32_argument_renders_as_written_not_widened`: left `"costs 0.10000000149011612"`, right `"costs 0.1"`. |
| `b65016528` → `5ded3bc82` | RED → fix | `…::a_key_missing_in_a_regional_locale_falls_back_to_its_language`: left `"greeting"` (the raw key), right `"Bonjour"`. |
| `75d5d28d1` → `27154b9f0` | RED → fix | `…::arguments_attached_to_an_element_reach_its_translated_text`: left `"Welcome back, {$userName}!"`, right `"Welcome back, Alice!"`. |
| `cd489a11f` | refactor | Shares one `data-l10n-*` parser across both parsers. Guarded by `test_data_l10n_with_fluent_args`. |
| `88df2e38e` → `c15cba982` | RED → fix | 4 tests. (1) core `xml_test::…::test_data_l10n_creates_localizable_text_node` and (2) `…::test_data_l10n_with_fluent_args`: node type `Text("greeting")`, expected `P`. (3) `…::a_data_l10n_element_keeps_its_tag_in_the_arena_builder_too`: 1 node, expected 2. (4) layout `xml_equivalence::a_data_l10n_element_keeps_its_tag_in_both_xml_paths` panics with "the <p> survives the core path". The two pinned core tests asserted the buggy shape and were rewritten. |
| `178dea7d1` → `7b5acfdc9` | RED → fix | `core dom_test::l10n_attribute_tests::a_word_that_float_parsing_accepts_stays_a_string_argument`: left `F32(NaN)`, right `String("Nan")`. |
| `120b00141` → `17d270a00` | RED → fix | `css system::system_language_tests::a_regional_variant_of_a_known_rtl_language_is_rtl`. **Compile-RED**: `SystemLanguage::resolve` did not exist, and the inline rule lived in cfg'd dll code no test reaches. With the old rule, `ar-DZ` gives `is_rtl: false`. This commit also fixes the Windows and Linux build breaks, which need `cargo check` on those targets. |
| `a2e26af0b` | fix | No test: a compile error under `--features web` (`cargo check -p azul-dll --features web`). |
| `c714814cd` → `7d8e914ff` | RED → fix | `core diff_test::dom_fingerprint_tests::a_changed_fluent_argument_changes_the_structure_fingerprint` and `…::a_text_becoming_a_translation_key_changes_the_structure_fingerprint`: `assert_ne!` fails because the two hashes are equal. |
| `eed748915` → `12a8f6162` | RED → fix | `dll/tests/localization_headless.rs`: `a_translation_key_in_the_app_dom_is_laid_out_as_its_translation` and `a_changed_argument_is_laid_out_after_the_next_rebuild`. **Compile-RED**: `AppConfig` has no field `fluent_locales`. With the field but no wiring, the texts are `["greeting", "unread"]`, expected to contain `"Hello"`. |
| `979e92636` → `7a3bc91c3` | RED → fix | Same file. `set_locale_relocalizes_the_laid_out_text_without_rebuilding_the_dom`: got `["Hello", "3 new emails"]`, expected "Hallo". `the_chosen_locale_outlives_the_next_rebuild`: same failure. `a_layout_that_read_is_rtl_is_rebuilt_when_the_locale_turns_rtl`: left `ShouldIncrementalRelayout`, right `ShouldRegenerateDomCurrentWindow`. |
| `6df7e92f9` → `299ce70fb` | RED → fix | `layout icu::layout_callback_locale_tests::layout_icu_formatting_follows_the_active_locale_and_declares_it`: left `"en-US"`, right `"de-DE"`. |
| `06fe73250` → `a5b535574` | RED → fix | `doc bug_classes::emitters_recognise_a_vec_through_the_shared_field_count_rule` panics with 14 offenders. The already-red `vec_category_is_exactly_the_vec_layout` goes green too. |
| `492206dfd` → `7bbf02dac` | RED → fix | `layout fluent::tests::app_config_check_translations_reports_the_keys_a_locale_lacks`. **Compile-RED**: there is no `check_translations` on `AppConfig`. |
| `596f1d8d1` | test | Hardens the set_locale test: it counts `layout()` calls across the change instead of pinning 1. |
| `d1b061bce` | docs | Guide example fixes (see "Doc changes"). |

### How `set_locale` works now (`7a3bc91c3`)

- **Where the key is kept.** The translation pass records each translated
  node's key in `NodeDataExt::l10n_key`. The field is internal, excluded from
  node identity like `marker`, and survives the flatten into the `StyledDom`.
- **What reads the locale.** `LayoutWindow::locale_override` holds what the app
  chose. `LayoutWindow::active_language()` returns that choice, else the
  system language, and it is the single source for translation, ICU, and
  `get_locale`/`is_rtl`.
- **What `SetLocale` does.** It resolves the locale's RTL-ness through
  `SystemLanguage::resolve` and `known_languages`, then asks
  `LocaleChange::needs_new_dom(depends_on_locale, depends_on_text_direction)`:
  - if the answer is yes, it returns `ShouldRegenerateDomCurrentWindow`, and the rebuilt DOM is translated into the new locale;
  - if no, `relocalize_laid_out_text()` re-translates every laid-out `StyledDom` in place. That goes through `fluent::localize_styled_dom`, which uses the same own-or-parent argument rule, then does `ChangeNodeText`'s invalidation. The result is `ShouldIncrementalRelayout`, and `layout()` is not called.
- **OS changes.** An OS language change that only needs a restyle re-localizes in place too (`adopt_system_style`).

## Public API changes (for `azul-doc autofix`)

**New:**

- `AppConfig.fluent_locales: StringPairVec`. This is a struct field, so it changes AppConfig's C layout.
- `AzStringPair::new(key, value)` (api name `StringPair::new`, used by the guide).
- `SystemLanguage::resolve(os_locale: &str, known: &[SystemLanguage]) -> SystemLanguage`.
- `FluentLocalizerHandle::from_locale_sources(&[AzStringPair]) -> Option<Self>`.
- `FluentArgKVVec::from_l10n_attributes(IntoIterator<(&str,&str)>)`. This is generic, so it is Rust-only.
- `AppConfigFluentExt::check_translations(&AppConfig) -> TranslationCompletenessReport`, a trait re-exported from `azul_layout` together with `TranslationCompletenessReport`. The report holds a `BTreeMap`, which is not FFI-safe (Q5).
- `NodeData::get_localization_key` / `set_localization_key`.
- `fluent::localize_styled_dom`.
- `LayoutWindow::{set_app_localization, active_language, set_locale, relocalize_laid_out_text, locale_override, known_languages}` and `LocaleChange`. These are Rust-side; LayoutWindow is not an api.json class.
- `ir::is_vec_field_count` (codegen-internal).

**Already public but missing from api.json:**

- `String.tr`, `String.is_localizable`, `String.set_localizable`. The guide tells users to call `String::tr`.
- `LayoutCallbackInfo.get_locale` and `LayoutCallbackInfo.is_rtl`, which the guide names. `get_locale` returns `&AzString`, so the api needs a by-value body such as `object.get_locale().clone()`.

**Signature and doc-only changes:**

- `LayoutCallbackInfo::{get_locale, is_rtl}` and `NodeData`/`Dom::{set,with}_fluent_args` now have doc comments.
- `CallbackInfo.set_locale` is unchanged in signature, but its behaviour now matches the guide.

**api.json issues to fix (not edited here):**

- M5: the `FluentLocalizerHandle.create` fn_body.
- `LocalizationConfig` is filed under module `image`.

## Doc changes

**Guide** (`d1b061bce`). The prose is untouched; only three examples changed, each where it disagreed with the code or with the guide itself:

1. "Translatable Strings": the Rust example's key `welcome-greeting` became `welcome-message`, the key the guide's own `.ftl` files define.
2. "Localizing XML": the comment said `"hello_key"`; it now says `"welcome-message"`, like the markup below it.
3. "Changing Locale": `info: CallbackInfo` became `mut info: CallbackInfo`, because `set_locale` takes `&mut self`.

**Code doc comments:**

- `002d1fef6` put five existing doc comments back on their items, word for word.
- The `NodeDataExt` identity comment ("Every field EXCEPT `marker`") now also names `l10n_key`, because the code excludes it too.
- The IR-builder and C++ `has_vec_layout` comments that describe the Vec layout now mention the `flags` byte. The C++ one said "exactly these four fields".
- All other doc comments added are on previously undocumented new items.

## Least sure to compile

Ordered by risk:

1. **cfg'd platform code that cannot be checked on this host:**
   - `dll/.../windows/mod.rs` (`WM_SETTINGCHANGE` rediscovery);
   - `dll/.../linux/system_style.rs` (`rediscovered_style_for`'s new parameter, `dump_discovered_style`);
   - `set_app_localization` calls in the wayland (4), x11, android and ios shells;
   - `dll/src/web/html_render.rs`, which builds only with `--features web`.
2. **`fluent::localize_styled_dom`.** `nodes: &mut [NodeData]` is read through a closure (`.and_then(|parent| nodes.get(parent))`) while `nodes[idx]` is also read. Both are shared reads, and the `&mut nodes[idx]` comes after the block, but this is the borrow-heaviest new code.
3. **`fluent::translate_subtree`.** It holds a shared borrow of `dom.root.fluent_args` while iterating `dom.children.as_mut()`. These are disjoint fields behind `&mut Dom`, which the borrow checker should accept.
4. **`fluent::less_specific_locales`.** It is `impl Iterator<Item = &str> + '_` over `core::iter::from_fn`, with a `move` closure that mutates the captured `rest`.
5. **`LayoutWindow::relocalize_laid_out_text` and `set_locale`.** They use `#[cfg(feature = "fluent")] { … true } #[cfg(not(…))] { false }` as the tail expression, and `#[cfg(feature = "icu")] self.icu_localizer.set_locale(..);` as a statement attribute. `discover_system_style` already uses the same pattern.
6. **`core::window::AzStringPair::new`** is a `const fn` that moves two `AzString`s (types with `Drop`) into the struct. Nothing is dropped in the body.
7. **`core/src/xml_test.rs`'s new arena test** calls the private `xml_node_to_fast_dom` through the test module's `pub use super::*` glob. The existing tests call `xml_node_to_dom_fast` the same way.
8. **`FluentArgKVVec::from_l10n_attributes`** uses the tuple pattern `if let (true, Ok(f)) = (plain_decimal, value.parse::<f32>())`.
9. **`dll/tests/localization_headless.rs`** as a whole: a new integration test (auto-discovered, gated `#![cfg(feature = "fluent")]`) that uses `PlatformWindow::apply_user_change`, `LayoutCallbackType`, `SystemLanguage` and `AzStringPair`.

## Open questions for the user

1. **What does an unknown key render as?** The guide says the "global default fallback behavior … can be configured to either render the raw translation key, or an empty string". Today it always renders the raw key, and there is no setting. Where should it live: `AppConfig`, `LocalizationConfig`, or `FluentLocalizerHandle`?
2. **Is `set_locale` per window or app-wide?** It currently changes only the window whose callback called it. Popups and menus created afterwards start in the *system* language, so the menus of a German-chosen app open in English. Should the choice be app-wide, for example held next to the localizer and inherited from `parent_lw` at creation?
3. **Should an RTL locale flip the document's base direction?** It does not today. `is_rtl` only reaches `layout()` as information; nothing sets CSS `direction: rtl` on the root the way `<html dir="rtl">` does. Relatedly, should `:lang()` / `@lang` CSS follow `set_locale`? The `DynamicSelectorContext` still uses the system language.
4. **Should the translation replace the element's markup content?** `<p data-l10n="k">Fallback</p>` now becomes `p > [text(k), text("Fallback")]`, with the key's translation first and the markup content kept. fluent-dom *replaces* the element's content instead. The guide's example is empty, so it does not decide.
5. **What C-API shape should `check_translations` have?** `TranslationCompletenessReport.missing_keys` is a `BTreeMap<String, Vec<String>>`. The guide also promises to check "all keys used in your code". The checker can only compare locales against each other: a key used in code but missing from every locale is not reported.
6. **What is `known_languages` for, beyond RTL-ness?** `80637842e` made `FluentLocalizerHandle::add_resource` reject locales not in the list. The product now passes an empty list (no rejection), because the default list would reject the guide's own `en`/`de` and any language not among its 20. Keep the validation, relax it to the primary subtag, or drop it?
7. **ABI and memory design.**
   - `NodeData.fluent_args` was added as a new public field of the `#[repr(C)]` `NodeData`, which grows every node by 8 bytes. NodeData's own doc names `extra: NodeDataExt` as the place to "retroactively add functionality to the node without breaking the ABI". Move the args there?
   - The `flags: u8` byte grows *every* Vec from 40 to 48 bytes. It could instead live in padding of the destructor enum's tag.
8. **Should Fluent's Unicode isolation marks be turned back on?** `FluentLocaleBundle::new` calls `set_use_isolating(false)`. In RTL UIs, interpolated LTR values (names, numbers) can then reorder around punctuation. Re-enabling isolation needs text3 to handle U+2068/U+2069.
9. **Should the platforms observe OS locale changes?** That would mean `NSCurrentLocaleDidChangeNotification`, `WM_SETTINGCHANGE` with `"intl"`, and the portal. Alternatively, keep `set_locale` as the only supported runtime switch.
10. **Which language should the default locale be?** `from_locale_sources` uses `en-US` (like `FluentLocalizerHandle::default`) as the last-resort locale. An app shipping only `de`/`fr` gets no default-locale hit. Should the default be the *first* registered locale instead?
