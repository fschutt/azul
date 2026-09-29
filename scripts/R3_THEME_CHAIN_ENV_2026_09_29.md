# R3 - theme chain and the environment variables (2026-09-29)

Branch `wt/r3-theme-chain-env` from `0a326afe5`. Design:
`scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` §4.1, §7.1, §7.4 step 3 and
§9.1 pitfalls 4 and 5. The user's naming ruling applies: "theme" means flat, flora or a user theme,
and "mode" means light, dark or system. So the doc's `AZ_COLOR_SCHEME` is **`AZ_MODE`** here.

> **Merge note: the last code commit needs R2.** `257018134` makes every context chain end in the
> default theme, so `with_theme("flora")` gives `[flora, flat]`. The matcher still tests plain
> membership, which makes flat's blocks live under flora as well. R2's rule that a compiled-in
> theme below another compiled-in theme in the chain is inert (plus the rank) is what keeps flora
> exclusive. Without R2, these go red on this branch:
>
> - `css --test app_theme_selects_theme_blocks a_flora_block_applies_only_under_flora_and_a_flat_block_only_under_flat`
>   (`!live(flat, &under_flora)`);
> - the widget comparisons that resolve a flora context and expect flora-only output:
>   `layout/src/widgets/themes/theme_blocks.rs` (`live_rules(.., Flora)`), `theme_checks.rs`, the
>   flora cases of `layout/tests/widgets_follow_the_app_theme.rs`, and the `with_app_theme`
>   tests in `flat.rs`, `divider.rs` and `video.rs`.
>
> Merge R2 together with this commit, or hold back `257018134` (with RED `3b534a3a1`) until R2
> has landed. The other commits do not depend on R2.

## What was built

### 1. `css/src/theme_chain.rs` - the chain builder (interface as specified)

```rust
pub struct ThemeChain { pub names: Vec<AzString>, pub warnings: Vec<String> }
pub fn expand_chain(head: &str, fallback_of: &dyn Fn(&str) -> Vec<String>, app_default: &str) -> ThemeChain
```

- The head expands by its `:` prefixes, longest first: `a:b:c` gives `[a:b:c, a:b, a]`.
- Every entry's `fallback_of(name)` list is then appended in chain order, breadth-first. An entry
  added this way has its own list read in turn, and a fallback that is itself a spin-off brings
  its prefixes along. The head's list comes before a base's list, so where the two headers
  disagree the head wins (pitfall 4): `xyz:pink -> native` and `xyz -> flora` give
  `[xyz:pink, xyz, native, flora, flat]`.
- Duplicates keep their first (highest) rank, so a diamond is silent.
- A cycle is cut with one warning. A cycle is a `fallback:` edge back to a theme the current
  entry builds on. Each entry records which entry brought it in, which is how a cycle is told
  apart from a diamond.
- `app_default` is always the last entry and appears only there. A header that names it does not
  move it up, and its own `fallback:` is not followed.
- The mode words `light`, `dark`, `system` and `auto` (any case) are dropped with an error line
  pointing at `AZ_MODE` (pitfall 5). This includes a prefix: `dark:pink` keeps `dark:pink` and
  drops `dark`. Malformed names are dropped the same way: bad characters, or an empty `:`
  segment as in `xyz::pink`, `:x` or `x:`.
- `is_theme_name` is the character rule `ThemeCondition::from_block_name` already had. Both now
  use this one helper, so the rule has a single definition.

### 2. The environment (`ThemeEnv`, read once per process)

- `ThemeEnv::from_values(az_theme, az_mode)` is the testable core. `theme_env()` reads the
  process environment once (a `OnceLock`). The tests never call `set_var`, so nothing leaks into
  other tests. That is the same rule the `AZ_RICING` tests follow.
- **`AZ_MODE=light|dark|system`** is the mode pin, with the old `AZ_THEME` pin's semantics.
  `system` or `auto` means no pin. An unknown value is ignored, with a warning.
- **`AZ_THEME=<theme>`** is the head of the theme chain. The order is **`AZ_THEME` > the app
  (`AppConfig::with_theme` / `CallbackInfo::set_theme`) > `DEFAULT_APP_THEME`**. This is one
  decision, `theme_chain::resolve_theme_head`, which `azul_core::app_theme::app_theme()` and the
  new `resolve_app_theme(choice)` apply. So window creation, `regenerate_layout`, the build scope
  (`current_theme`) and the E2E runner all see the environment's theme.
- **Alias:** `AZ_THEME=light|dark` (or `system`/`auto`, which are mode words) still pins the mode
  for one release, and logs a deprecation line saying to set `AZ_MODE=<value>`. When both are
  set, `AZ_MODE` decides and the alias is reported as ignored.
