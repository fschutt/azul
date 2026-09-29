# R4 - the rice loader (2026-09-29)

Branch `wt/r4-rice-loader`, base `0a326afe5`. Design:
`scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` (4.1, 4.4, 7, 9, 9.2,
pitfalls 7, 8, 12, gaps 4, 5). Nothing compiled (house rule); every touched file was
syntax-checked with `rustfmt --check`.

## What was built

**`css/src/rice.rs` (new, `azul_css::rice`)**

- Discovery over the theme chain: for each chain entry `n`, `<root>/css/<n>/*.css` with `:`
  as a path segment (`xyz:pink` -> `css/xyz/pink/`); global files in `<root>/css/*.css`
  (their `theme:` header names their theme; without one they are unthemed); the legacy
  `styles/<app>.css` (decided: keeps loading as a **per-app, unthemed** file, at its header's
  priority, `base` by default - today's behaviour). Root = `~/.azul`
  (`%USERPROFILE%\.azul`), injectable through `RiceEnv::with_root(root, app)`; every test
  uses a temp dir.
- **Shared directory walk for I1**: `azul_css::rice::chain_directories(root, kind, chain)`
  (`kind` = `"css"` or `"icons"`), with `theme_dir_segments(name)` (validation, `:` split) and
  `default_rice_root()`. I was not told to talk to I1; the parent unifies.
- Header meta-comment (`parse_rice_header`): first `//` lines or `/* */` block, keys `theme`
  (`name@version`), `priority`, `fallback`, `app`, `azul`, `requires`; an unknown key warns
  when the comment has a known key (so typos do not pass silently, plain comments stay silent).
- Priorities (`RicePriority`) onto `rule_priority`: `base` = SYSTEM 10 (default),
  `app` = APP 25, `widgets` = WIDGETS 35, `force` = FORCE 60, `palette` = PALETTE (= 35: a
  spin-off's `:root` block has to meet its base theme's definitions in one slot so rank
  decides), `off` = inert. New consts `APP`, `WIDGETS`, `PALETTE`, `FORCE` in
  `css::rule_priority`.
- Palette: a header-less file whose every declaration is `--name` is a palette by inspection
  (text-level check, independent of R1); a declared palette that sets anything else is applied
  at `base` as a whole, with a warning.
- Clamp: an unthemed file above `base` is clamped to `base` with a warning (palette stays).
- Every applied rule is wrapped in `@theme(<n>)` (theme condition FIRST, file conditions kept)
  and stamped; emitted order: themes outside the chain, unthemed, then chain floor -> head;
  per-app after global within a theme. At equal priority R2's rank decides; source order agrees.
- `azul:` gate (the one gate): OR list, `*` wildcard, a short version is a prefix; mismatch =
  file ignored, exactly one warning line. `requires:` (Cargo `^`, `~`, `=`, `>=`.., `1.*`)
  never gates: applied anyway, `requires_ok = false`, message in status + one warning.
- Pitfall 7: per-app rules come after the global rules of their theme; an explicit
  `priority:` in a per-app file replaces the priority of the global files of the same theme
  for this app, both directions (`off` turns them off; conflicting per-app files: lowest wins,
  warning).
- Pitfall 8: remote `url()` declarations dropped (any scheme but `data:` and host-less
  `file:`, plus `//host` and `\\host`), `@import` dropped, all comments and full-line `//`
  comments blanked before parsing (simplecss has no `//`), 1 MiB per file, 256 files per dir,
  theme names `[A-Za-z0-9_-]` segments only and not `light`/`dark`, symlinks resolving outside
  the root refused.
- Status (`RiceStatus`, repr(C)): root, app, azul version, chain, files (`RiceFileStatus`:
  path, theme, version, app, requires, requires_check, note, rule range, live/inert counts,
  state, priority, rule_priority, per_app, requires_ok), warnings, mode. `LoadedRice::
  status_under(ctx)` counts live vs inert by evaluating each rule's conditions
  (pseudo-state / container conditions count as live). `RiceStatus::to_report()` prints it and
  ends with the support policy + the `AZ_RICING=off` self-check.
- Process state: `install(env)` / `install_with_mode`, `rice_for_theme(head)` (cached per
  head), `generation()`, `poll_watch()`, `take_reload_signal()`, `take_pending_status(ctx)`,
  `rice_status(ctx)`, `installed_fallback_of(name)`, `SystemStyle::get_rice_status()`.
  Nothing reads `$HOME` before `install`.
- `fallback_of(env, name)` answers R3's `fallback_of` from the headers (dir files + global files
  naming the theme, only files for this app/azul, file order, deduplicated);
  `rice_chain(env, head, app_default)` calls `crate::theme_chain::expand_chain`.
