# X1a: DOM export in 16 more printers (report, 2026-09-29)

Branch `wt/x1a-dom-export-printers` (from `0a326afe5`). **Nothing was compiled with cargo**
(house rule). The Ruby/Lua/PHP/Kotlin/C#/Node/Nim/D/Zig/Julia/OCaml/Haskell/Pascal printers
were written by four helper agents in this worktree from a shared brief and reviewed and
committed here. The OCaml / Haskell / Pascal helper also wrote their expected outputs by hand
and checked them with the real toolchains against the generated bindings (OCaml builds under
dune's dev profile and runs; Haskell typechecks with `ghc -fno-code -Wall`; Pascal links
against libazul and its registration ran). The Rust printer code itself is unchecked.

## 1. What was built

One generator, as before: `azul_core::xml` lowers markup to B3's IR, and every printer only
spells it. The 16 printers now print `Expr::Method` / `Param` / `Concat`, item parameters, the
component-library part and an app project, and `exports_dom()` is `true` for all 20
(rust, c, cpp, python + these 16).

Two ways to spell a DOM, chosen per binding (read from `doc/src/codegen/v2/lang_*` and
verified against `target/codegen`):

- **Wrapper layer** (the binding has idiomatic `Dom` / `SmallAriaInfo` classes with native
  strings, while its CSS values go through the raw C layer): the printer implements
  `dom::WrapperDomSyntax` on a small `XxxDom` struct and prints DOM items through
  `dom::WrapperDom(XxxDom)`. Literals come from the printer's own syntax; a raw struct /
  variant / Vec, or a class the DOM lowering does not build, is a limitation
  (`dom::wrapper_dom_limitation`), never wrong code. A module without DOM items prints
  byte-identically to before. Used by Java, Kotlin, C#, Go, Node, Ruby, Lua, OCaml.
- **The printer's own syntax** implements `dom_limitation -> None`, `method`, `param` and
  `concat`: native-API printers (Swift, D; a static `create_x` is now `x`, the generators'
  rule), and raw-layer printers that nest C calls like C++ (Julia, Zig, PHP) or chain the
  binding's by-value procs (Nim). Haskell (everything is `IO`) and Pascal (a statement
  printer) have their own DOM walkers over the IR.

### Per language

| Language | DOM (the card) | Component library | App project |
|---|---|---|---|
| Java | `Dom.createDiv().withCss("..").withChild(..)`, class `AzulUi` | note | pom.xml, AzulUi.java, AppMain.java (`App.create(new AppData(), AppMain::layout)`) |
| Kotlin | `Dom.createDiv()..`, `"by ${author}"`, default params | note | build.gradle.kts (+ runnable jar), settings, Ui.kt, Main.kt |
| C# | `Dom.CreateDiv().WithCss(..)`, `namespace AzulUi` | note | AzulApp.csproj, Ui.cs, Program.cs |
| Go | `azul.DomCreateDiv().` / `WithCss(azul.Str(..)).` (dot at line end) | note | go.mod, ui/ui.go, main.go (`azul.Bind(layout)`) |
| Swift | `Dom.div().withCss(..)`, `Dom.a(href, text:, aria:)`, labelled defaults | note | Package.swift, UI.swift, main.swift (title set) |
| Node | `azul.Dom.createDiv().withCss("..")`, default params | note | package.json, ui.js, main.js (title set) |
| Ruby | `Azul::Dom.create_div.with_css("..")`, `"by #{author}"` | note | Gemfile, ui.rb, main.rb (title set) |
| PHP | raw FFI nested like C++ (`$L->AzDom_withChild(..)`) | note | **limited**: ui.php + main.php that builds the content and says on STDERR why no window opens (php-ffi cannot make a C callback; the Zend extension has no create_a / SmallAriaInfo) |
| Lua | `azul.Dom.create_div():with_css("..")`, `..` | note | ui.lua, main.lua (title set) |
| Zig | nested `C.AzDom_*` (wrapper methods take `*Self`, no chaining on temporaries), `azConcat` | **registers** (C registration transliterated, `callconv(.c)`) | build.zig, ui.zig, main.zig (title set) |
| Nim | `AzDom_createDiv().` / `withCss(azStr(..)).` (dot at line end) | note | ui.nim, main.nim (title set) |
| D | `Dom.div().withCss(..)`, `"by " ~ author`, `module ui` | note | dub.json, source/ui.d, source/main.d (title set) |
| OCaml | `Dom.create_div () |> Fun.flip Dom.with_css ".."`, optional labelled args | note | dune-project, dune, ui.ml, main.ml |
| Haskell | `do` block binding IO children, then `Dom.createDiv >>= Dom.withCss ..` | note | azul-app.cabal, cabal.project, Ui.hs, Main.hs |
| Julia | nested `Azul.AzDom_withChild(..)`, `Azul.az_string("by $(author)")` | note | ui.jl, main.jl (hello-world pattern, title set) |
| Pascal | `TDom.Div_.WithCss('..').WithChild(TDom.H2(Title))` | **registers** (`TDom.Release`, `cdecl` render fn) | ui.pas, main.pas (`TAzApp<TAppData>`, title set) |

