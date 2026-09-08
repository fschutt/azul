# Fortran binding rewrite — parked notes (2026-09-08)

Task: make the Fortran binding usable (owner: "pretty unusable") and fix the
monomorphized tagged-union tag width in `lang_fortran/layout.rs`. Work was
parked after research + design; the emitter rewrite is NOT done. This file
is the handoff so the next session can resume without the conversation.

## Branch / worktree state

- Worktree `.claude/worktrees/agent-af41a6602068d11a0`, branch
  `worktree-agent-af41a6602068d11a0`, based on `site/quality-pass` at
  `3c24640c0` (the worktree had been cut from an old master commit
  `7874670e0` with no commits of its own; it was `git reset --hard 3c24640c0`).
- Nothing pushed. `master`, `feat/web-api-remodel`, `site/quality-pass` untouched.
- The WIP commit carrying this note also carries the ONE change made so far:
  `doc/src/codegen/v2/lang_fortran/layout.rs::mono_layout`, `TaggedUnion` arm,
  now uses `tag_layout(repr.as_deref())` instead of a hard-coded 4-byte tag.
  **Unverified: not compiled, not tested; the pinning unit test is not written.**

## Environment prepared (verified by running)

- `brew install gcc` poured gcc 16.2.0 -> `/opt/homebrew/bin/gfortran`
  (GNU Fortran 16.2.0). There was no gfortran before.
- CI's Fortran lanes: `ubuntu-22.04` (apt `gfortran` = GCC 11), `macos-14`
  (`brew install gcc`), `windows-2022` (MSYS2 mingw gfortran). So generated
  Fortran must stay within F2003/F2008 core (nothing F2018-only, e.g. do not
  rely on `c_funloc` of a non-`bind(C)` procedure).
- `nice -n 10 cargo build --release -p azul-doc` in the worktree: OK (4m23s),
  `target/release/azul-doc`. `./target/release/azul-doc codegen all`: OK ->
  `target/codegen/azul.f90` (192,382 lines, 7.9 MB) + `Makefile.fortran`
  (byte-identical to the tracked `examples/fortran/Makefile`). Disk after the
  build: 22 GiB free (the e2e gate builds the DLL in the worktree; watch `df`).
- The gate: `nice -n 10 bash scripts/e2e_language_matrix.sh --gate-shipped fortran`
  (its `lang_fortran` recipe copies `target/codegen/azul.f90` and
  `Makefile.fortran` into `examples/fortran/`, runs `make`, then
  `./hello_world` with `AZ_E2E=tests/e2e/hello_world_counter.json`,
  `AZ_BACKEND=headless`; WORKS iff the log has `test result: ok`).
  Never run `codegen all` while a gate run is active.

## gfortran probe results (scratchpad `fprobe/probe.f90`, `probe2.f90`, both `-std=f2008 -Wall -Wextra`, clean)

1. gfortran 16 **finalizes function results after the assignment that
   consumed them** (`h = make_handle(7)` printed `FINAL id=7` after the
   assignment). Therefore `final ::` on a wrapper type returned by a factory
   would `_delete` every value a factory ever returns -> use-after-free.
   Decision: NO finalizers; explicit `delete` type-bound procedure guarded by
   an `owned` flag.
2. A handle store of `class(*), pointer` entries filled by
   `allocate(entry%obj, source=obj)` (obj is `class(*), intent(in)`) works;
   `select type (p); type is (t_model); p%counter = p%counter + 1` mutates
   the stored object and the change persists across lookups.
3. A `private` module can re-export `iso_c_binding` entities:
   `public :: c_int, c_int64_t, c_ptr, c_funptr, c_null_ptr, c_loc,
   c_f_pointer, c_associated, c_funloc, c_size_t, c_bool` compiled and a
   `use`-r used `c_int` without importing iso_c_binding.
4. `c_int == kind(0)` is true (plain `integer` == `integer(c_int)`), so the
   user-facing layer may use plain `integer` for unit enums.