- **One reader of the pin:** `theme_pinned_by_env` is now `mode_pinned_by_env` and
  `apply_env_theme_pin` is now `apply_env_mode_pin`. Every caller is renamed: css, the layout
  window/callbacks/widgets, the dll shells and the tests. `from_system_style`'s private duplicate
  of the env read (`option_env_theme`) is deleted.
- **Logging:** `set_app_theme` (called by `App::create` and `SetTheme`) emits through
  `azul_core::diagnostics::emit` as `[azul][warn] ...`. It emits the environment's warnings once
  per process, and on every call the chain warnings for the resolved head (a mode word or
  malformed name, a cycle).
- **dll `SetTheme`:** it now publishes first, then compares the window with what the app
  *resolves* to. Under `AZ_THEME`, a switch no longer rebuilds every window for nothing.
- `scripts/screenshot_single.sh` pins the mode with `AZ_MODE`.

### 3. The context chain goes through `expand_chain`

- `dynamic_selector::app_theme_chain(name)` was already the one function every `theme_chain`
  write goes through: `Default`, `from_system_style`, `with_app_theme` and
  `LayoutWindow::dynamic_selector_context`. It is now
  `expand_app_theme_chain(name).names`, and `expand_app_theme_chain` calls
  `expand_chain(name, <no fallbacks>, DEFAULT_APP_THEME)`.
- **R4 hook:** `expand_app_theme_chain` is where the rice loader plugs in its header map
  (`fallback_of`). Today it returns nothing.
- Results: `flora` gives `[flora, flat]`, `xyz:pink` gives `[xyz:pink, xyz, flat]`, and `dark`
  gives `[flat]`.

### 4. Docs

- `doc/guide/en/styling/themes.md`:
  - the chain paragraph;
  - a new section "Choosing the theme and the mode from the environment", covering both orders,
    `system`, the alias and examples;
  - the stale sentence "There is no env var that forces dark/light mode" is removed;
  - `css/src/theme_chain.rs` is added to `tracked_files`.
- `doc/guide/en/debugging.md`: `AZ_MODE` and `AZ_THEME` added to the environment flags list.

## Commits

| hash | what |
|---|---|
| `47d7d1bf6` | RED: `expand_chain` unit tests (stub) |
| `b77a50979` | `expand_chain` + shared `is_theme_name` |
| `e58dd4228` | RED: `ThemeEnv` / `resolve_theme_head` tests (stub) |
| `d2b2855a8` | `AZ_MODE`, `AZ_THEME` head, alias + deprecation, pin renames, core/dll/e2e wiring, screenshot script |
| `d752a09b5` | progress checkpoint |
| `3b534a3a1` | RED: context chain tests (css + layout), `monokai` now shows the default look |
| `257018134` | `app_theme_chain` through `expand_chain`, `set_app_theme` reports chain warnings (**needs R2**) |
| `a14a2b105` | guide docs |
| `e821cbf55` | rustfmt on `theme_chain.rs`; reworded two doc lines (accidental list item / block quote) |

## api.json

**Nothing.** No FFI type or function changed:

- `theme_chain` (`ThemeChain`, `ThemeEnv`, `expand_chain`, `theme_env`, `resolve_theme_head`,
  `is_theme_name`, `is_reserved_theme_name`) is Rust-only. It uses `Vec`/`String` and is not
  `repr(C)`.
- `dynamic_selector::expand_app_theme_chain`, `mode_pinned_by_env`, `system::apply_env_mode_pin`
  and `azul_core::app_theme::resolve_app_theme` are Rust-only free functions. None of them, nor
  the renamed originals, are in api.json today.
- `DynamicSelectorContext::theme_chain` got a doc-only edit. The field is not in api.json yet.

## Tests

- `css/src/theme_chain.rs` unit tests:
  - chain: prefixes, default alone, transitive fallbacks, the head's header wins, diamond
    dedupe, three cycle shapes each giving exactly one warning, the default always last, mode
    words, malformed names;
  - environment: `AZ_MODE` pins, `system` pins nothing, unknown `AZ_MODE` warns,
    `AZ_THEME=dark` still pins and points at `AZ_MODE`, `AZ_MODE` outranks the alias,
    `AZ_THEME` is the head, blank means unset, env > app > default.
- `css/tests/app_theme_selects_theme_blocks.rs`:
  - `the_theme_chain_is_the_app_theme_over_the_default_theme` replaces `..._alone_for_now`;
  - `az_theme_xyz_pink_gives_the_context_the_chain_xyz_pink_xyz_flat`;
  - `a_mode_word_as_the_app_theme_leaves_the_default_chain`;
  - the `monokai` half of `a_flora_block_applies_only_under_flora...` now expects flat's block
    live (the floor).
