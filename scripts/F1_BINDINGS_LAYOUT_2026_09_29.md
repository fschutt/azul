# F1 - bindings generator follow-ups: union payload layout, azul.h, bug classes (2026-09-29)

Branch `wt/f1-bindings-layout`, cut from 282890483 (the local tip of `fix/input-bugs-2026-09-19`,
PR #476). 24 commits (RED test commit first, then the fix, per item). Nothing was compiled here
(house rule): the parent compiles once and runs the suites. The PR tip has since moved to
0a326afe5; this branch does not merge it (cherry-pick it). Checkpoint:
`scripts/F1_BINDINGS_LAYOUT.PROGRESS.md`.

## 1. Result

- **P0, done.** In azul.h, 210 variant payloads of 47 tagged unions sat at a different offset
  than in the DLL. **One function now decides** where a payload goes:
  `c_layout::union_payload_layout`. azul.h pads each variant from it and checks every padded
  offset at compile time. 15 bindings pad from the same function. OCaml reads the offset
  directly, and its private layout calculator is gone. The C conformance program reads every
  comparable payload back through the union.
- **P1, done.** All three failing bug-class tests are fixed at their cause:
  - zig: keyed on the IR category;
  - byref exports: the macro now requires the argument;
  - D: `&mut self` of a native type.

  azul.h compiles as C, and a new bug class guards it.
- **P2, done where reading was enough.** Ada, FreeBASIC, Red and VB6 are fixed, each with a
  RED test. GetHash imports are moved. ALGOL 68 cannot be fixed by reading: its FFI construct
  does not exist in a68g. The rest is listed precisely in section 7.

## 2. P0 - the tagged-union payload offset

**Rule.** Rust lays a `#[repr(C, u8)]` / `#[repr(C)]` enum out as
`struct { tag; union { one repr(C) struct per variant } }`. Every payload therefore starts at
the tag size rounded up to the largest alignment of any variant. azul.h used
`{ uint8_t tag; T payload; }` per variant instead, which uses the payload's own alignment.

**Rust ground truth:** `css/tests/a_union_payload_sits_after_the_largest_alignment.rs`.
- `StyleBackgroundContent::Color` has its ColorU bytes at offset 8.
- `CssProperty::Display` has its inner tag at 8 and its payload at 12.

**Structure (no duplication):**

- **`doc/src/codegen/v2/c_layout.rs`**
  - `union_payload_layout(name, ir) -> Option<UnionPayloadLayout>` returns
    `{ tag, payload_offset, abi, padding(variant), padded_variants() }`. It covers regular
    enums and monomorphized `TaggedUnion` aliases, which both reduce to one `UnionShape`.
    `variant_payload_padding(union, variant, ir)` is the per-variant shorthand.
  - Padding rule: a variant gets `payload_offset - tag.size` bytes exactly when C's own
    alignment of its first payload member would place it elsewhere.
  - `enum_layout` / `mono_layout` now model the padded structs (`variant_struct_layout`), so
    the Fortran blobs and `return_c_size` describe the corrected header. Sizes do not change
    (proved in the doc comment; tested).
  - The module doc no longer presents the "1536/1536 clang" match as a check against Rust:
    that comparison was model vs. header.
- **`lang_c.rs`**
  - The two variant-struct emitters are merged into `generate_union_type`, fed by
    `CUnionVariant` lists.
  - It emits `uint8_t _pad0[N];` after the tag where N > 0.
  - After each union it emits an `#ifdef AZ_LAYOUT_CHECK` block with
    `AZ_LAYOUT_CHECK(offsetof(AzXVariant_Y, payload) == N, "...")` for every padded variant.
    `AZ_LAYOUT_CHECK` is defined in the preamble as `_Static_assert` (C11) or `static_assert`
    (C++11, MSVC ≥ 2010), and is undefined before those. The LuaJIT/PHP cdef stripper drops
    `#if` blocks, so Lua and PHP get the padding but not the checks.
  - `matchRef` / `matchMut` read `&casted->payload`, so they are correct now.
- **Bindings:** each binding calls `union_payload_layout` once per union and spells the padding
  in one small `emit_variant_padding` (no layout logic in the bindings):

  | Binding | Padding as emitted |
  |---|---|
  | Go | `_ [N]byte`, plus an `unsafe.Offsetof` compile-time check |
  | Node/koffi | N `uint8_t` members (the Deno shim cannot parse arrays) |
  | Crystal | `UInt8[N]` |
  | Odin | `[N]u8` |
  | V | `pad0 [N]u8` |
  | Racket | `(_array _uint8 N)` |
  | Java | `byte[]`, added to `getFieldOrder` |
  | Kotlin | `ByteArray`, added to the field order |
  | C# | N `public byte` fields |
  | D | `ubyte[N]` |
  | Zig | `[N]u8` (`ir` is now threaded through `c_decls`) |
  | Nim | `pad0*: array[N, uint8]` |
  | Julia | `NTuple{N,UInt8}` |
  | Smalltalk | `(uint8 _pad0[N])`. Also, the u8 tag field is now `uint8`, not the int-sized enumeration |
  | Common Lisp | `(pad0 :uint8 :count N)` |

  - **OCaml** has no records. Its Option/Result extractor now adds `payload_offset`, and it
    sizes unions from `c_layout`. The ~230-line private calculator is deleted.
  - **VB6:** its comment now gives the real offset.
  - **Unaffected:**
    - Pascal, FreeBASIC, Ruby and Ada already place payloads at the largest alignment (variant
      records, or tag + union).
    - C++, Swift, Lua, PHP and Haskell use azul.h's layout.
    - Fortran, COBOL, Perl, Red, ALGOL 68 and Python never read a payload at an offset.
- **Conformance:** `VariantCase::payload_check`. The C program passes a distinctive primitive
  (0x5A.., 1.5, true), or compares through `Az<Payload>_partialEq`, reading
  `v.<Variant>.payload` back.

**Tests (all over every union, not a sample):**

| Test | What it checks |
|---|---|
| `bug_classes::a_union_variant_payload_starts_where_rust_puts_it` | azul.h is generated in memory; each variant's members before its payload are parsed and compared with Rust's rule. RED before 9ca29759c |
| `bug_classes::azul_h_checks_every_padded_variant_payload_at_compile_time` | Every padded variant has its compile-time check. RED |
| `bug_classes::c_layout_sizes_every_tagged_union_like_rust` | Guard; passes before and after |
| `bug_classes::every_binding_pads_a_union_variant_payload_like_azul_h` | Reads `target/codegen`, so it needs `codegen all`. RED before 8f1b6dd45 |
| `c_layout::tests::a_small_payload_is_padded_to_the_largest_variant_alignment` | Unit test |
| `c_layout::tests::an_int_tag_pads_to_the_largest_variant_alignment_too` | Unit test |
| `c_layout::tests::a_union_of_one_alignment_needs_no_padding` | Unit test |
| `conformance::tests::the_c_program_reads_every_variant_payload_back_through_the_union` | RED before a3aa9e8d5 |

**Affected unions.** Measured on the 08:00 `target/codegen/azul.h` with
`clang -Xclang -fdump-record-layouts-simple`: 47 unions, 210 variants. For each: the Rust offset,
then the C offset and the variants at it. The RED test prints the same list:

- `AzAccessibilityAction` (Rust 8): at 4: ScrollToPoint, SetScrollOffset, CustomAction
- `AzAttributeType` (Rust 8): at 1: ContentEditable, Draggable; at 4: MinLength, MaxLength, ColSpan, RowSpan, TabIndex
- `AzColorOrSystem` (Rust 4): at 1: Color
- `AzComponentDefaultValue` (Rust 8): at 1: Bool, ColorU; at 4: I32, U32, F32
- `AzCssColorParseErrorOwned` (Rust 8): at 1: InvalidColorComponent; at 4: IntValueParseErr, FloatValueParseErr, FloatValueOutOfRange, MissingColorComponent
- `AzCssDirectionParseErrorOwned` (Rust 8): at 4: ParseFloat
- `AzCssFontWeightParseErrorOwned` (Rust 8): at 4: InvalidNumber
- `AzCssPathPseudoSelector` (Rust 8): at 4: NthChild
- `AzCssPathSelector` (Rust 8): at 4: Type
- `AzCssProperty` (Rust 8), 97 variants:
  - at 1: CaretColor, SelectionBackgroundColor, SelectionColor, TextColor, HangingPunctuation, TextCombineUpright, BorderTop/Right/Left/BottomColor, ScrollbarColor, ColumnRuleColor
  - at 4: the other 85, i.e. every `CssPropertyValue<unit enum or 4-aligned value>`: FontWeight, FontStyle, TextAlign, Display, Float, Position, ZIndex, Flex*, Align*, Justify*, Overflow*, Break*, Column*, ListStyle*, ...
- `AzCssPseudoSelectorParseErrorOwned` (Rust 8): at 4: InvalidNthChild
- `AzCssShape` (Rust 8): at 4: Circle, Ellipse, Inset
- `AzCssStyleColorMatrixParseErrorOwned` (Rust 8): at 4: Float
- `AzCssStyleCompositeFilterParseErrorOwned` (Rust 8): at 4: Float
- `AzCssStyleTransformParseErrorOwned` (Rust 8): at 4: NumberParseError
- `AzDirection` (Rust 8): at 4: FromTo
- `AzDynamicSelector` (Rust 8): at 4: Os, OsVersion, Media, ViewportWidth, ViewportHeight, ContainerWidth, ContainerHeight, AspectRatio, Orientation, PrefersReducedMotion, PrefersHighContrast, PseudoState
- `AzFluentArg` (Rust 8): at 4: I32, F32
- `AzFmtValue` (Rust 8): at 1: Bool, Uchar, Schar; at 2: Ushort, Sshort; at 4: Uint, Sint, Float
- `AzFocusTarget` (Rust 8): at 4: Directional
- `AzGlobalHotkeyError` (Rust 8): at 4: AlreadyRegistered
- `AzGridLine` (Rust 8): at 4: Line, Span
- `AzGridTrackSizing` (Rust 8): at 4: Fr
- `AzMarginBoxContent` (Rust 8): at 4: PageCounterFormatted
- `AzMenuItemIcon` (Rust 8): at 1: Checkbox
- `AzNativeGestureEvent` (Rust 8): at 4: Swipe
- `AzNodeType` (Rust 8): at 4: TransientWindow, GeolocationProbe
- `AzNodeTypeFieldValue` (Rust 8): at 1: CheckBox, ColorInput; at 4: NumberInput
- `AzPercentageParseError` (Rust 8): at 4: ValueParseErr
- `AzPercentageParseErrorOwned` (Rust 8): at 4: ValueParseErr
- `AzPermissionState` (Rust 4): at 1: EphemeralGranted
- `AzPixelValueOrSystem` (Rust 8): at 4: System
- `AzRawWindowHandle` (Rust 8): at 4: Web
- `AzResultAppliedEditDocumentEditError` (Rust 8): at 4: Err
- `AzResultEmptyStructFileError` (Rust 8): at 1: Ok
- `AzResultGlobalHotkeyGlobalHotkeyError` (Rust 8): at 4: Ok
- `AzResultRawImageDecodeImageError` (Rust 8): at 4: Err
- `AzResultU8VecEncodeImageError` (Rust 8): at 4: Err
- `AzScreenCaptureSource` (Rust 8): at 4: Display
- `AzStyleBackgroundContent` (Rust 8): at 1: Color; at 4: SystemColor
- `AzStyleFilter` (Rust 8): at 1: Flood; at 4: Blend
- `AzStyleFontFamily` (Rust 8): at 4: SystemType
- `AzTextOperation` (Rust 8): at 4: SetSelection, ExtendSelection, ClearSelection, MoveCursor, SelectAll
- `AzThreadReceiveMsg` (Rust 8): at 4: Update
- `AzXmlError` (Rust 8): at 4: InvalidXmlPrefixUri, UnexpectedXmlUri, UnexpectedXmlnsUri, InvalidElementNamePrefix, UnexpectedEntityCloseTag, MalformedEntityReference, EntityReferenceLoop, InvalidAttributeValue, UnexpectedDeclaration, InvalidName, NonXmlChar, InvalidChar, InvalidChar2, InvalidString, InvalidExternalID, InvalidComment, InvalidCharacterData, UnknownToken
- `AzXmlParseError` (Rust 8): at 4: UnknownToken
- `AzXmlStreamError` (Rust 8): at 4: NonXmlChar, InvalidChar, InvalidQuote, InvalidSpace

**Checked by hand, without cargo:**

- **azul.h (C/C++).** I applied exactly these paddings and checks to a scratch copy of the
  current azul.h:
  - `clang -fsyntax-only` passes for C99, C11, C17 `-Wpedantic` (0 warnings), C++03, C++11
    and C++20.
  - All 210 checks hold.
  - No record changes size (5610 records compared).
  - A wrong padding trips its check.
- **Other languages.** These put the payload at 8 with the exact emitted spellings: Go (`go
  vet` + run; the Offsetof check fails on a wrong pad), Zig, Odin, ldc2, Nim, Crystal, V
  (`-gc none`) and Racket.

## 3. P1

- **Zig, `emitters_never_key_behaviour_on_api_names`:** the comptime `String.tr` is now keyed on
  `TypeCategory::String`, and hand-written methods go in a `handwritten` set that supersedes the
  generated factory of the same name.
- **`bindings_only_reference_symbols_libazul_exports`** (go/haskell/lua):
  - Cause: `impl_managed_callback!` took `from_handle_byref_fn` as an optional argument.
    79 of 81 sites named it; the two newest widget kinds did not.
  - Fix: the argument is now required in all five forms of the macro, so any future kind that
    omits it fails to compile. `tree_view.rs`, `video.rs` and `host_invoker_test.rs` now name
    theirs.
- **D, `every_api_function_is_reachable_from_the_idiomatic_api`:** a `&mut self` method of a
  natively mapped type becomes a `ref` receiver with write-back. `String::set_localizable` is
  now `stringLocalizable(ref string self, bool)`, called through UFCS.
- **azul.h as C:**
  - The static-bytes helpers are renamed `AzString_fromStaticBytes` and
    `AzString_trStaticBytes`, and `TR(key)` uses the latter.
  - The 1-argument `AzString_fromConstStr(s)` initializer macro is kept (AZ_REFLECT, docs).
  - New bug class: `azul_h_never_emits_one_name_as_macro_and_function_or_with_two_linkages`.
    Its Python port finds exactly the two known names on today's header and none after the
    rename.
  - Checked: C99, C11 `-Wall` and C++11 compile a TU using `TR`, `AzString_fromStaticBytes` and
    a static `AzString_fromConstStr("..")`.

## 4. P2

Each fix below has its own RED test.

- **Ada:**
  - Every record type is `pragma Convention (C_Pass_By_Copy, ..)`. GNAT passed Convention-C
    records BY REFERENCE (RM B.3(69)).
  - `for X_Tag'Size use 8;` is emitted for u8 union tags; a Convention-C enumeration is
    int-sized.
- **FreeBASIC** (`azul.bi` did not compile):
  - Integer widths:
    - `i32` is now `Long`; it was `LongInt`, which is 64-bit.
    - 64-bit is `LongInt`/`ULongInt`; the binding used `LongLong`/`ULongLong` 4627 times, and
      FreeBASIC has no such types.
    - `isize`/`usize` are `Integer`/`UInteger`.
  - Emission follows the IR's `sort_order`. Before, 1205 by-value fields came ahead of their
    type.
  - Plain aliases and monomorphized aliases are now emitted; before, none were.
  - VecRef slices and destructor/clone types are emitted; every Vec holds its destructor by
    value.
  - The u8 union tag is `UByte`.
- **Red/System:**
  - Every union, regular or monomorphized, is a blob of exactly its `c_layout` size and
    alignment. It was one `byte-ptr!` whatever the union.
  - Emission follows `sort_order`, and monomorphized aliases are declared. Before, 828
    by-value fields came before their alias.
- **VB6:**
  - A function that passes or returns an aggregate is declared and called through its
    `<symbol>Byref` twin. This covers Declares, `Azul.bas` wrappers and `.cls` wrappers.
  - The decision is made in one place in `lang_vb6/functions.rs`: `uses_byref_twin`,
    `declared_symbol` and `call_lines`, using `CodegenIR::is_value_aggregate`.
  - This removes the by-value Declares that passed records ByRef, and every SKIPPED
    constructor.
- **GetHash:** the five imports now use `azul_css::hash::GetHash`, and `layout/Cargo.toml` drops
  azul-css's `codegen` feature. Core keeps `codegen` while `xml.rs` uses `VecContents` (B2
  follow-up 3).

