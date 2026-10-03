# B2 - azul-css CSS code generation for every binding language (2026-09-29)

Branch `wt/b2-css-codegen` (cut from `fix/input-bugs-2026-09-19` @ 433423e57), 14 commits, nothing
compiled here (house rule): the parent integrates, compiles once and runs the suites.
Checkpoint file: `scripts/B2_CSS_CODEGEN.PROGRESS.md`.

## 1. Result in one paragraph

`azul_css::codegen` is now behind a `codegen` cargo feature. It lowers parsed CSS once into a
language-neutral construction IR - through an exhaustive, generated `match` over every
`CssProperty` variant - and prints that IR in all 35 languages azul has bindings for, in the
spellings of each language's generated bindings (`target/codegen`, `doc/src/codegen/v2/lang_*`).
Each language produces a snippet (`emit_css` / `emit_stylesheet` / `emit_module`) and a
standalone project (`emit_project`: build file + `main`). The 20 main languages build every value
the bindings can express. The other 15 do the same where their binding allows, and report
precisely what their binding cannot express: 5 of those bindings are themselves incomplete or
broken. Golden tests (one hand-written `basic` golden per language, the rest to be blessed) and
structural tests cover the output.

## 2. Audit (before any change)

Callers of `azul_css::codegen` (grep `codegen::` outside `css/src/codegen`, `doc/src/codegen`):

| Where | What it uses | Status |
|---|---|---|
| `dll/.../debug_server/platform.rs:288-310`, route `POST /debug/compile?lang=` | `codegen::backend_for` + `emit_project`, returned as a zip | WIP. Worked for `rust` only; no UI calls it (debugger.js "Export" uses `core/src/xml.rs`); the 400 message hard-codes "rust, cpp, python" (`platform.rs:390`) |
| `core/src/xml.rs:30,5527,7475,7522,7711,7782` | `codegen::format::VecContents` (+ `insert_from_css_property`) | DEAD: xml.rs no longer emits the collected blocks (`xml.rs:5547-5551`); styles are inlined as `.with_css("..")` |
| `core/src/xml.rs:7535`, `core/src/dom.rs:20`, `core/src/resources.rs:22`, `layout/src/solver3/layout_tree.rs:106`, `layout/src/solver3/display_list.rs:33`, `css/src/props/style/filter.rs:21` | `codegen::format::GetHash` | WORKS, but it is a runtime hashing helper, not codegen, so it blocked gating the module |

`css/src/codegen/*` and the `FormatAsRustCode` impls in about 30 `css/src/props/**` files:

| Piece | Evidence | Status |
|---|---|---|
| module compiled unconditionally | `css/src/lib.rs:79`, no feature | FIXED: `codegen` feature |
| `CppBackend`, `PythonBackend` | `cpp.rs:25`, `python.rs:25`: `"// TODO: ... not yet implemented."` | PLACEHOLDER, replaced |
| `RustBackend::emit_css` | `rust.rs:63-94`: array literals for Vec types, `CssRuleBlock` without `conditions`, `Css` without `keyframes`, `vec![..].into()` inside a `const`, `{:?}` of an `AzString`, `\r\n` line endings, `@media/@os/@theme/@keyframes` silently dropped | BROKEN (did not compile), replaced |
| `RustBackend::emit_project` | `azul = "0.0.7"`, `const CSS` never used | BROKEN, replaced |
| `format_static_css_prop` (`property.rs:8046-8817`) | exhaustive but Rust-only, uses azul-css-internal `const_*` helpers that the `azul` crate does not export | WIP, legacy, gated |
| `LayoutWidth/Height::Calc` formatter | `format.rs:520,533` prints `Calc(/* {} items */)` | PLACEHOLDER (legacy, gated) |
| `ScrollbarInfo` / radial stops / `Rotate3D` / `Image` / `GridTemplate` formatters | `format.rs:947,1091,1259,985,562`: `button` printed twice, wrong type name, no field names, `&str` for `AzString`, non-const call in a const | BROKEN (legacy, gated) |
| fractional values | `format.rs:301,339` keep 2 decimals | lossy (0.125 -> 0.13); the new lowering is exact (section 4) |
| knowledge of the bindings | none of the old backends knew api.json / binding spellings | MISSING, now the core of the work |
| tests | 2 `contains` smoke tests, output never compiled, other languages untested | MISSING, now goldens + structural tests |