5. A derived type with a component `procedure(cb_iface), pointer, nopass :: cb`
   in an allocatable, `save`d table, assigned from a DUMMY procedure
   (`handles(n)%cb => proc`) and later called from a `bind(C)` invoker that
   receives `type(c_ptr), value` args, `c_f_pointer`s them into a borrowed
   wrapper (`a0%raw = p0; a0%owned = .false.`) and writes the result through
   an out pointer: works (`out=43`). String round trip through a
   `character(kind=c_char), target :: buf(max(len(s),1))` array + `c_loc(buf)`
   + `c_f_pointer(p, chars, [n])`: works, including the empty string.

## Emitter facts found

- `doc/src/codegen/v2/lang_fortran/{mod,types,functions,wrappers,managed,layout,makefile}.rs`.
  `mod.rs::generate` order: FFI types -> FFI `interface` block -> wrapper
  decls -> managed decls -> `contains` -> wrapper bodies -> managed bodies.
  `generator.rs:454-462` writes `azul.f90` + `Makefile.fortran`.
- The old wrapper layer was unusable by construction: factories such as
  `Dom_create_body` were never `public` (0 `public ::` lines for them; the
  module is `private` by default), methods took raw `type(AzString)` args and
  returned raw FFI types, and every wrapper carried a `final ::` finalizer.
  That is why `examples/fortran/hello_world.f90` used only the raw `az_*`
  layer with a hand-written `mk_str`, `c_f_pointer`, `azul_refany_create(c_loc(model))`,
  `azul_host_invoker_init()`, `wco%window_state%layout_callback = ...` and an
  `if (c_associated(arg1)) return` warning-silencer.
- The managed layer (`managed.rs`) stored `c_funptr`s from `c_funloc` of
  `bind(C)` user procedures with `type(c_ptr), value :: arg0, arg1, out_ptr`
  signatures; names were `azul_register_<kindlowercase>`, `azul_refany_create/get`,
  `azul_host_invoker_init`. Only `tests/memtest/mem_test.f90` (not wired into
  CI), the example, the guide and `examples/fortran/README.md` reference them.
- Shared IR helpers to derive everything from: `managed_host_invoker.rs`
  (`HOST_INVOKER_KINDS`, `host_invoker_kinds(ir)`, `wrapper_name(cb)`,
  `has_return(cb)`, `managed_c_symbol(func)` -> the `<c_name>Struct` symbol
  for functions with a callback-wrapper arg, `layout_callback_factory_info(s, ir)`
  -> `{default_c_name, callback_wrapper, field_path, field_types}` for
  `WindowCreateOptions`, `smart_callback_setter_info`, `to_snake_case`).
- Callback typedef IR args have EMPTY names (emitter synthesizes `arg0`, `arg1`)
  and `ref_kind = Owned`. Kinds' signatures: all take `(RefAny, <Info>[, state|usize])`,
  return `Update` / `Dom` (Layout) / `VirtualViewReturn` / `OnTextInputReturn` / void (Thread).
- 75 functions take a callback-WRAPPER struct arg (`with_on_click(self, data: RefAny,
  on_click: ButtonOnClickCallback)` etc.; these link the `Struct` C symbol);
  12 take a raw typedef (`WindowCreateOptions::create(layout_callback: LayoutCallbackType)`,
  `Dom::with_callback(.., CallbackType)`) -- the latter carry no host-handle
  ctx and cannot dispatch to a typed Fortran procedure; leave them `type(c_funptr)`
  except the layout-callback factory, which gets the smart `create(layout)`.
- api.json: no method named `delete`/`raw`/`owned`/`free`/`t`; `get` exists on 69
  classes (Vec types, `FloatValue`) but NOT on `RefAny`; `create` on 203 classes.
  Longest proc name 60 chars, longest snake wrapper name 52 (+`_t` fits 63).
  `String` = `{vec: U8Vec}`, `U8Vec = {ptr: *const u8, len, cap, destructor}`
  (Fortran field `len_` because `len` is sanitized); String constructor
  `copy_from_bytes(ptr: *const u8, start: usize, len: usize)`, static
  `from_utf8(ptr, len)`; `AzString_delete`, `AzString_clone` exist.
  `RefAny`: `AzRefAny_clone`/`_delete` exist (custom Clone/Drop), constructor `new_c` only.
  `App::run(&self, root_window: WindowCreateOptions)`; `App::create(initial_data: RefAny, app_config: AppConfig)`;
  `Dom::with_child/with_css(self by value) -> Dom`; `Button::create(label: String)`,
  `with_button_type(self, ButtonType)`, `with_on_click(self, data, on_click)`, `dom(self) -> Dom`.
  `WindowCreateOptions` has `_default` + `create(LayoutCallbackType)`; field path
  `window_state % layout_callback` (`AzFullWindowState`, `AzLayoutCallback`).