## 5. Commits (oldest first)

| Commit | What |
|---|---|
| 5af803035 | test(codegen): RED - a union variant payload starts where Rust puts it |
| 9ca29759c | fix(codegen): azul.h puts every union variant payload where Rust does |
| fd2ce2478 | test(codegen): RED - azul.h never emits one name as macro and function |
| 7b2dd2fdd | fix(codegen): azul.h compiles as C - the static-bytes helpers get free names |
| e865f4590 | fix(zig): key the comptime String.tr on the IR category, not the api name |
| 2f44856a5 | fix(core): every managed callback kind exports its byref constructor |
| b7d1f01ef | fix(d): a &mut self method of a natively mapped type is a ref member |
| cf13151b1 | test(codegen): RED - every binding pads a union variant payload like azul.h |
| 8f1b6dd45 | fix(codegen): every binding pads a union variant payload like azul.h |
| 9a8038c93 | test(conformance): RED - the C program reads every variant payload back through the union |
| a3aa9e8d5 | feat(conformance): read every comparable variant payload back through the union |
| ca4792584 | refactor(core,layout): import GetHash from the ungated azul_css::hash |
| 5aa26ede8 / ec0609854 | test RED / fix(ada) |
| a5b8b85b7 / 3dab8a0d3 | test RED / fix(freebasic) |
| 076ffec91 / 87100fd51 | test RED / fix(red) |
| fb769705d / cabfe7228 | test RED / fix(vb6) |
| 22cd9d7eb, dcbb9963f, ca61dd5eb, 08334b70a, + the final one | progress checkpoints, this report |