## 3. What changed

- `css/Cargo.toml`: `codegen = []`. Test targets `codegen_goldens` and `codegen_structure` have
  `required-features = ["codegen", "parser"]`. `exclude` keeps tests/tools out of the package.
- `css/src/lib.rs`: `#[cfg(feature = "codegen")] pub mod codegen;`, plus a new ungated
  `pub mod hash;` (`GetHash`, moved out of `codegen::format`; `format` re-exports it).
- The legacy `FormatAsRustCode` impls in `css/src/props/**` and `macros.rs` are gated with
  `#[cfg(feature = "codegen")]`, as are their tests and `format_static_css_prop`.
- Explicit opt-in by every existing user: `core/Cargo.toml` and `layout/Cargo.toml` set azul-css
  features to `["parser", "codegen"]` (xml.rs still uses `VecContents` / `GetHash` through
  `codegen::format`). In `dll/Cargo.toml`, `debug-server` and `e2e-scripting` add
  `"azul-css?/codegen"`.
- The old `codegen/{rust,cpp,python}.rs` are removed. New modules:
  - `codegen/{ir,doc,lower,lower_types}.rs`
  - `codegen/lang/*.rs`: 35 printers, plus `lang/mod.rs` (shared expression walker) and
    `lang/linear.rs` (statement flattening)
  - generator: `css/tools/gen_codegen_lowering.py`
- API (`codegen/mod.rs`): trait `CodegenBackend` with
  - `lang`, `aliases`, `display_name`, `extension`
  - `emit_module(&ir::Module)`, `emit_project_files`
  - provided: `emit_css` (flat styles), `emit_stylesheet` (the exact `Css` value), `emit_project`
  - free functions: `backend_for(lang)`, `all_backends()`, `supported_languages()`.

## 4. IR design

- `ir::Expr` is a construction tree. Every node names the api.json type it builds:
  - `Int{value: i128, ty: Prim}`
  - `Float{text, ty}` (canonical decimal text, or `nan` / `inf` / `-inf`)
  - `Bool`
  - `Str` (an `AzString`)
  - `Call{class, method, args}`: an api.json constructor, snake_case method
  - `Variant{ty, shape, variant, args}`
  - `Struct{ty, fields}` (declaration order; the generator checks api.json order == Rust order)
  - `Vec{ty, elem, items}`
  - `Unsupported{what}`
- `EnumShape` decides how a variant is built:
  - `CLike`: a C enum constant.
  - `Tagged`: the C variant constructor `Az<Enum>_<lowerFirst(Variant)>`; reserved names get a
    `Variant` suffix, mirroring `ir_builder.rs`.
  - `TaggedShadowed`: a `CssProperty` variant whose constructor name is taken by a hand-written
    api.json function (`AzCssProperty_width` takes a `LayoutWidth`), so it is built by hand.
  - `Generic{base: "CssPropertyValue", arg}`: the monomorphized `XValue` aliases, which have no
    C constructor functions. Tags: Auto=0, None=1, Initial=2, Inherit=3, Revert=4, Unset=5,
    Exact=6.
- Lowering (`lower.rs`, and `lower_types.rs`, which is generated with 192 `CssProperty` arms and
  no `_` arm, plus 283 `impl Lower`):
  - A property that has an api.json constructor becomes `CssProperty::<ctor>(inner)`.
  - Keywords become `CssProperty::{auto,none,initial,inherit}(CssPropertyType::X)`.
  - The 72 properties without a constructor become `CssProperty::<Variant>(XValue::Exact(..))`.
  - `revert` / `unset` become the variant over the alias.
  - Floats use the sugar the bindings expose: `PixelValue::px/em/pt/percent/..`,
    `FloatValue::create`.
  - Fixed-point values print the shortest decimal that round-trips `FloatValue::new`
    (`0.7px` stays `0.7`).
- Styles (`lower_styles`) merge `.btn`, `.btn:hover` and `@media/@os/@theme/@lang` rules into one
  `Vec<CssPropertyWithConditions>`. They use the idiomatic constructors (`simple`, `on_hover`,
  `on_active`, `on_focus`, `when_disabled`, `dark_theme`, `light_theme`, `on_windows/macos/linux`,
  `on_os`, `with_condition(s)`).
