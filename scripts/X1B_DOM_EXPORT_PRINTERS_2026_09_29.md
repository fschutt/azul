# X1b: DOM export in 15 printers (report, 2026-09-29)

Branch `wt/x1b-dom-export-printers`, cut from `0a326afe5`. **Nothing was compiled** (house rule).
Each edited Rust file passes `rustfmt --edition 2021 --check`, used here as a parse check only.
The formatting diffs it prints come from the nightly-only `rustfmt.toml`.

## 1. Result per language

| Language | DOM export | Registration | App |
|---|---|---|---|
| Crystal | done: `Azul::Dom.div.with_css(..)` chain; `String` params with defaults; `"by #{author}"` | done: lib structs; class methods behind non-capturing proc literals (C fn pointers) | shard.yml + ui.cr + main.cr (examples/crystal) |
| Odin | done: C API nested, trailing commas; `css_str(title)`; `css_concat(..)` (core:strings) | done: mirrors C, `proc "c"` + `runtime.default_context()` | ui.odin + main.odin |
| V | done (linear): one temporary per chain, declared by its first value; `azul.az_str('by ${author}')` | done: mirrors C (V functions are C functions) | ui.v + main.v |
| Ada | done (linear): package `Ui`, `String` params with defaults, `To_Az_String ("by " & Author)` | done: C-convention functions, model read through address overlays | azul_app.gpr + ui.ads/adb + main.adb |
| FreeBASIC | done (linear): `ByRef .. As Const String = ".."`, `CssStr("by " & author)` | done: mirrors C | ui.bas + main.bas |
| Fortran | done (linear): `character(len=*)` dummies, `azul_string('by ' // author)` | **limited**: tagged unions are opaque blobs, so a render fn cannot read the String default | ui.f90 + main.f90 (idiomatic `dom_t` layout callback) |
| Common Lisp | done: raw CFFI calls nested; `&optional` defaults; `(css-str (concatenate 'string ..))` | **limited**: the binding's C callbacks go through the host invoker (no ComponentRenderFn kind), and CFFI `defcallback` cannot return a struct by value | azul-app.asd + ui.lisp + main.lisp (examples/lisp word-struct pattern) |
| Racket | done: kebab wrappers nested; optional args; `(string->azul-string (string-append ..))` | done: `function-ptr` callbacks, `union-ref` model reading, library through `app-config-add-component-library` | ui.rkt + main.rkt |
| PowerShell | done: `[Azul.NativeMethods]::AzDom_withChild(..)` nested; `param()` with defaults | done: script blocks of the C# delegate types → `GetFunctionPointerForDelegate`; `Add-<Lib>Library` via `..addComponentLibraryStruct` | ui.ps1 + main.ps1 (LayoutCallbackInvokerDelegate + host invoker) |
| Smalltalk | done: `(AzulNative azDomWithChild: .. child: ..)`, one keyword per line when nested; keyword method per item plus a unary method with the defaults | **limited**: the binding makes no C callbacks at all (no FFICallback wrappers, no host invoker) | **limited**: AzulUi.st + main.st builds the UI once and says why no window opens |
| Perl | **limited**: every tagged union is a fake `sint32` + `uint8[256]` record, so a Dom (NodeData holds NodeType) has the wrong size by value | - | - |
| Red/System | **limited**: Red/System builds 32-bit executables only, while the binding mirrors libazul's 64-bit LP64 layouts (the parent's "respect the red limit") | - | - |
| VB6 | **limited**: a Declare is stdcall, libazul is cdecl, and VB6 needs a 32-bit libazul (the Byref twins do not help) | - | - |
| COBOL | **limited**: the copybook records are packed (AzDomVec ends in a 1-byte FLAGS-X without C padding), so AzDom is smaller than the C struct the Byref twin writes; COBOL cannot define the layout callback | - | - |
| ALGOL 68 | **limited**: a68g rejects the binding's `ALIEN` declarations (no FFI) | - | - |

`exports_dom()` is true for 10 printers: ada, crystal, fortran, freebasic, lisp, odin, powershell,
racket, smalltalk and v. The structure test expects that list, together with rust, c, cpp and
python. Every limited printer now overrides `dom_limitation` with the precise reason above,
instead of printing "not implemented yet".

### Extra scope (coordinator): Racket union variants by field name

F1 adds a `pad0` slot between `variant-tag` and `payload` (checked in the regenerated
`azul.rkt`: `[pad0 (_array _uint8 7)]`). The positional `make-<Union>_Variant_<V>` therefore
took the pad as an argument.

The printer now calls `(css-union _AzT AzT_Variant_V-tag set-AzT_Variant_V-variant-tag! AzT_Tag_V
set-AzT_Variant_V-payload! payload)`. The helper works like this:
- it mallocs the union;
- `cpointer-push-tag!`s the variant struct's pointer tag onto that memory;
- sets both fields through the `define-cstruct` setters, by name.