The three P1 bug-class tests were already RED on the PR, so their fix commits have no separate
RED commit. For the combined RED pass, apply each fix commit in reverse and expect:

- the test named in its RED commit to fail;
- `c_layout_sizes_every_tagged_union_like_rust` to pass on both sides.

## 6. Least sure to compile

- **`bug_classes.rs`:**
  - `VariantRecords` uses `fn(..)` pointer fields initialised with non-capturing closures
    (`record: |u, v| format!(..)`, `files: || one("..")`, where `one` is a nested fn).
  - `leading_ident` / `declared_fn_name` return borrowed slices.
  - `c_struct_members` uses an `Option<(String, Vec<String>)>` state machine.
- **`c_layout.rs` `union_layout`:** the zip of `&[(&str, Vec<(&str, FieldRefKind)>)]` with
  `&Vec<Vec<AbiLayout>>`, and `member_layout(t, *rk, ..)` with `t: &&str`.
- **`lang_c.rs` `generate_union_type`:**
  - the closure `padding` borrows `layout`, which the later `if let Some(layout) = &layout`
    also reads;
  - `filter_map(|v| v.members.first().map(|(member, _, _)| (v.name, member)))`.
- **`lang_d/wrappers.rs` `Recv::Native` arm:** a nested `if is_mut { .. } else { .. }` whose
  value is the call argument, with `self_param` set in both branches.