- `lower_stylesheet` rebuilds the exact `Css` value.
- `env()` survives as `CssDeclaration::Dynamic`; a flat style keeps its fallback and adds a note.
- Not expressible (`Unsupported`, with the reason printed as a comment):
  - `text-shadow`: `BoxOrStatic` payload, no constructor
  - grid `minmax()`: `GridMinMax` holds raw pointers
  - `FontRef`
- Droppable lists (`CssPropertyWithConditionsVec`, `CssDeclarationVec`, `CssPropertyVec`,
  `CssRuleBlockVec`) drop an item they cannot build and leave a note. In any other Vec, one bad
  item makes the whole value inexpressible.
- Per-language limitations use the same mechanism (`ExprSyntax::limitation`,
  `LinearSyntax::limitation`).
- Layout (`doc.rs`) does not depend on line width, so goldens are reviewable. A list breaks to one
  item per line exactly when it holds a Vec with 2 or more kept items; a style's top-level list
  always breaks.
- Printers come in two families:
  - `lang/mod.rs::ExprSyntax`: expression languages. It has an optional `field_value` hook, which
    Crystal and Perl use.
  - `lang/linear.rs::LinearSyntax`: statement languages. The value is flattened into temporaries
    `t1..`, children first. Optional `*_expr` hooks keep calls and aggregates inline where the
    language can nest them. COBOL also gets data-item hooks: a float `CALL` argument, a string's
    bytes, and the `usize` count of a Vec.

## 5. Per-language status

"Full" means every value the IR can express is printed. Dropped items always carry a comment with
the reason. Only `basic` goldens are hand-written; everything else must be blessed (section 7).

| Language | Form | Status | Notes / limitation |
|---|---|---|---|
| Rust | `azul::css::*` API, `XVec::from(vec![..])` | full | project: Cargo.toml + `.cargo/config.toml` + src/ |
| C | C API, compound literals, `static` fns | full | hand-built unions: X3 (below) |
| C++ | C API, aggregates, lambdas for unions | full | Makefile |
| Python | extension module, list-returning styles | limited | the Python API cannot build a Vec of 2+ items, a String-payload variant, or a keyword-named field (so a style is a Python list) |
| C# | `NativeMethods`, object initialisers, `AzulCodegen.Vec/Str` | full | csproj |
| Java | JNA `AzulNative<Module>`, `AzulCodegen.with/vec/str` | full | pom.xml; JNA nested union semantics unverified |
| Kotlin | JNA, `apply {}` blocks | full | gradle |
| Go | purego, C calls + Go-side builders | full | builders are hand-built: X3 |
| Swift | native API with labels | full | Package.swift |
| Node | koffi `azul.__lib`, plain objects, `new azul.<Ty>()` | full | package.json |
| Ruby | FFI `N.az_*`, `AzulCodegen.struct/union/vec` | full | Gemfile |
| PHP | FFI `$L->Az*`, `azul_struct/union/vec/str` | full | needs `php -d ffi.enable=1` |
| Lua | LuaJIT `azul.C`, `ffi.new` | full | |
| Zig | `@cImport` `C.` literals | full | build.zig |
| Nim | native API, `azVec` template, `azStr` | full | |
| D | native API | full | |
| OCaml | ctypes, `az_struct/az_union/az_payload` | limited | a `TaggedShadowed` CssProperty (revert/unset of a property with a ctor): the bindings only expose the api constructor |
| Haskell | pure `Azul.Types` ADTs, `IO [..]` style lists | limited | no pure constructor for non-droppable Vecs, Strings, or calls other than `CssPropertyWithConditions.*` |
| Julia | ccall API, `az_vec/az_union` | full | |
| Pascal | value layer `AzXxx(..)`, statements, `FillChar` + `Tag` + `Payload_X` | full | first `linear` printer |
| Ada | `Az_<Type>_<Method>`, qualified aggregates, `aliased array` + `'Address` | full | spec + body in one file (`gnatchop`); GNAT passes Convention-C records to imported functions BY REFERENCE (RM B.3), while the C functions take them by value: a binding issue, see follow-ups |
| ALGOL 68 | `az css property width (..)`, `AZCOLORU (REPR 255, ..)`, rows | limited | no MODE for `XValue` aliases, so every CssPropertyValue union is dropped; the binding itself uses undeclared MODEs, and a68g support for `ALIEN` is unverified: binding likely does not compile |
| COBOL | one subprogram per style, `CALL "Az..Byref" USING BY REFERENCE ..` | limited | `XValue` / hand-built CssProperty unions are dropped (the copybook has only `TAG` + `PAYLOAD-ANCHOR`); the copybook's TYPEDEF records are packed (no `SYNC`), so a record with C padding is smaller than the C struct: binding issue |
| Crystal | `Azul::X.method(..)`, `Azul::XValue::Exact.new(..)`, lib structs via `__own(LibAzul::AzX.new(..))` | full | styles return `Array(Azul::CssPropertyWithConditions)`; uses the nodoc `__own`/`__take`; variant classes are hand-built: X3 |
| Fortran | `az_css_property_width(..)`, keyword structure constructors, `target` array + `c_loc` | limited | tagged unions are opaque blobs, so `XValue` / hand-built CssProperty values are dropped |
| FreeBASIC | C names, `Type<AzX>(..)`, `Dim` array + `@t(0)` | limited | azul.bi declares no `XValue` types (their unions do not compile): binding likely broken |
| Common Lisp | CFFI `azul-internal::%az-*`, plists, keywords, `css-vec` | limited | CFFI cannot build a union from Lisp data, so `XValue` / hand-built CssProperty values are dropped; union by-value marshalling in cffi-libffi is unverified |
| Odin | C names in `azul.`, compound literals, `&[]T{..}[0]` | full | `#raw_union` literal with one member: X3 |
| Perl | FFI::Platypus `Azul::FFI::Az*`, Records | limited | union records have a fake layout, so unions are not built by hand and no Vec of 2+ items is built (styles return an array ref, like Python) |
| PowerShell | `[Azul.NativeMethods]::Az*`, hashtable casts, `New-CssVec` | full | the C# layer compiled by Add-Type; hashtable-to-struct casts rely on PS field adaptation |
| Racket | kebab wrappers, `make-AzX`, `css-union` / `css-vec` | full | variant struct written into union memory: X3 |
| Red/System | - | not expressible | every tagged union is an 8-byte opaque placeholder, so no CSS property can cross the FFI; each style is reported with the reason |
| Smalltalk | `AzulNative azCssPropertyWidth: ..`, cascades, `FFIExternalArray` | limited | no classes for `XValue` aliases, so those unions are dropped |
| V | `C.Az*`, struct/union literals, `mut t := [n]T{}` | full | the binding uses uppercase enum and union field names, which V may reject: binding issue |
| VB6 | - | not expressible | a `Declare` cannot pass or return a UDT by value; the binding skips every CSS constructor and does not declare the `..Byref` twins |