- Fortran bundle map (`api.json` `installation.languages.fortran.bundle`):
  `Makefile`, `azul.f90`, `hello_world.f90` only (no README ships). Install
  routes end in `make` (+ variants of LDFLAGS) then `./hello_world`; the
  design below changes neither, so no api.json edit / `normalize` needed.
- `azul-doc check derives` (`lint_derives.rs`, binding "fortran", prefix `Az`,
  ABI profile) scans `azul.f90` for the FFI names -- keep the FFI interface
  block untouched and it keeps passing. `lint_examples.rs` checks that
  `az_`-prefixed symbols used in examples/guides exist in `target/codegen`.
- lang_c tag rule (`lang_c.rs` ~505-540 monos, ~760-800 hand-written unions):
  `is_u8_repr = repr.contains("u8")` -> `uint8_t tag;` else `<Name>_Tag tag;`.
  `layout.rs::tag_layout(repr)` already mirrored this for hand-written unions;
  only `mono_layout` had the stale 4-byte assumption (13 `CssPropertyValue<Color>`
  blobs: Fortran 8 bytes vs C 5). `CGenerator` implements `LanguageGenerator`
  (`generator.rs`), so a test can call `CGenerator.generate_types(&ir, &CodegenConfig::c_header())`
  on a fixture IR and assert `uint8_t tag;` on the C side against
  `type_layout(..) == (5,1)` / `integer(c_int8_t) :: opaque_(5)` on the Fortran side.
  `lang_haskell/types.rs:1120` has fixture-IR builder helpers to copy.

## Design decided (not implemented)

Wrapper layer in `wrappers.rs` (rewrite), managed layer in `managed.rs`
(rewrite), `mod.rs` (`wrapper_type_name` -> `<snake>_t`, re-exports, docs).
Everything derived from the IR; no per-function lists.

- **Wrapper types** `<snake>_t` (`dom_t`, `button_t`, `app_t`, `ref_any_t`,
  `window_create_options_t`, `app_config_t`, ...) for every struct that
  passes `should_emit_wrapper` and has a `Delete` fn, EXCEPT the `String`
  class (`TypeCategory::String`) -- strings are Fortran `character`. The
  suffix exists because Fortran folds case: `type(Button) :: button` is illegal.
  Components PUBLIC: `type(AzX) :: raw`, `logical :: owned = .true.`.
  TBPs: `delete` + one per api.json method (`clone` for DeepCopy), static
  methods as `nopass`. No `final`.
- **Factories** (Constructor/Default) are PUBLIC module procedures
  `<snake>_<method>`: `dom_create_body()`, `dom_create_p_with_text(text)`,
  `button_create(label)`, `app_create(data, config)`, `app_config_create()`,
  `window_create_options_create(layout)`. All wrapper procedures public
  (procedural style allowed).
- **Argument mapping** (dummy decl / actual expr), per `UserType`:
  `Str` (String class): `character(len=*), intent(in)` / Owned ->
  `azul_string(x)` (temp AzString consumed by C); Ref -> local
  `type(AzString), target :: azul_tmp_x = azul_string(x)`, pass `c_loc`,
  `az_string_delete` after the call. `Wrapper(w)`: Owned -> `type(w_t), intent(in)`
  / `azul_take_<w>(x)` when the class has a `_clone` (returns `x%raw` if
  `x%owned`, else a clone -- this is how a borrowed callback `data` reaches
  `with_on_click` safely), else `x%raw`; Ref/Ptr -> `..., intent(in), target`
  / `c_loc(x%raw)`; RefMut/PtrMut -> `intent(inout), target`. `Kind(k)`
  (callback wrapper struct in HOST_INVOKER_KINDS, checked first): Owned ->
  `procedure(<snake k>_iface) :: x` / `azul_register_<snake k>(x)`. `Enum`
  (unit enum, incl. mono SimpleEnum): `integer, intent(in)` / `int(x, c_int)`.
  `Bool`: `logical, intent(in)` / `logical(x, c_bool)`. `Raw(fty)`: FFI
  spelling; Ref of a derived FFI type -> `target` + `c_loc(x)`; Ref of
  `type(c_ptr)` -> pass as is.
