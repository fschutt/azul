# B2 - azul-css CSS code generation (PROGRESS / checkpoint)

Branch `wt/b2-css-codegen` (cut from `fix/input-bugs-2026-09-19` @ 433423e57). Final report:
`scripts/B2_CSS_CODEGEN_2026_09_29.md`. No compiles in this worktree - the parent integrates.

## 1. AUDIT (2026-09-29, before any change)

Callers of `azul_css::codegen` today (grep `codegen::` outside `css/src/codegen`, `doc/src/codegen`):

| Where | What it uses | Status |
|---|---|---|
| `dll/.../debug_server/platform.rs:288-310` route `POST /debug/compile?lang=` | `codegen::backend_for` + `emit_project` -> zip | WIP: works for `rust` only, no UI caller (debugger.js "Export" uses the `export_code(_zip)` ops = `core/src/xml.rs`, not this route); 400 message hard-codes "rust, cpp, python" (`platform.rs:387`) |
| `core/src/xml.rs:30,5527,7475,7522,7711,7782` | `codegen::format::VecContents` (+ `insert_from_css_property`) | DEAD: xml.rs no longer emits the collected blocks (`xml.rs:5547-5551` "are no longer emitted"), styles are inlined as `.with_css("..")` strings |
| `core/src/xml.rs:7535`, `core/src/dom.rs:20`, `core/src/resources.rs:22`, `layout/src/solver3/layout_tree.rs:106`, `layout/src/solver3/display_list.rs:33`, `css/src/props/style/filter.rs:21` | `codegen::format::GetHash` | WORKS - but it is a runtime hashing helper, not codegen (blocks gating the module) |
| `azul-doc` (`doc/src`) | nothing from `azul_css::codegen` (its own `doc/src/codegen/v2` is the bindings generator) | n/a |

`css/src/codegen/*` and the `FormatAsRustCode` impls spread over 30 `css/src/props/**` files:

| Piece | Evidence | Status |
|---|---|---|
| module compiled unconditionally | `css/src/lib.rs:79` `pub mod codegen;`, no `codegen` feature in `css/Cargo.toml` | TO FIX (task) |
| `CodegenBackend` trait + `backend_for` | `codegen/mod.rs:28-49` | WORKS as dispatch; only 3 langs |
| `CppBackend` | `codegen/cpp.rs:25` returns `"// TODO: C++ codegen backend not yet implemented."` | PLACEHOLDER |
| `PythonBackend` | `codegen/python.rs:25` returns `"# TODO: Python codegen backend not yet implemented."` | PLACEHOLDER |
| `RustBackend::emit_css` = `css_to_rust_code` | `codegen/rust.rs:63-94` | BROKEN - output does not compile against today's API: `rules: [ .. ]` array for a `CssRuleBlockVec` (`rust.rs:67`), `declarations: [ .. ]` array (`:78`), `CssRuleBlock` literal misses the `conditions` field (`:69-83` vs `css.rs:869`), `Css` literal misses `keyframes` (`:66`, `css.rs:38`), `vec![..].into()` / `String::from(..)` inside a `const` (`:349`, `:362`, `:364`), `DynamicCssProperty.dynamic_id` printed with `{:?}` of an `AzString` (`:471`); `\r\n` line endings; `@media/@os/@theme` conditions and `@keyframes` silently dropped |
| `RustBackend::emit_project` | `codegen/rust.rs:36-58` | BROKEN: `azul = "0.0.7"` (`:45`), `const CSS` never used by an app (only prints a rule count) |
| `format_static_css_prop` (the only exhaustive CssProperty -> Rust formatter) | `props/property.rs:8046-8817` | WIP - exhaustive match (good) but Rust-only, emits azul-css-internal const helpers (`PixelValue::const_from_metric_fractional`, `FloatValue::const_new_fractional`) that do not exist in the `azul` crate's dynamic bindings (`target/codegen/dll_api_external.rs` has 106 `const_*` fns, all `CssProperty::const_*`) |
| `LayoutWidth/LayoutHeight::Calc` | `codegen/format.rs:520,533` `"LayoutWidth::Calc(/* {} items */)"` | PLACEHOLDER (uncompilable) |
| `ScrollbarInfo` formatter | `codegen/format.rs:947` prints `button:` twice, never `corner:` | BROKEN |
| radial colour stops | `codegen/format.rs:1091` emits `RadialColorStop {..}` (type is `NormalizedRadialColorStop`) | BROKEN |
| `Rotate3D` | `codegen/format.rs:1259` `StyleTransformRotate3D {{ {}, {}, {}, {} }}` - no field names | BROKEN |
| `StyleBackgroundContent::Image` | `codegen/format.rs:985` `Image({id:?})` - a `&str` literal for an `AzString` | BROKEN |
| `GridTemplate` | `codegen/format.rs:562` `GridTrackSizingVec::from_vec(vec![..])` in const context | BROKEN (non-const) |
| fractional values | `codegen/format.rs:301,339` keep 2 decimals (`* 100`) | WIP (lossy: 0.125 -> 0.13) |
| `VecContents` const-slice collector | `codegen/format.rs:58-270` | DEAD (see xml.rs row) |
| `CssShape::format_as_rust_code` (inherent) | `css/src/shape.rs:382`, test `:1055` documents `NaN_f32`/`inf_f32` output | WIP, not part of the trait |
| bindings-side knowledge | none: no backend knows the api.json/binding spellings (`create*` not `new`, `Az*_` C names, per-language casing, keyword escaping) | MISSING |
| tests | 2 smoke tests in `codegen/rust.rs:481-521` (string `contains`), none compile the output, none for other langs | MISSING |

Does the Rust backend's output compile against today's API? **No** (see the BROKEN rows). Real code
(`layout/src/widgets/themes/flat.rs:994-1300`, `button.rs`) builds styles as
`CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(PixelValue::const_px(3)))))`
- a flat `Vec<CssPropertyWithConditions>` with the pseudo-state / @os / @theme as `apply_if`
conditions, not a `Css` rule tree. The bindings (`api.json`) expose ~120 `CssProperty::<snake>(inner)`
constructors (Exact only), `CssProperty::{auto,none,initial,inherit}(CssPropertyType)`, and an
auto-generated variant constructor per non-generic enum (`Az<Enum>_<lowerCamelVariant>`), but NOT for
the 72 CssProperty variants without a manual constructor (Animation*, CaretWidth, Clip, Filter,
FlexBasis, Grid*, WhiteSpace, TextShadow, ...) - those need `CssProperty::<variant>(XValue::Exact(..))`,
i.e. a monomorphized `CssPropertyValue<T>` alias which has no C constructor functions.

## 2. DONE
(none yet)

## 3. IN PROGRESS
- audit (this section) -> commit

## 4. NEXT
1. `codegen` feature: gate `pub mod codegen` + every `FormatAsRustCode` impl; move `GetHash` to an
   always-compiled `azul_css::hash` (re-exported from `codegen::format`); enable the feature in
   core / layout / dll (debug-server, e2e-scripting) Cargo.toml.
2. IR (`codegen/ir.rs`) + exhaustive lowering (`codegen/lower*.rs`).
3. Golden tests (RED) then printers per language.

## 5. Open questions
- none yet