### X3 bug in the bindings (please forward)

The bindings lay out tagged unions differently from Rust:

- **azul.h:** declares one struct per variant, `typedef struct { uint8_t tag; Payload payload; }`,
  so the payload sits at the payload's own alignment.
- **Rust:** `repr(C, u8)` places the payload at the maximum alignment of all variants.
- **Example:** `AzStyleBackgroundContentVariant_Color {uint8_t tag; AzColorU payload;}` puts the
  payload at offset 1 in C, but Rust puts it at offset 8.

**Affected:**

- every C reader
- Go's Go-side builders, which are used for every variant
- Pascal variant records
- koffi unions
- Crystal variant classes
- Odin and V raw unions

**How the printers avoid it:** they use the Rust-implemented C constructors for `Tagged` variants,
which are safe. They hand-build a union only for:

- `Generic` aliases: a single payload variant, so the layouts agree
- `TaggedShadowed` CssProperty variants: these are affected unless the payload is 8-aligned. They
  only occur for `revert` / `unset` of a property that has a constructor.

## 6. Commits

| Commit | What |
|---|---|
| 61568f4c1 | docs(b2): audit + progress checkpoint |
| aa0224900 | feat(css): `codegen` feature, `hash` module, core/layout/dll wiring |
| c20a016c0 | test(css): RED - golden harness + shared cases, rust + c `basic` goldens |
| 6c4ef68d7 | feat(css): IR + exhaustive lowering + layout + Rust and C printers + structural tests |
| 27532d02a | feat(css): C++, Python, C#, Java, Kotlin printers + goldens (not split RED/GREEN) |
| 6884243a4 / d4cf2f6c5 | RED goldens / printers: Go, Swift, Node, Ruby, PHP, Lua |
| 37f06be47 | checkpoint |
| 5d28f453b / e8e82a719 | RED goldens / printers: Zig, Nim, D, OCaml, Haskell, Julia, Pascal |
| 06d0baf92 | checkpoint |
| 5d72c7ce0 / 120ebd8f7 | RED goldens / printers: Ada, ALGOL 68, COBOL, Crystal, Fortran, FreeBASIC, Lisp, Odin, Perl, PowerShell, Racket, Red, Smalltalk, V, VB6 |
| 447182b0b | checkpoint |
| (this file) | docs(b2): final report |

