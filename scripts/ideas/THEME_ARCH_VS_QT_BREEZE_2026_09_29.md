# Would Azul's theming avoid the Qt/Breeze class of bug? (2026-09-29)

Trigger: linux.org.ru thread 18389093 (Qt 5.15 + Breeze 6.7 draws a black frame around every
alternate-row cell in a `QTableView` with `SelectRows`). Verified against Breeze master `554bd5d`
and qtbase 5.15 / 6.8; write-up in `help/homewallet-qt-review.md`.
Azul tree: `fix/input-bugs-2026-09-19` @ `8020e34c3`. Pairs with
`doc/THEME_CHAIN_ANALYSIS_2026_09_12.md` (invariants I1–I9 referenced below).

The question: the theme rework is planned as three ordered steps, (1) Flat/Flora widget themes
with DOM recreation, (2) the `@theme` rule/API, (3) the native theme combined with `@os`. Is that
sufficient to exclude this class architecturally, not just make it rarer?

## 1. The Qt failure, reduced to mechanisms

Facts: Breeze `aba0f922` (2026-02) routed the alternate-row background through
`Helper::renderViewItemPosition(pos, bg, outline = QColor())`; the `Invalid` position branch calls
`setPen(outline)`; `QPainter::setPen(QColor)` substitutes black for an invalid colour. Qt 5.15's
`QTableView` never fills `viewItemPosition` (Qt 6.8 does), and Breeze forces `OnlyOne` unless the
view uses `SelectRows`. Three sibling code paths already guard `outline.isValid()`; one did not.
Unfixed in master, no bug filed, and the same commit already caused two text-elision bugs.

| # | Mechanism | In the Qt case |
|---|---|---|
| M1 | Painting is *code* in a plugin chosen and versioned by the OS, not the app | a distro update changed every Qt 5 table; the app can only override the whole style |
| M2 | Widget→style contract is a struct of optional fields with an `Invalid` default, filled or not depending on toolkit version, with no exhaustiveness check | `viewItemPosition` |
| M3 | An unset value becomes an opinionated concrete one instead of "draw nothing" | invalid `QColor` → black pen |
| M4 | Per-cell painting must reconstruct row context because the table has no row object | Beginning / Middle / End needed by the style, unavailable to it |
| M5 | Two styling systems layered with per-property precedence known only from the source | QSS `alternate-background-color` only changes the palette and still runs Breeze's painter |
| M6 | No combinatorial rendering test; the style is built for two toolkit versions from one tree | Qt 5 × table × `SelectRows` × alternate was never rendered by anyone before release |

## 2. Where Azul stands today (verified on the tree)