No pad number appears anywhere, and a structure test rejects any positional `make-..._Variant_..`.

## 2. Commits (oldest first)

| hash | what |
|---|---|
| fe0ac6551 | test(css): racket sets a union variant by field name (RED: 3 hand-edited goldens + structure test) |
| 73d3b6e4e | fix(css): racket sets a union variant by field name |
| ae6ec7ec0 | docs(x1b): progress |
| d8da7de4f | test(css): crystal and odin export the DOM (RED: hand-written `dom_card` goldens, structure list) |
| 149adf4a5 | feat(css): crystal and odin export the DOM |
| 5560953f5 | test(css): ten more printers export the DOM (RED: structure list) |
| f21ac11a2 | feat(css): linear.rs DOM hooks; v, ada, fortran, freebasic |
| fbb4abd56 | feat(css): lisp, racket, powershell, smalltalk |
| bc23d95ba | fix(css): perl, red, vb6, cobol, algol68 say why |
| 06e372307 | docs(x1b): progress |
| 8538420dd | test(css): drop the dom_app goldens the app projects no longer write |
| 248cd159e | fix(css): Ada / FreeBASIC apps call AzWindowCreateOptions_create as exported |
| 1207452de | fix(css): red / vb6 limitations name what still blocks them after F1 |
| (this) | docs(x1b): report |

## 3. Shared code (and the overlap with X1a)