## 7. How to verify

Rust side:

```sh
cargo test -p azul-css --features codegen,parser --lib codegen          # unit tests (doc layout, fixed point)
cargo test -p azul-css --features codegen,parser --test codegen_structure
cargo test -p azul-css --features codegen,parser --test codegen_goldens  # FAILS until blessed
AZ_BLESS=1 cargo test -p azul-css --features codegen,parser --test codegen_goldens
git diff css/tests/codegen_goldens   # review, then commit the blessed files
cargo check -p azul-core -p azul-layout; cargo check -p azul-dll --features debug-server
cargo check -p azul-css --no-default-features --features parser   # codegen really optional
```

`every_c_abi_name_exists_in_azul_h` checks every `Az*` name in the C output against azul.h. It
reads `AZ_CODEGEN_DIR` (default `../target/codegen`) and skips when azul.h is missing.

- **Blessing:** after blessing, compare each language's blessed `basic` with the hand-written
  golden it replaced. A missing golden fails the test (it is never skipped). The hand-written
  `basic` goldens are the RED spec. Everything else (families, conditions, env, paint, widget,
  stylesheet, keywords, and the project files outside rust/c) must be reviewed and blessed.
- **Generated code per language:**
  - Take `backend.emit_project(..)` of the BASIC case, or the blessed `project/` directory.
  - Copy the named binding file(s) from `target/codegen` and libazul next to it.
  - Build it with the command printed in each project's header.

| Lang | Build / run |
|---|---|
| Rust | `cargo run` |
| C | `make AZUL_INCLUDE=. AZUL_LIB=. && ./app` |
| C++ | `make && ./app` |
| Python | build `azul-dll --features python-extension`, copy as `azul.so`, `python3 main.py` |
| C# | copy `Azul.cs` next to the SDK-style csproj, `dotnet run` |
| Java | `mvn -q package && java -Djna.library.path=. -cp target/azul-styles-1.0.0.jar:<jna.jar> com.azul.StylesMain` |
| Kotlin | `gradle build && java -Djna.library.path=. -jar build/libs/azul-styles.jar` |
| Go | copy target/codegen/go to ./azul-go (go.mod `replace`s it), `go run .` |
| Swift | `swift build -Xlinker -L. && ./.build/debug/AzulStyles` |
| Node | copy target/codegen/node to ./azul-node, `npm install && node main.js` |
| Ruby | `ruby -I. main.rb` |
| PHP | `php -d ffi.enable=1 main.php` |
| Lua | `luajit main.lua` |
| Zig | `zig build run` |
| Nim | `nim c -d:release -r main.nim` |
| D | `ldc2 main.d styles.d azul.d -L-L. -L-lazul && ./main` |
| OCaml | `dune exec ./main.exe` |
| Haskell | `cabal run azul-styles --extra-lib-dirs=.` |
| Julia | `AZUL_LIB=$PWD/libazul.so julia main.jl` |
| Pascal | `fpc -Mdelphi -Fl. -k-L. -k-lazul main.pas && ./main` |
| Ada | `gprbuild -P azul_styles.gpr && ./obj/main` |
| ALGOL 68 | `a68g main.a68` |
| COBOL | `cobc -x -free main.cob styles.cob -L. -lazul -o main && ./main` |
| Crystal | `shards install && crystal run main.cr --link-flags=-L.` |
| Fortran | build the azul modules (`make` in target/codegen/fortran), then `gfortran -ffree-line-length-none styles.f90 main.f90 azul*.o -L. -lazul -o main` |
| FreeBASIC | `fbc main.bas -p . -l azul && ./main` |
| Common Lisp | `sbcl --eval '(asdf:load-system :azul-styles)' --eval '(azul-styles::main)' --quit` |
| Odin | `odin run . -extra-linker-flags:"-L."` |
| Perl | `cpanm --installdeps . && perl main.pl` |
| PowerShell | `pwsh ./main.ps1` |
| Racket | `AZ_LIB_DIR=. racket main.rkt` |
| Red/System | `redc -r main.red` (prints the count of expressible styles: 0) |
| Smalltalk | load Azul.st + AzulStyles.st into Pharo, evaluate main.st |
| V | `v run .` |
| VB6 | Standard EXE with Azul.bas, AzulStyles.bas, Main.bas |