- **Receiver**: consumed (`args[0].ref_kind == Owned`) -> `class(w_t),
  intent(inout), target :: self`, actual `azul_take_<w>(self)`/`self%raw`;
  if the return is Self -> emit a SUBROUTINE that does `self%raw = <call>;
  self%owned = .true.` (`call label%with_css('font-size: 32px;')`); otherwise
  a function, then `self%owned = .false.`. `&self` -> `intent(in), target`,
  `&mut self` -> `intent(inout), target`, actual `c_loc(self%raw)`.
- **Returns**: wrapper -> `type(w_t) :: r; r%raw = ..; r%owned = .true.`;
  String -> `character(len=:), allocatable :: r = azul_string_value(<call>)`
  (copies `raw%vec%ptr[1..len_]` then `az_string_delete`s the temp); unit
  enum -> `integer :: r = int(..)`; bool -> `logical`; else FFI type.
  Public helpers `azul_string(s) -> type(AzString)` and
  `azul_string_value(az) -> character(:)` for raw-layer users.
- **Enum constants**: `integer, parameter, public :: ButtonType_Primary =
  AzButtonType_Primary` for every included unit enum variant (and mono
  simple enums); skip with a `! NOTE` on a case-folded collision with another
  public name.
- **Re-exports** after `private`: `public :: c_int, c_int8_t, c_int16_t,
  c_int32_t, c_int64_t, c_size_t, c_intptr_t, c_float, c_double, c_bool,
  c_char, c_ptr, c_funptr, c_null_ptr, c_null_funptr, c_loc, c_f_pointer,
  c_associated, c_funloc`.
- **Managed layer** (`managed.rs`), per kind `k` from `host_invoker_kinds(ir)`,
  `snake = to_snake_case(wrapper_name(cb))`:
  - abstract interface `<snake>_iface` derived from the typedef: wrapper-typed
    args `type(w_t), intent(inout)` (data AND info -- `&mut self` TBPs need
    inout and the user must match intents exactly), scalars `intent(in)`
    (`integer(c_size_t)` for usize, `integer` for enums, `logical` for bool),
    result per the return mapping (`type(dom_t)` for Layout, `integer` for
    Update). Bare `import`.
  - ONE handle table: `type :: azul_handle_t { integer(c_int64_t) :: id = 0;
    class(*), pointer :: object => null(); procedure(<snake>_iface), pointer,
    nopass :: <snake> => null()  (one component per kind) }`,
    `type(azul_handle_t), allocatable, save :: azul_handles(:)`,
    `azul_last_handle_id`, `azul_invokers_installed`. Ops: `azul_handle_new()`
    (grow via `move_alloc`, mint id, calls `azul_ensure_invokers()` which
    lazily installs the releaser + every per-kind invoker via `c_funloc` of
    the `bind(C)` module procedures -- so no `host_invoker_init` in user code),
    `azul_handle_slot(id)`, `azul_handle_release(id) bind(C)` (deallocates
    `object` if associated, removes the entry).
  - invoker `azul_<snake>_invoker(id, arg0.., [out_ptr]) bind(C)`: `slot =
    azul_handle_slot(id)`; copy the proc pointer to a local `fp` before
    calling (the callback may grow the table); unpack each `type(c_ptr)` arg
    via `c_f_pointer` into a borrowed wrapper (`a%raw = p; a%owned = .false.`)
    / scalar; call; write the result through `out_ptr` (`out = r%raw`,
    `int(r, c_int)`, `logical(r, c_bool)`, `azul_string(r)`) if associated.
  - `azul_register_<snake>(proc) -> type(Az<k>)`: new slot, `%<snake> => proc`,
    `az_<snake>_create_from_host_handle(id)`. FFI decls bound to
    `AzApp_set<k>Invoker`, `Az<k>_createFromHostHandle`, plus
    `AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`, `AzRefAny_getHostHandle`.
  - `ref_any_create(object)`: `class(*), intent(in)`; new slot;
    `allocate(azul_handles(slot)%object, source=object)` (the binding OWNS a
    copy, freed by the releaser when the last clone drops -- Haskell/Python
    semantics); `r%raw = az_ref_any_new_host_handle(id)`.
    TBP `get => ref_any_get` on `ref_any_t` (class found via
    `TypeCategory::RefAny`): returns `class(*), pointer` (null if unknown).
  - smart factory from `layout_callback_factory_info`: `window_create_options_create(layout)`
    = `_default()` then `r%raw%window_state%layout_callback =
    azul_register_layout_callback(layout)`; skip the raw factory whose single
    arg's `callback_info.callback_wrapper_name` matches.
  - Layout order in the spec part: wrapper types -> abstract interfaces
    (`import` the wrapper types) -> handle type/state -> FFI host-invoker
    interface block -> publics.