- `layout/tests/app_theme_override.rs`:
  - `a_spin_off_app_theme_gives_the_window_its_prefix_chain_over_the_default`;
  - `monokai` now expects `FLAT` (the default look) in the stylesheet and inline-widget tests.

A Python port of `expand_chain` passes every chain case
(scratchpad `r3_chain_sim.py`). `rustfmt --check` parses every edited `.rs` file.

## Commands for the parent

```
cargo test --release -p azul-css --lib theme_chain
cargo test --release -p azul-css --lib dynamic_selector
cargo test --release -p azul-css --test app_theme_selects_theme_blocks
cargo test --release -p azul-css --lib system
cargo test --release -p azul-core --lib app_theme
cargo test --release -p azul-core --lib callbacks_test
cargo test --release -p azul-layout --test all app_theme_override
cargo test --release -p azul-layout --test all app_color_scheme_override
cargo test --release -p azul-layout --test all widgets_follow_the_app_theme   # needs R2
cargo test --release -p azul-layout --lib themes                               # needs R2
cargo test --release -p azul-dll --test app_theme_headless
cargo test --release -p azul-dll --test color_scheme_headless
cargo test --release -p azul-dll --test headless_lifecycle
cargo test --release -p azul-dll --lib --features build-dll initial_window_theme_tests
cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings
```

## Least sure to compile / lint

1. `theme_chain.rs` test helper
   `headers(&'static [(&'static str, &'static [&'static str])]) -> impl Fn(&str) -> Vec<String>`
   is called with literals such as `&[("abc", &["native"])]`. It relies on the inner `&[_; N]`
   coercing to `&[&str]` at the tuple and array coercion sites. Each table uses one array length.
2. `expand_app_theme_chain` passes `&|_: &str| Vec::new()` as `&dyn Fn(&str) -> Vec<String>`.
   It relies on the return type being inferred from the trait object.
3. `resolve_theme_head` is a `const fn` matching `(Option<&'a str>, Option<&'a str>)` and
   returning `DEFAULT_APP_THEME` (`&'static str`) as `&'a str`.
4. `ModeWord::pin` is a `const fn` returning `Some(ThemeCondition::Light)`. `ThemeCondition`
   has an `AzString` variant (it has a destructor), but only unit variants are built here.
5. Clippy nursery on `ThemeEnv::from_values`: I avoided `if let ... else` on `Option` (because
   of `option_if_let_else`) by comparing a `ModeWord` enum instead.
6. `layout/src/e2e/runner.rs` `SetTheme` arm: `let theme = resolve_app_theme(Some(theme.as_str()))`
   shadows the matched `&AzString` with an owned `AzString`.

## Files other tasks own (minimal edits)

- **R0 (naming):**
  - the one-token renames `theme_pinned_by_env` -> `mode_pinned_by_env` and
    `apply_env_theme_pin` -> `apply_env_mode_pin`, plus the `AZ_THEME` -> `AZ_MODE` word in
    comments next to them, in `layout/src/window.rs`, `callbacks.rs`, `widgets/{date_picker,
    stepper,pagination,segmented}.rs`, `dll/.../event.rs`, the three `system_style.rs` files,
    `dll/tests/{color_scheme_headless,headless_lifecycle}.rs` and
    `layout/tests/app_color_scheme_override.rs`;
  - the dll `SetTheme` arm (3 lines).
  - I deliberately did **not** edit the api.json-backed docs that R0 is rewriting anyway:
    `CallbackInfo::set_color_scheme` / `get_resolved_color_scheme` (`layout/src/callbacks.rs`
    ~1800/1817), `AppConfig::color_scheme` (`core/src/resources.rs` ~1046) and
    `WindowCreateOptions::theme` (`layout/src/window_state.rs:47`). They still say "the
    `AZ_THEME` pin", which stays true for one release. R0 should say `AZ_MODE` there.
    `core/src/callbacks_test.rs:2136` says the same and is left to R0 too.
  - The guide section names `AppConfig::color_scheme` / `CallbackInfo::set_color_scheme`. R0's
    rename should update that line.
- **E1:** `layout/src/e2e/runner.rs`, the `SetTheme` arm (3 lines).

## What is left

- **R2:** the exclusive-floors rule and the rank (see the merge note).
- **R4:** plug the rice header map into `expand_app_theme_chain` (`fallback_of`). The loader
  calls `expand_chain` itself for the directories.
- **Structural base (not in scope):** `UiTheme::current()` maps only exact names. Under
  `flora:abc` a widget builds flat's structure. §7.1 says the structural base is the
  highest-ranked compiled-in theme in the chain, so that lookup should walk the chain.
- **Next release:** drop the alias. `ModeWord` in `ThemeEnv::from_values` then only runs for
  `AZ_MODE`, and a mode word in `AZ_THEME` becomes a plain reserved-name error.