| Mech. | Azul today | Verdict |
|---|---|---|
| M1 | No style plugin. Widgets are DOM + inline `CssPropertyWithConditions`; `UiTheme::{Flat, Flora}` (`layout/src/widgets/themes/mod.rs:10`) selects a Rust *declaration generator* compiled into the app. Custom paint exists only for GL surfaces (`node_graph.rs`, `capture_common.rs`). The OS supplies values only: `SystemStyle` (colours, fonts, metrics via probes) and the ricing CSS file (`~/.config/azul/styles/<app>.css`, declarations only, `AZ_RICING=off`). | Excluded by construction: an OS update can change values, never painting logic. Price: the native look lags the platform (macOS 26 already does, `scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md`). |
| M2 | One `DynamicSelectorContext` (theme, os, viewport, language, pseudo-state), always fully populated; a non-matching condition falls through to the base declaration, not to an `Invalid` branch. The 09-12 analysis' R2 (cascade ran before the context existed) was the same shape as Qt 5 not filling the field; `fa9aefec4`, `cda56baed` and `5df176033` landed since, `set_dynamic_selector_context` now recascades on `theme_changed` (`core/src/styled_dom.rs:2458-2501`). I2's `debug_assert!(dynamic_context.is_some())` is not in the tree. | Mostly excluded. Add I2. |
| M3 | `system:` colours fall back to themed neutrals (`SystemColorRef::fallback(dark)`, `css/src/props/basic/color.rs:1612`), which is the right shape. The themed UA table landed (`a52fdb33a`), but `unwrap_or(ColorU::BLACK)` (`layout/src/solver3/getters.rs:2559`) and `DEFAULT_TEXT_COLOR` (`core/src/prop_cache.rs:2649`) are still live fallbacks, and widgets carry hard-coded light Bootstrap values (`button.rs:146-201`). "Missing value → black regardless of theme" is exactly the Breeze bug. | Not excluded yet. Turn the seeds into `debug_assert!`s (I3's last step). |
| M4 | ListView rows are DOM containers (`__azul_native-list-rows-row` with cell children, `list_view.rs:620-632`); hover / focus / selection are pseudo-states or classes on the *row* node; `:nth-child`, `:first`, `:last` exist (`css/src/css.rs:2049-2116`). A selection pill is one node's background and radius; no cell needs neighbour information. | Excluded as long as row nodes stay. A cell-virtualised grid (cells as direct children) would bring it back. Alternating rows are a selector on the row class (`:nth-child(odd|even)`, `doc/guide/en/styling.md:169`), not a widget flag; the ListView rows do not declare them themselves yet (NATIVE ref §6.7). |
| M5 | One cascade, three layers, one rule (system → ricing → app CSS, last match at equal priority, `doc/guide/en/styling/themes.md`). But R5/R6: three theme *sources* with two precedence implementations, and theme functions appending `dark_theme` twins after a type's own face, which last-match silently replaces. That is Azul's "which layer paints this?". | Partly. I1/I7 (one source, one trigger) and I8 (pair builder + lint) close it. |
| M6 | `CssMockEnvironment` lets one binary render as any (os, theme, viewport, language) without the OS. Reftest baseline exists (`doc/reftest_baseline.txt`) but nothing enumerates widget × theme × os × state; `layout/tests/theme_conditional_stylesheet.rs` pins one path. | The one thing Qt structurally cannot do and Azul can. Unused for widgets so far. |

## 3. The three planned steps against M1–M6

**Step 1, Flat/Flora with DOM recreation on switch.** Correct and cheap; it is Qt's
unpolish/polish with the whole tree rebuilt, so no per-widget state can go stale. A `UiTheme`
switch changes inline declarations, which `NodeDataFingerprint` hashes, so the DL cache is safe;
a pure `@theme` flip needed `cascade_epoch`, which landed (`5c1814036`). Rule to write down: a theme
function may only *append declarations*; it never branches on engine state at paint time.

**Step 2, the `@theme` rule/API.** Already declarative. What makes it robust are engine
invariants, not the rule syntax: I2 (context before the first cascade, flip = full restyle),
I3 (no black seeds), I4 (compact and slow readers agree; the two-reader split is Azul's version of
"two painters disagree"). I4 and most of I3 landed since 09-12.

**Step 3, native theme + `@os`.** This is where the Breeze class can re-enter, in data form.
`@os(linux:kde)` × `@theme dark` × pseudo-state × row position multiplies rows in a table, and
every row nobody has rendered is an `aba0f922` waiting to ship. Declarative does not shrink the
combinatorics; it makes each cell an entry you can *enumerate*, so enumerate them (4.3). Second
hazard: "match Breeze" is a moving target; Breeze itself changed its item views in 2026-02 and
broke Qt 5 apps doing so. Azul's KDE look will lag Breeze. That is M1's price, and it should be
stated in the docs so "doesn't look like Plasma 6.8" is not filed as a bug.

**Verdict.** The three steps remove M1, M4 and M5 by construction, and M6 becomes available. They
do not by themselves remove M2 and M3, which are engine invariants, and the Breeze bug is
literally an M3 instance. Three steps + I2 + the last step of I3 + a rendering matrix (4.3) is
sufficient. Three steps alone is "less likely", not "excluded".

## 4. Additions to the plan, ranked

1. **No unthemed default on any paint path** (I3, last step). Replace `unwrap_or(ColorU::BLACK)`
   (`getters.rs:2559`) and the `DEFAULT_TEXT_COLOR` fallback (`prop_cache.rs:2649`) with
   `debug_assert!` + themed lookup; widget light values move into the theme functions. Direct
   Breeze-class fix. Small now that `a52fdb33a` exists.
2. **Exhaustive-by-construction declarations** (I8). Extend
   `layout/tests/widget_lint_manifest_is_exhaustive.rs`: every conditional declaration
   (`dark_theme(X)`, `@os` twin, pseudo-state) in a `*_style` vector must be preceded by an
   unconditional `X`. Breeze's `Invalid` branch was "the case with no base value".
3. **Widget rendering matrix as reftests**, driven by `CssMockEnvironment`: widgets ×
   {Flat, Flora, Native} × {light, dark} × {macos, windows, linux:gnome, linux:kde} × {rest, hover,
   focus, active, disabled, selected}, headless, one DPI. Prune to the states a theme actually
   declares (the lint in 2 can emit the list). This is the guard Breeze lacked. Cost is baseline
   churn; per-widget crops and a small threshold keep it tolerable.
4. **Row context stays structural.** Rule for ListView, TreeView and any future grid: alternation,
   selection span and first/last rounding live on the row node or use `:nth-child` /
   `:first-child` / `:last-child`; widget code never passes a "position in row" flag into a theme
   function. The native theme declares alternating rows this way (`:nth-child(even)` on the row
   node, macOS `#FFFFFF` / `#F4F5F5`), which a rice file can then override at the same selector.
5. **Theme = data, enforced.** `layout/src/widgets/themes/` must not import display-list, GL or
   paint APIs (grep lint in `scripts/check.sh`); `UiTheme::Native` is a declaration generator like
   Flat and Flora, selected at DOM build time.
6. **Ricing allowlist (decision needed).** The user CSS file is Azul's only "system style
   plugin" analog. Restricting it to paint properties (colours, borders, radii, shadows, fonts)
   would guarantee a user theme can recolour but never break layout, stronger than QSS. It costs
   ricers `padding` and `display`. Not needed for the Breeze class; worth deciding while the native
   theme is designed.
7. **One source, one trigger** (I1/I7), already in the analysis doc; listed because R5 is the M5 analog.

## 5. What Azul cannot avoid

- Lagging the real platform look (M1's price). Document it.
- The combinatorics of step 3: testable, not removable.
- Third-party widget authors bypassing the theme functions (as `Button::dom` did until
  `faa2f6c6a`); the lint manifest is the only guard.

Open for Felix: (a) adopt items 1–5 as preconditions for step 3 in the theme ledger? (b) item 6, yes or no?