- `RicingMode::Watch` (`watch`/`live`/`reload`), `RicingMode` now `repr(C)`;
  `ricing_mode_from(Option<&str>)`. `AZUL_VERSION` moved to the css crate root (codegen
  re-exports it) - the running version for `azul:`.

**core / layout**

- `StyledDom::create_from_dom_with_user_sheets(dom, ctx, &[Css])`: user-origin sheets appended
  after the DOM's own, UNSCOPED (hung on the root Dom a rice `* {}` at >= INLINE would be
  node-only). `create_from_dom_with_context` delegates. `icon::
  styled_dom_resolving_icons_with_user_sheets` keeps the resolve+cascade pair one call.
- `LayoutWindow::style_user_dom_in_scope` cascades with `rice_for_theme(self.app_theme)` -
  this was the missing piece: `SystemStyle.app_specific_stylesheet` was loaded by the shells
  and **never read by anything**.
- `LayoutWindow.rice_generation: u64` (+ both field-audit patterns).

**dll**

- `App::create` installs the loader (`RiceEnv::from_process()`) and starts the watcher.
- The three shell twins `load_app_specific_stylesheet` (macOS / Linux + `get_config_dir` /
  Windows) are deleted; `app_specific_stylesheet` is no longer filled (doc says so, kept for ABI).
- Watch: thread polls `poll_watch()` every 500 ms -> `loop_waker::wake()` -> the app-event
  collector (`AppEvents.rice_reloaded`) calls `PlatformWindow::rebuild_all_windows_for_app_theme`,
  which is the `SetTheme` tail extracted (SetTheme now calls it) - no second path.
  `regenerate_layout` treats `rice_generation` lag exactly like `app_theme` lag
  (`AppThemeChange`, disables the pre-cascade skip).
- After styling, a pending status is logged at info (`AZ_DEBUG` shows it), warnings at warn.

**docs**: `doc/guide/en/styling/ricing.md` (layout, header, priorities table, per-app,
hardening, status, `AZ_RICING` incl. watch, current limits, support-policy sentence with
`AZ_RICING=off` for bug reports); `themes.md` points at it; `styling.md` lists it;
`autodoc-groups.toml` registers it.

## Commits

- `5b2670ce7` test(rice): the rice loader over theme directories, RED
- `51cbb1f07` feat(css): the rice loader over theme directories
- `bed897f0f` feat(layout): windows cascade the rice as a user-origin sheet
- `6762b0e12` feat(dll): install the rice loader; AZ_RICING=watch rebuilds through the app-theme path
- `8c278fd2c` docs(styling): the ricing guide page
- `7f6fd5724` style(css): rustfmt the rice loader
- (this report + progress file)

## Dependencies on parallel work

- **R3 (hard, compile)**: `azul_css::rice::rice_chain` calls
  `crate::theme_chain::expand_chain(head, &fallback, app_default)` and reads
  `.names` / `.warnings` via `.iter()` + `.as_str()` (works for `Vec<String>` or `StringVec`).
  **The css crate does not compile until R3's `css/src/theme_chain.rs` is merged.**
  Integration step for the parent: R3's context chain (`dynamic_selector::app_theme_chain`)
  should feed `expand_chain` with `azul_css::rice::installed_fallback_of`, so the cascade's
  chain and the rice's chain are the same chain (otherwise `css/<fallback>/` rules are wrapped
  in `@theme(<fallback>)` but inert because the context chain lacks the name). Do not call
  `installed_fallback_of` while holding the rice lock (std Mutex, not reentrant) - nothing in
  rice.rs does.