## 8. Spots I am least sure compile

Nothing here was compiled. The Rust spots to check first:

- **`lower_types.rs` (generated):**
  - every field and variant path of 283 `impl Lower` blocks. The generator cross-checks api.json
    against the Rust sources, but private or renamed fields would only show at compile time.
  - `impl<T: Lower> Lower for BoxOrStatic<T>` (via `as_ref()`)
  - the `binary_search` closures in `api_module` / `css_property_variant_of_ctor`
- **`lang::all()`:** `alloc::vec![Box::new(..), ..]` coercing to `Vec<Box<dyn CodegenBackend>>`
  (35 entries). The first element's type drives inference, so a `: Vec<Box<dyn ..>>` annotation
  may be needed.
- **Format strings:** captures of consts (`{HEADER}` in ada.rs, `{AZUL_VERSION}` in the project
  emitters, `{POINTER_LITERAL}` in cobol.rs), and mixed named and positional arguments (racket.rs
  main, cobol.rs main).
- **Closure and fn-pointer coercions:**
  - `chars.peek().is_some_and(char::is_ascii_uppercase)` (ada.rs)
  - `next.is_some_and(char::is_lowercase)` (cobol.rs)
  - `module_any(m, &is_union)` (racket.rs): a fn item passed as `&dyn Fn`
- **Trait-object calls:** `lang/linear.rs` calls default trait methods through `&dyn LinearSyntax`,
  e.g. `string_from_buffer` calling `self.string`.
- **The legacy `cfg(feature = "codegen")` gating** of about 25 props files:
  - a gated import that is still used ungated, or the reverse, shows up as a hard error
  - check `--no-default-features --features parser` separately

Generated-code spots, per language, most uncertain first:

- **Odin:** `&[]T{..}[0]`
- **V:** the binding's enum and union naming
- **Crystal:** `__own`/`__take` are public but marked nodoc
- **PowerShell:** hashtable casts to structs with fields
- **Java:** JNA nested unions
- **OCaml:** `Ctypes.Ptrdiff.of_int`
- **Nim:** the block template
- **Swift and D:** the camel rules for acronyms
- **Pascal:** variant record layout (X3)
- **Ada:** the `Azul.Float` expanded name for literals hidden by `Standard`
- **COBOL:** `BY VALUE` of a `FLOAT-SHORT` item
- **Smalltalk:** Tonel comment placement

## 9. Follow-ups (not done here, outside my paths or scope)

1. `dll/.../debug_server/platform.rs:390`: replace "Supported: rust, cpp, python." with
   `azul_css::codegen::supported_languages()`. The route now serves all 35 languages. This is a
   one-line change, but the house rule allows only Cargo.toml wiring in dll/.
2. Switch the four `codegen::format::GetHash` imports (core xml.rs/dom.rs/resources.rs,
   layout_tree.rs/display_list.rs) to `azul_css::hash::GetHash`. Then drop `codegen` from
   layout's azul-css features. Core keeps it while xml.rs uses `VecContents`.
3. Migrate `core/src/xml.rs` off the legacy `FormatAsRustCode` / `VecContents` path, which is dead
   code today. Then delete `codegen/format.rs` and the gated impls in props.
4. Bindings generator (doc/src/codegen/v2), largest wins first:
   - X3 union layout in C / Go / Pascal / koffi / Crystal / Odin / V.
   - `XValue` types for ALGOL 68 / FreeBASIC / Smalltalk.
   - Real union layouts for Red/System and Perl.
   - `..Byref` declares for VB6.
   - `SYNC` / padded records for COBOL.
   - `C_Pass_By_Copy` records for Ada.
5. The UI (debugger "Export") could offer `/debug/compile?lang=` for all languages.