- **Target example** (`examples/fortran/hello_world.f90`, no comments, one
  blank line between top-level items), ~55 lines:

```fortran
module hello_impl
  use azul
  implicit none

  type :: t_model
    integer :: counter = 5
  end type t_model

contains

  function layout(data, info) result(body)
    type(ref_any_t), intent(inout) :: data
    type(layout_callback_info_t), intent(inout) :: info
    type(dom_t) :: body
    class(*), pointer :: model
    type(dom_t) :: label
    type(button_t) :: button
    character(len=16) :: text

    model => data%get()
    select type (model)
    type is (t_model)
      write (text, '(I0)') model%counter
    class default
      text = '?'
    end select

    label = dom_create_p_with_text(trim(text))
    call label%with_css('font-size: 32px;')

    button = button_create('Increase counter')
    call button%with_button_type(ButtonType_Primary)
    call button%with_on_click(data, on_click)

    body = dom_create_body()
    call body%with_child(label)
    call body%with_child(button%dom())
  end function layout

  function on_click(data, info) result(update)
    type(ref_any_t), intent(inout) :: data
    type(callback_info_t), intent(inout) :: info
    integer :: update
    class(*), pointer :: model

    model => data%get()
    select type (model)
    type is (t_model)
      model%counter = model%counter + 1
    end select
    update = Update_RefreshDom
  end function on_click

end module hello_impl

program hello_world
  use azul
  use hello_impl
  implicit none

  type(app_t) :: app
  type(window_create_options_t) :: window

  app = app_create(ref_any_create(t_model(5)), app_config_create())
  window = window_create_options_create(layout)
  call app%run(window)
end program hello_world
```

  Unused `info` dummies warn only under `-Wextra`, which the Makefile does
  not pass -> no silencing lines. The Makefile (`FFLAGS = -O2 -std=f2008
  -ffree-line-length-none -fimplicit-none`) stays as is.

- **Tag-width test** (to write in `layout.rs` `#[cfg(test)]`): fixture IR
  with `ColorU {r,g,b,a: u8}` and two mono aliases of a TaggedUnion
  `[Auto, None, Inherit, Initial, Exact(ColorU)]`, repr `"C, u8"` and `"C"`;
  assert `type_layout` = (5,1) and (8,4); assert the emitted Fortran blob
  decls `integer(c_int8_t) :: opaque_(5)` / `integer(c_int32_t) :: opaque_(2)`;
  assert `CGenerator.generate_types` spells `uint8_t tag;` for the u8 one and
  `<Name>_Tag tag;` for the other, so both emitters are pinned to one `repr`.

## Not done (nothing below was run; nothing is claimed)

- `wrappers.rs` / `managed.rs` / `mod.rs` rewrite; new `hello_world.f90`;
  `doc/guide/en/hello-world/fortran.md` (its embedded example is stale even
  against the OLD file: uses `create_div`/`create_span_with_text`);
  `examples/fortran/README.md`; `tests/memtest/mem_test.f90` update
  (`azul_host_invoker_init` -> nothing, `azul_refany_create(c_loc(x))` ->
  `ref_any_create(x)` / `app_create(...)`, `call the_app%delete()`).
- The tag-width unit test; `cargo test --release -p azul-doc`;
  `./target/release/azul-doc check`; `python3 scripts/preflight_contracts.py`;
  the e2e gate `--gate-shipped fortran`.
- Verifying the layout.rs change compiles (it is a two-line edit inside
  `mono_layout`: `repr: _` -> `repr`, `AbiLayout::new(4, 4)` ->
  `tag_layout(repr.as_deref())`).