- Test needing R3: `the_rice_chain_is_the_theme_chain_fed_by_the_fallback_headers`.
- **R2**: rank from the `@theme(n)` wrap; no test here needs R2 (source order agrees with rank,
  and the layout tests load one theme).
- **R1**: no test here needs R1. A palette rice's `:root { --x }` reaching a widget's
  `var(--x, system:..)` needs R1; add that end-to-end test after R1 (palette stamping and
  inference are pinned here, text-level).

## api.json (for autofix; nothing edited by hand)

New types (module `css` unless the autofix prefers `rice`):

- `RicingMode` - `external: azul_css::system::RicingMode`, `repr: C`, enum
  `Off | Default | Force | Watch`, derive Debug Clone Copy PartialEq Eq, custom_impls Default.
- `RicePriority` - `azul_css::rice::RicePriority`, `repr: C`, enum
  `Off | Palette | Base | App | Widgets | Force`, derive Debug Clone Copy PartialEq Eq
  PartialOrd Ord Hash, Default (Base).
- `RiceFileState` - `azul_css::rice::RiceFileState`, `repr: C`, enum
  `Applied | OtherApp | AzulVersionMismatch | Off | Refused`, derive Debug Clone Copy
  PartialEq Eq PartialOrd Ord Hash.
- `RiceFileStatus` - `azul_css::rice::RiceFileStatus`, `repr: C`, derive Debug Clone
  PartialEq Eq, struct_fields in this order (decreasing alignment): `path: String`,
  `theme: String`, `version: String`, `app: String`, `requires: String`,
  `requires_check: String`, `note: String`, `first_rule: usize`, `rule_count: usize`,
  `live_rules: usize`, `inert_rules: usize`, `state: RiceFileState`,
  `priority: RicePriority`, `rule_priority: u8`, `per_app: bool`, `requires_ok: bool`.
- `RiceFileStatusVec`, `RiceFileStatusVecDestructor`, `RiceFileStatusVecDestructorType`,
  `RiceFileStatusVecSlice`, `OptionRiceFileStatus` - the usual `impl_vec!` / `impl_option!`
  family (`azul_css::rice::...`), like `SystemLanguageVec`.
- `RiceStatus` - `azul_css::rice::RiceStatus`, `repr: C`, derive Debug Clone PartialEq Eq
  Default, struct_fields: `root: String`, `app: String`,
  `azul_version: String`, `chain: StringVec`, `files: RiceFileStatusVec`,
  `warnings: StringVec`, `mode: RicingMode`.

New functions:

- `SystemStyle.get_rice_status` - `fn_args: [{"self": "ref"}]`, returns `RiceStatus`,
  `fn_body: "object.get_rice_status()"`. Doc: "What the rice loader loaded - the theme chain,
  every file with its priority, version, requires check and live versus inert rules under
  the context this system style describes - and the warnings."
- `RiceStatus.to_report` - `self: ref`, returns `String`, `fn_body:
  "object.to_report().into()"`. Doc: "The listing as text, ending with the support policy and
  the AZ_RICING=off self-check."
- `RiceStatus.live_rule_count` - `self: ref`, returns `usize`.
- `RiceStatus.is_beyond_base` - `self: ref`, returns `bool`.

Changed docs: `SystemStyle.app_specific_stylesheet` (no longer filled by discovery; kept for
ABI; see `get_rice_status`).

Not in api.json (Rust-only): `StyledDom::create_from_dom_with_user_sheets`,
`icon::styled_dom_resolving_icons_with_user_sheets`, `LayoutWindow.rice_generation`, the rest
of `azul_css::rice`, `rule_priority::{APP, WIDGETS, PALETTE, FORCE}`, `ricing_mode_from`,
`azul_css::AZUL_VERSION`.