- **`lang_freebasic/types.rs` and `lang_red/mod.rs`:** function-local `enum Item<'a>` in a
  `Vec<(usize, Item)>` (elided lifetime), sorted with `sort_by_key(|(order, _)| *order)`.
- **`lang_ocaml/types.rs`:** `if let Some(AbiLayout { size, align }) = aggregate` and
  `AbiLayout { size: 8, align: 8 }` literals from outside `c_layout`. The fields are `pub` and
  the struct `pub(crate)`.
- **`lang_vb6/wrappers.rs`:** `functions::{arg_clause_and_type, call_lines, uses_byref_twin}`
  are `pub(super)` in a sibling module.
- **`core/src/host_invoker.rs`:** every `$( from_handle_byref_fn: .. )?` became mandatory.
  All 82 invocations (core, layout and the test fake) now pass it; a missed one would be a
  macro-match error.
- **`layout/Cargo.toml`:** `features = ["parser"]` (codegen dropped). If anything in layout
  still reaches `azul_css::codegen`, this is where it would show. I grepped for it and found
  nothing.

## 7. What the parent must run

```sh
# regenerate every binding (needs cargo)
cargo build --release -p azul-doc && ./target/release/azul-doc codegen all

# azul.h on its own, as C and C++ (the 4 errors are gone; the 210 layout checks run)
printf '#include "azul.h"\nint main(void){return 0;}\n' > /tmp/t.c
clang -fsyntax-only -std=c99  -I target/codegen /tmp/t.c
clang -fsyntax-only -std=c11 -Wall -Wpedantic -I target/codegen /tmp/t.c
cp /tmp/t.c /tmp/t.cpp && clang++ -fsyntax-only -std=c++11 -I target/codegen /tmp/t.cpp
clang++ -fsyntax-only -std=c++20 -I target/codegen /tmp/t.cpp
grep -c 'AZ_LAYOUT_CHECK(offsetof' target/codegen/azul.h        # expect 210

# tests
cargo test -p azul-doc --lib codegen::v2        # bug_classes (needs target/codegen), c_layout, conformance, emitter unit tests
cargo test -p azul-css --test a_union_payload_sits_after_the_largest_alignment
cargo check -p azul-core -p azul-layout -p azul-dll            # macro change, GetHash, layout without css/codegen
cargo test -p azul-core host_invoker                             # AutoWrapper now has a byref fn

# conformance (C reads every comparable payload back through the union)
cargo build --release -p azul-dll --features build-dll,alloc-stats
bash scripts/conformance.sh c
```