- **`lang/linear.rs`:**
  - DOM for every statement printer, done once.
  - A builder method is a statement.
  - A chain updates ONE temporary: `t1 = AzDom_withChild(t1, child)`, via `Ctx::is_temp_of`.
  - A child chain gets its own temporary first.
  - New `LinearSyntax` hooks:
    - `dom_limitation` (default: the old `LINEAR_DOM` text);
    - `method` (default: the C-ABI call with the receiver first, through `call`);
    - `param` / `param_expr`;
    - `concat` / `concat_expr`.
  - `item_blocker` asks `dom_limitation` instead of refusing every DOM item.
  - Pascal (X1a's) keeps working unchanged until it opts in.
- **`lang/dom.rs`:** adds `is_dom_item`, `one_line` and `registration_note`. The text is
  **identical** to X1a's working copy, so on merge keep one copy of each. X1a's block also has
  `DOM_CLASSES` and `WrapperDom`.
- **`lang/mod.rs`:**
  - `item_dom_blocker` uses `dom::is_dom_item`.
  - `call_param_names` knows the DOM lowering's multi-argument `Dom` constructors.
  - Both hunks are identical to X1a's.
- **`smalltalk.rs`:** has one local helper, `method_param_name` (`with_child` → `child`, ...). It
  gives the keyword after the receiver. If X1a adds a shared twin, keep one.
- **`layout/src/e2e/export.rs`:** not touched.
  - X1a's branch replaces the hard-coded `["rust","c","cpp","python"]` in
    `the_dialogs_get_one_language_list_the_code_generators_and_whether_they_do_dom` with a list
    derived from `all_backends()`.
  - It also replaces the README's "does not write a runnable app yet".
  - **If X1b is merged before X1a, that layout test fails** until X1a's version is in.

## 4. Binding bugs found (for the generator owners, `doc/src/codegen/v2`; not edited here)

1. **Ada and FreeBASIC bind callback-taking functions under the wrong C symbol.**
   `azul.ads` / `azul.bi` declare `AzWindowCreateOptions_create` and
   `AzAppConfig_addComponentLibrary` with the wrapper record (`Az_LayoutCallback`,
   `AzRegisterComponentLibraryFn`). They import them under the raw symbol, which takes the
   function pointer; the record form is `..Struct`. Racket gets this right
   (`#:c-id AzWindowCreateOptions_createStruct`), and so does C#.
   - The generated Ada / FreeBASIC programs work around it: they import the symbol as libazul
     exports it.
   - Probably every function with a callback argument is affected.
2. **Stale examples.**
   - `examples/lisp` calls `AzWindowCreateOptions_default`; the symbol is `..._createDefault`.
   - `examples/racket` calls `make-window-create-options`, which does not exist; the binding has
     `window-create-options-create`.
   - `examples/powershell` casts the layout block to `Func[IntPtr,IntPtr,object]`; the C# layer
     takes a `LayoutCallbackInvokerDelegate(ulong, IntPtr, IntPtr, IntPtr outPtr)`.
   - The generated apps follow the bindings, not these examples.
3. **Perl:** the union records (F1 section 9, not done) also break the Perl **CSS** output. Its
   CssProperty values cross the FFI with the fake 260-byte size too; B2's printer only reports
   "limited".
4. **Red:** B2's CSS reason ("8-byte union placeholder") is stale since F1. The Red CSS printer is
   a stub anyway (strukt / vec are placeholders), and the 32-bit target blocks it.

## 5. api.json

No change.

## 6. Least sure to compile (Rust)

- **`linear.rs` Method arm:**
  - `self.emit(recv)` with `recv: &Box<Expr>` (deref coercion);
  - the `if self.is_temp_of(..) { r.clone() } else { self.temp(ty) }` borrow sequence.
- **`v.rs` item_fn:** `body[assigned[0]] = format!(.., &body[assigned[0]][first.len()..])`. The
  RHS is evaluated before the place, so it should borrow-check.
- **`crystal.rs` / `odin.rs` / `v.rs` / `ada.rs` / `freebasic.rs` / `racket.rs` /
  `powershell.rs` registration:**
  - long `write!` format strings that mix positional `{}` with captured identifiers (`{cu}`,
    `{fields}`, `{components}`, `{module}`); the counts were checked by hand;
  - `for (c, item) in &components` over `Vec<(&ComponentSpec, &Item)>` (`c: &&ComponentSpec`).
- **`smalltalk.rs`:** `keys.get(i).map_or_else(..)` on an array of `&str`.
- **Unused imports:** checked by hand per file. CI's clippy runs with `-D warnings`.

## 7. Least sure in the generated code (by language)

- **Odin:** `string(f.name.vec.ptr[:len])` slice-to-string; `[?]T{..}` + `&fields[0]`;
  `base:runtime`.
- **V:** union field reads inside `unsafe {}`; the `[..]!` fixed arrays; enum
  `azul.AzComponentSource.UserDefined` (uppercase enum and union names, as the binding declares
  them).
- **Crystal:** that the proc literals passed to `LibAzul::AzComponentDef.new` count as
  non-closures (they reference only the `AzulUi` constant).
- **Ada:**
  - the imported constant overlays in `Model_String`;
  - variant-record component reads (`Payload_Some_K`) after the tag check;
  - aspects on body-local subprogram declarations.
- **FreeBASIC:** `r[j] = s.vec.ptr_[j]` byte writes; the field named `name` (as azul.bi declares
  it).
- **Racket:** `union-ref` on a cstruct's union field; `function-ptr` callbacks returning a
  cstruct by value; the host invoker writing back the library struct.
- **PowerShell:** script blocks as delegates with struct returns; `Marshal.StructureToPtr` of a
  boxed `AzDom`; multi-line method-call argument lists.
- **Lisp:** the CFFI plist round-trip of AzDom (it holds unions). The CSS output already relies
  on the same round-trip for CssProperty.
- **Smalltalk:** the binding itself does not load as a Tonel package yet (see
  `examples/smalltalk`).

## 8. Commands for the parent

```sh
# printers + goldens (bless first: every DOM golden of the 15 languages changes)
AZ_BLESS=1 cargo test -p azul-css --features codegen,parser --test codegen_goldens
git diff --stat css/tests/codegen_goldens     # review; crystal/odin dom_card must NOT change (hand-written RED)
cargo test -p azul-css --features codegen,parser --test codegen_goldens
cargo test -p azul-css --features codegen,parser --test codegen_structure
cargo test -p azul-css --features codegen,parser --lib codegen
# after merging X1a too (the dialogs' language-list test lives in layout):
cargo test -p azul-layout --features e2e-server --lib e2e::export::tests
cargo test -p azul-layout --features e2e-server --lib export_tests
```

**Goldens to bless:**
- `dom_card`, `dom_library` and `dom_app/*` of all 15 languages. Keep the hand-written
  `crystal/dom_card.cr` and `odin/dom_card.odin`: if the output differs, the printer is wrong.
- The new app files:
  - `ui.*` + `main.*` for 10 languages;
  - `ada/dom_app/azul_app.gpr`, `lisp/dom_app/azul-app.asd`, `crystal/dom_app/shard.yml`;
  - `smalltalk/dom_app/AzulUi.st`.
- `racket/{families,paint}.rkt`: css-union by name. Its `basic`, `keywords` and
  `project/styles.rkt` are already hand-edited.
- The stale `dom_app` files the new projects no longer write are deleted (8538420dd).

## 9. Left

- Bless the goldens (section 8).
- Registration for Fortran, Lisp and Smalltalk needs binding work:
  - Fortran: variant accessors for its opaque unions.
  - Lisp: a host-invoker kind for ComponentRenderFn, or struct-return callbacks.
  - Smalltalk: callbacks at all.
- The Smalltalk app cannot open a window until the binding has callbacks.
- Fix the binding bugs of section 4; the Ada / FreeBASIC workarounds can then go.