## Least sure to compile

1. `rice_chain` -> R3's `expand_chain` (name, argument order, `&dyn Fn(&str) -> Vec<String>`,
   field names `names` / `warnings`). Fix the call site to R3's real signature.
2. `crate::impl_option!` / `crate::impl_vec!` / `impl_vec_mut!` / `impl_vec_clone!` /
   `impl_vec_partialeq!` / `impl_vec_eq!` for `RiceFileStatus` (copied from the
   `SystemLanguage` pattern; `impl_vec_debug!` too).
3. `with_process`: `f(&mut guard)` relies on `&mut MutexGuard<T>` -> `&mut T` deref coercion.
4. `layout/src/window.rs` `style_user_dom_in_scope`:
   `rice.as_deref().map_or(&[][..], |loaded| core::slice::from_ref(&loaded.css))` lifetime.
5. `dll/src/desktop/app.rs` `start_rice_watcher`: `.spawn(|| loop { .. })` (diverging closure;
   common pattern) and `std::sync::Once` static.
6. `load_rice`: partial moves out of `parsed: Css` (`parsed.rules`, later `parsed.keyframes`)
   - fine because `Css` has no `Drop`.
7. `dll/src/desktop/shell2/windows/system_style.rs`: dropped the `boxed::Box`, `css::Css`,
   `parser2::new_from_str` imports (grep found no other use; the crate allows unused imports
   anyway, but a missing one would not be).
8. `version_satisfies`: `[u64; 3]` comparisons and the `find_map` over operator prefixes.

## Test commands (parent)

```sh
cargo test -p azul-css --test rice_loader            # needs R3 merged (crate compiles only then)
cargo test -p azul-css --lib system                  # ricing mode tests (Watch added)
cargo test -p azul-layout --test all rice_styles_the_window
cargo test -p azul-layout --test all rice_styles_the_window -- --ignored   # the inline gap, expected RED
cargo test -p azul-dll --test app_theme_headless     # SetTheme tail extracted
cargo test -p azul-css --features codegen,parser --test codegen_goldens   # AZUL_VERSION re-export
cargo check -p azul-dll --features build-dll         # on macOS, Linux and Windows (shell loaders removed)
```

## Findings / gaps (not fixed here)

- **Widget inline properties beat every rule** (`CssPropertyCache` "PRIORITY 1" inline lookup
  before `css_props`, whatever the rule's priority). So `widgets`/`force` beat `with_css`
  rules (INLINE rules, tested) and the app's stylesheets, but not a widget's
  `with_css_props`. Pinned by the ignored test
  `a_widgets_rice_beats_a_widgets_static_inline_property`; needs the inline-vs-component
  unification the `rule_priority::INLINE` doc mentions.
- **A global `* {}` rule is looked up after every per-node rule** (`global_css_props`,
  "PRIORITY 2b"), whatever its priority - a `force` `* { color }` loses to an author `.x`
  rule. Relevant to pitfall 3.
- **Behaviour change**: the rice is now actually applied (the legacy file was loaded and
  dropped before). E2E / reftest runs through `App::create` on a machine with `~/.azul/css`
  or a legacy file will pick it up; CI and screenshot tooling should set `AZ_RICING=off`.
- Watch on Windows: `loop_waker::wake()` posts nothing on Windows, so the reload lands with
  the next window message.
- Not built (outside the task list): `AppConfig.ricing_policy` / `Beyond::Clamp` (4.4), the
  About panel widget itself, crash-mail attribution, the icon remap loader (I1), the
  accessibility floor (gap 3). `RiceStatus::is_beyond_base` is the hook for 4.4.
- Twins: the three shell loaders were twins (removed). `rice::legacy_stylesheet_path`
  re-derives the per-OS config dir that `azul_layout::file::FilePath::get_config_dir` gets
  from the `dirs` crate; the css crate cannot depend on layout.
- The CSD titlebar and E2E mount-XML DOMs are styled outside `style_user_dom_in_scope`, so the
  rice does not reach them.