Optional compile checks with the other toolchains, if installed. Each binding must build
against the regenerated output:

- `go vet ./...` in `target/codegen/go`;
- `zig build` against `azul.zig`;
- `nim check azul.nim`;
- `crystal build --no-codegen azul.cr`;
- `odin check`.

## 8. What changed that is not mine to edit

- **`css/src/codegen/**` (B3).** Three CSS printers build padded variant records positionally.
  For a `TaggedShadowed` CssProperty variant that is padded (97 of CssProperty's), they need
  the padding argument:
  - `css/src/codegen/lang/cpp.rs:68-76`: `Az{ty}Variant_{variant}{ tag, payload }` is aggregate
    init and would now initialise `_pad0` with the payload. Use `{ tag, {}, payload }` or assign
    `.tag` / `.payload`.
  - `css/src/codegen/lang/julia.rs:72-83`: `Azul.Az{ty}Variant_{variant}(UInt8(tag), payload)`
    calls the positional constructor, which now has `_pad0::NTuple{N,UInt8}` second. Pass
    `ntuple(_ -> 0x00, N)`.
  - `css/src/codegen/lang/racket.rs:130-141`: `make-Az{ty}_Variant_{variant}` is positional and
    now has the `pad0` slot. Pass an N-byte array.
  - N is `c_layout::union_payload_layout(ty).padding(variant)`. The css crate cannot depend on
    the doc crate, so B3 needs that number in its own tables. It is always
    `payload_offset - tag size`: 7 for every padded CssProperty variant, whether its payload
    is 1- or 4-aligned.
  - `CssPropertyValue<T>` monos (`Generic`) have one payload variant and need no padding.
  - Every other printer builds these records keyed or through constructors: C, C#, Nim, V,
    PowerShell, Java, Kotlin, Lua, Odin, Zig, Go, Crystal, PHP, Ruby.
  - The CSS golden files that contain these three shapes must be re-blessed after the change.
- **`api.json`:** no change needed. Its C doc examples, and the same text in
  `core/src/callbacks.rs:1803`, `core/src/resources.rs:507,1300,1356,1390` and
  `layout/src/callbacks.rs:1927-1982`, pass `AzString_fromConstStr("id")` as a function
  argument. That is a brace-initializer macro and does not compile there; `AZ_STR("id")` is
  the expression form.

## 9. Left (precise)

- **ALGOL 68** (`doc/src/codegen/v2/lang_algol68`). a68g 3.11.3 (installed) rejects the binding's
  two foundations, even in 1-line files:
  - `ALIEN` ("tag ALIEN has not been declared properly"). This affects every
    `PROC .. : ALIEN ".." ! "azul"` (19321 in `azul.a68`; `functions.rs:164-168`,
    `wrappers.rs:110`).
  - `REF VOID` ("incorrect sentence"; 105 uses; `mod.rs:341-390`).

  Monomorphized MODEs (`AZPHYSICALSIZEUTHREETWO`) are also undeclared. a68g has no FFI, so no
  generator change makes this binding call libazul. It needs a different a68 implementation or
  a C shim; that is a decision, not a fix.
- **VB6 cannot call libazul at all.** A `Declare` is stdcall, and libazul exports cdecl. On
  32-bit x86 they differ in who pops the arguments, so every call with arguments raises "Bad
  DLL calling convention" (error 49). VB6 also needs a 32-bit libazul, while `c_layout` models
  64-bit. Needed: an `i686` libazul build with `extern "system"` twins (or a stdcall shim DLL),
  and 32-bit layouts.
- **COBOL** (B2): the copybook `TYPEDEF` records are packed (no `SYNC`), so any record with C
  padding is smaller than the C struct (`lang_cobol/types.rs`). Emit `SYNC` on every elementary
  item, or explicit `FILLER` from `c_layout`. Not done.
- **Perl** (B2): union records are `record_layout_1('sint32' tag, 'uint8[256]' payload)`
  (`lang_perl/types.rs:397-432`): wrong tag width and a fake size. Needs `c_layout` sizes.
  Not done.
- **Smalltalk / ALGOL 68 / FreeBASIC `XValue` constructors** (B2 section 5): FreeBASIC now
  declares the aliases; Smalltalk still skips `type_aliases` entirely (`lang_smalltalk/types.rs`
  `generate_types`).
- **Latent, no instance today** (every data enum is `repr(C, u8)`): Pascal
  (`lang_pascal/types.rs:425,646`), Ruby, Node's direct path (`lang_node/types.rs:363`), Java,
  Kotlin and C# hard-code a 1-byte tag. A `repr(C)` data enum would break them. They should
  take `c_layout::union_payload_layout(..).tag`.
- **FreeBASIC**, not compiled (no fbc): the width of its `Enum`, used for unit-enum fields, is
  not pinned to 32 bits.
- **Racket**, user-visible: the positional `make-Az..._Variant_X` of a padded variant gains the
  `pad0` argument. Generated code never calls it.