"note" = `dom::registration_note(lib, REASON)` with a binding-specific reason: none of these
bindings can build a `ComponentDef` whose raw `render_fn` returns
`ResultStyledDomRenderDomError` by value and hands it the Dom the render function built
(JNA / P/Invoke host invokers return through out-pointers and have no ComponentRenderFn kind;
purego callbacks return only ints/pointers/bools; koffi and the Lua cdef declare the field
`void*`; ruby-ffi's typedef returns `:pointer`; php-ffi has no callbacks; Swift and D wrappers
hand out no raw value; Julia unions are opaque blobs; Nim has no union tag constants; OCaml's
render fn is a bare pointer without a host-handle route; GHC's `foreign import "wrapper"`
cannot return a struct by value).

### Coordinator addition: Julia union variants by field name (F1's `_pad0`)

`az_union(U, V, tag, payload...)` builds the variant struct `V` from its field names (`tag`,
`payload`, zeroes for any other field, i.e. azul.jl's `_pad0::NTuple{N,UInt8}`) and stores it
into the union blob. The printer needs no knowledge of which variants are padded, so the css
crate stays independent of `c_layout`. RED: `codegen_structure::julia_builds_a_union_variant_by_field_name_so_the_c_padding_cannot_shift_its_payload`.

## 2. Commits (oldest first)

| hash | what |
|---|---|
| eef8f44c8 | test: RED Julia union variant by field name |
| d40e51839 | fix: Julia `az_union` by field name |
| 07ae46413 | test: RED flag list of 20 + every DOM export balanced; hand-written `dom_card` for java, go, swift |
| 4f0b15494 | feat: shared `dom.rs` / `mod.rs` helpers; Java |
| 240e0ebc8 | feat: Go, Swift |
| 7ce216756 | docs: progress |
| 2a7c082f2 | test(builder): export tests follow `exports_dom()` (they assumed Java had no DOM export); README line |
| 6d5b10881 | feat: Kotlin, C#, Node, Ruby, Lua, PHP; `lang::unicode_utf16` |
| 78d3305af | docs: progress |
| b6e3af7a0 | feat: D, Julia, Nim, Zig; `dom::chained_dot_at_line_end` |
| 7f33c8a14 | feat: OCaml, Haskell, Pascal; `dom::chained_infix` |
| (this) | docs: report |

## 3. Files outside the 16 printers (all minimal)

- `css/src/codegen/lang/dom.rs` (appended, one section): `is_dom_item`, `DOM_CLASSES`,
  `WrapperDomSyntax`, `WrapperDom`, `wrapper_dom_limitation`, `chained_dot_at_line_end`
  (Go + Nim), `chained_infix` (OCaml + Haskell), `one_line`, `registration_note`.
- `css/src/codegen/lang/mod.rs`: `call_param_names` gains the multi-arg `Dom` constructors
  (Swift labels; X1b's Smalltalk can use them), `item_dom_blocker` uses `dom::is_dom_item`,
  new `unicode_utf16` (Java and Kotlin have no `\U` escape; replaces Kotlin's local copy).
- `css/tests/codegen_structure.rs`: the DOM list (20 names), `dom_outputs()` +
  `every_dom_export_has_balanced_brackets`, the Julia test.
- `css/tests/codegen_goldens/{java,go,swift}/dom_card.*` hand-written; stale `dom_app` files
  of all 16 deleted (the printers write other paths now).
- `layout/src/e2e/export.rs` + `export_tests.rs`: B3's tests pinned the DOM list to
  rust/c/cpp/python and used Java as THE printer without DOM export. They now compare with
  each backend's `exports_dom()`, pick any printer without DOM export for the "says why"
  check (nothing to check once none is left), and assert Java's card / component output. The
  project README no longer says a DOM-exporting printer "does not write a runnable app yet".

Duplication found and removed on the way: Go's and Nim's dot-at-line-end chain (one copy in
`dom.rs`), OCaml's infix chain that Haskell imported from `ocaml.rs` (moved to `dom.rs`), and
Kotlin's UTF-16 escape (now `lang::unicode_utf16`).

## 4. API changes

- **api.json: none.**
- azul-css (`codegen` feature), public: `lang::dom::{is_dom_item, DOM_CLASSES,
  WrapperDomSyntax, WrapperDom, wrapper_dom_limitation, chained_dot_at_line_end,
  chained_infix, one_line, registration_note}`, `lang::unicode_utf16`; `exports_dom()` is
  `true` for the 16.

## 5. Least sure to compile (Rust)

1. `dom.rs` `impl<W: WrapperDomSyntax> ExprSyntax for WrapperDom<W>` (every trait signature
   copied from `ExprSyntax`), and `wrapper_dom_limitation`'s or-patterns binding
   `class` / `method` / `ty` across variants.
2. `haskell.rs` `dom_classes` (`class == c` with `c: &&str`), `HsDom::value` recursion.
3. `pascal.rs` `pas_ctor` (`let .. else`), the `call` / `local` closures in `registration()`,
   `LinearSyntax::int(&Pascal, ..)`.
4. `zig.rs` `camel()` (`Ident { words }`), the registration `write!`s mixing inline captures
   and positional arguments; the same mix in several `app_main` `format!`s (Node, PHP, Julia,
   Nim raw-string templates with `{{ }}`).
5. `layout/src/e2e/export.rs` tests: `let Some(lang) = all_backends().iter().find(..).map(..)
   else { return; };` and `.is_some_and(Vec::is_empty)` in `export_tests.rs`.

Least sure in the target languages (not compiled here): Zig's registration and `pub fn main()
void` on the newest Zig; Nim's dot-at-line-end continuation (checked against the parser
source); Kotlin's `App.create(AppData(), ::layout)` inference; D's `dub.json`
(`lflags-posix`, path dependency); the Julia / Nim apps (hello-world patterns). Pascal's
registration reads the data model with hard-coded union tags (Some = 1, String = 1), tested
at runtime against today's bindings; Nim declines for exactly that reason.

## 6. Commands for the parent

```sh
# review the new files, then bless (dom_card / dom_library / dom_app for the 16, and the
# Julia CSS goldens whose hand-built unions changed spelling):
AZ_BLESS=1 cargo test -p azul-css --features codegen,parser --test codegen_goldens
cargo test -p azul-css --features codegen,parser --test codegen_goldens
cargo test -p azul-css --features codegen,parser --test codegen_structure
# the builder tests that followed the flag:
cargo test -p azul-layout --features e2e-server --lib e2e::export::tests
cargo test -p azul-layout --features e2e-server --lib export_tests
```

After blessing, `git status` should show no new file for Java/Go/Swift `dom_card` (hand-written
anchors) and no CSS golden change except Julia's (`keywords`, `families`, `paint`, `basic`,
`project/styles.jl` and the helper text in every Julia golden). Any other CSS golden diff is a
bug in this branch.

## 7. What is left / for the parent

- **Merge with X1b:** both branches edit the DOM list in `codegen_structure.rs`
  (`exports_dom_is_true_exactly_for_the_printers_that_build_the_dom`) and may touch
  `call_param_names` / `dom.rs`. The builder tests no longer need a list.
- `layout/src/e2e/export.rs` writes component libraries to `components/<lib>.<ext>`: fine for
  scripts, but Java needs `com/azul/<Class>.java` (a public class per file), C# / Go / Kotlin /
  Haskell / D / Pascal have module or package names inside the file (`AzulUi`, `package ui`,
  `module Ui`, `module ui`, `unit Ui`), so two libraries of one app collide. A
  `CodegenBackend::library_path(lib)` (and a per-printer build-steps string for the README)
  would put this in the printers.
- Pre-existing CSS printer bugs noticed, not fixed (outside DOM): Haskell's CSS `string()` uses
  `\U..` (Haskell has no such escape; the DOM uses `\x..\&`); the OCaml CSS project's `dune`
  comment says to copy the bindings WITH their `dune-project`, which hides the `azul` library;
  PHP's CSS `azul_str` has no `function_exists` guard, so a CSS module included after a DOM
  module redeclares it.
- Window titles are not set by the Java, Kotlin, C#, Go, OCaml and Haskell apps (their wrapper
  layers have no setter; only the raw `window_state.title`); the title is in the header comment.
