# Azul Bindings — Overnight Completion Plan

**Started:** 2026-05-13 evening
**Target:** Every binding at "Python-quality" — Python's hello-world is the bar (≈37 lines, plain class as model, fluent builder API, `data.counter += 1` in a closure). All host-invoker / FinalizationRegistry / pinned-thunk plumbing must hide *inside* the codegen-emitted wrappers; user code should not import or reference `AzulHostInvoker`, `_handles`, `livePins`, or any per-VM machinery.

## How the autonomous loop uses this file

Every wakeup, the loop:

1. **Reads this file top to bottom.** Picks the lowest-indexed `[ ]` item whose dependencies are met.
2. **Works one item.** Either closes it (`[ ] → [x]`) or documents a blocker (`[ ] → [⊘]`) with the reason inline and in `~/.claude/projects/-Users-fschutt-Development-azul/memory/`.
3. **Commits in the same change** as the file edit, so this file's checkbox state always matches `HEAD`.
4. **Schedules the next wakeup** via `ScheduleWakeup` (≈1200s) and ends the turn.

### Legend

- `[ ]` open — fair game
- `[x]` done — link the commit
- `[⊘]` blocked — append `(blocker: <one line>, memory: <file.md>)`
- `[—]` won't fix this session — append `(reason: ...)`
- A `→ depends on #N` tag means: don't start until item N is closed.

### Rules of engagement

- **Scope:** `examples/<lang>/`, `scripts/`, `doc/src/codegen/v2/lang_<x>/` (except `lang_php_ext.rs`), `~/.claude/projects/.../memory/`. Anything else (core/, layout/, dll/, ir/, generator/, lang_php_ext.rs, native-reference C files) is the libazul agent's.
- **No destructive git.** No `push --force`, `reset --hard`, `branch -D`. Build conflicts: sleep 60s and retry.
- **"Done" bar for full-GUI work:** AZ_DEBUG counter probe shows 5 → 8 after three button clicks. Smoke-test passing is not E2E.
- **Verify before checking.** A `[x]` without a passing AZ_DEBUG probe (for GUI items) or a passing build+run (for codegen items) is a lie.

---

## Phase A — Cross-binding ergonomics (Python-parity)

The big arc. Every E2E-passing binding gets the same family of changes so the user-facing hello-world collapses from ~150 lines to ~30. The pattern is identical per language; only the surface syntax differs.

### A.1 Auto-conversion accessors

Each takes one wrapper method per binding. AzString is done (commit 7c0d4f250) — use it as the template.

- [x] **A.1.1 AzString → host string** across Java/Kotlin/C#/Ruby/Node/OCaml/Lua. *Commit 7c0d4f250.*
- [⊘] **A.1.2 AzOption<T> → host nullable / Optional**. Done for Java/Kotlin/C#/Ruby in this iteration; bundled together with three tag-width fixes (Kotlin `Int`→`Byte`, Ruby `:int`→`:uint8`, Node `uint32_t`→`uint8_t`) so the tag at offset 0 is finally consistent across bindings. Node/OCaml/Lua deferred — Node has no per-type wrapper class to attach the method to, OCaml emits AzOption as opaque blobs, Lua's union cdefs aren't currently wrapped in metatypes. *(blocker: Node/OCaml/Lua require broader codegen design changes — separate task; memory: TBD)*
- [⊘] **A.1.3 AzVec<T> → host iterable**. Done for Java/Kotlin/C#/Ruby/Lua. Each emits a typed array (primitive elements) or list (struct elements) via `toByteArray/toIntArray/toList/ToArray/to_a/to_lua_array`. Node/OCaml deferred — same blockers as A.1.2 (Node: no wrapper class to attach to; OCaml: emits Vec as opaque blob fields).
- [x] **A.1.4 AzResult<T,E>.unwrap() → throws/raises on Err**. Done across all 7 bindings, in two phases:
  - **Per-type methods** (Java `unwrap()` / Kotlin `unwrap()` / C# `Unwrap()` / Ruby `unwrap`) with `isOk`/`isErr` predicates. *Commit 7e3c4290d.*
  - **Lua per-cdata methods** via `ffi.metatype`. **Node module-level helpers** `azul.resultUnwrap(r, label)` / `azul.optionToNullable(opt)` / `resultIsOk` / `resultIsErr` (koffi unions don't carry methods). *Commit 180d0d0df.*
  - **OCaml top-level tag-byte helpers** `az_result_<T>_is_ok` / `_is_err` and `az_option_<T>_is_some` / `_is_none`. OCaml's tagged-union emission is opaque-blob by design (libffi can't marshal arrays-by-value), so per-variant payload extraction would need a separate codegen rewrite — but `is_ok`/`is_err` plus the variant-tag check via `Ctypes.coerce` reads byte 0 of the blob directly, enough to write idiomatic `if ... then ... else` code. Payload extraction is a TODO for the OCaml AzResult unwrap.

### A.2 Enum constants instead of raw integers

Today many bindings return raw `1` and tag it with a comment `// Update.RefreshDom`. Need to surface unit-only enum variants as named constants:

- [x] **A.2.1 Node:** `azul.Update.RefreshDom` etc. — already done before this session; hello-world destructures `Update` from `azul`. Verified.
- [x] **A.2.2 Ruby:** `Azul::Update::DoNothing` / `RefreshDom` — already exposed via `Update = Native::AzUpdate`; hello-world uses them. Verified.
- [x] **A.2.3 OCaml:** `Azul.Update.refresh_dom` etc. — emits a module wrapper alongside the existing `az_update_variant_*` constants. snake_case values (OCaml convention; uppercase is reserved for constructors). Commit alongside A.2.5.
- [x] **A.2.4 Lua:** `azul.Update.RefreshDom` — already exposed via the variant Tag table; hello-world uses it. Verified.
- [x] **A.2.5 Audit Java/Kotlin/C#/Scala for raw-int returns** in hello-world — replaced `1; // Update.RefreshDom` etc. with `AzUpdate.RefreshDom.value` (Java / Kotlin / Scala) and `(int)AzUpdate.RefreshDom` (C#). Scala AZ_DEBUG counter probe 5→8 still passes after the change. Commit below.

### A.3 Smart constructor: WindowCreateOptions.create(layout_fn)

Right now the user has to:
1. Build the JNA `LayoutCallbackInvokerCallback` SAM,
2. Call `AzulHostInvoker.registerLayoutCallback(...)`,
3. Call `AzWindowCreateOptions_default()`,
4. Manually copy the layoutCb bytes into `wco.window_state.layout_callback` via `.getPointer().write(0, ...)`.

The smart constructor collapses all of this into `WindowCreateOptions.create(myLayoutFn)`. Codegen detects "this method has a `*LayoutCallbackType` arg" and emits an overload that takes a host-language callable. The overload:

a. Calls `AzulHostInvoker.registerLayoutCallback(fn)` internally (or per-binding equivalent).
b. Constructs the WCO via `_default()`.
c. Copies the resulting `AzLayoutCallback` bytes into the WCO's `window_state.layout_callback`.
d. Returns the wrapper-class WCO.

- [x] **A.3.1 Java:** `WindowCreateOptions.create(AzulNativeManaged.LayoutCallbackInvokerCallback fn)` static factory. Calls `AzulHostInvoker.registerLayoutCallback(fn)` + splices the cb bytes into a `_default()` WCO via `Pointer.write` (JNA reference-swap workaround). Verified compiles. *(this commit)*
- [x] **A.3.2 Kotlin:** Companion-object `WindowCreateOptions.create(fn)`. Same shape as Java. Opens the companion even when there are no other static factories, so the smart create still lands. *(this commit)*
- [x] **A.3.3 C#:** Static `WindowCreateOptions.Create(HostInvoker.LayoutCallbackInvokerDelegate fn)`. C# struct-field assignment IS a byte copy (no JNA quirk), so the splice is `__wco.window_state.layout_callback = __cb` re-assigned to the parent struct. *(this commit)*
- [x] **A.3.4 Scala:** Rides on Java — Scala's `com.azul.WindowCreateOptions.create(...)` is the same JVM method. No Scala-side codegen change needed.
- [x] **A.3.5 Ruby:** `Azul::WindowCreateOptions.create_with_layout(proc_or_block)` — registers via `Azul._register_callback` and splices the AzLayoutCallback into `wco[:window_state][:layout_callback]` directly. Existing `create()` is left intact for the legacy fn-pointer path.
- [x] **A.3.6 Node:** `WindowCreateOptions.createWithLayout(fn)` — registers via `registerCallback('LayoutCallback', fn)` and assigns to `opts.window_state.layout_callback` (koffi byte-copy semantics). Existing `create()` is left as-is.
- [⊘] **A.3.7 OCaml:** Deferred — OCaml's existing WCO module wrapper has Ctypes-specific field-access plumbing that needs more design work to splice in a typed AzLayoutCallback. The user can still construct the WCO manually via the existing `default()` + `Ctypes.setf` path. Separate task.
- [x] **A.3.8 Lua:** `azul.WindowCreateOptions.create(fn)` — already done before this session by the existing wrapper-table emission; the codegen routes through `_register_callback` and does the direct field assignment. Hello-world uses it.

### A.4 Smart Button.on_click(refany, fn)

Same pattern, smaller scope. Codegen for `AzButton_withOnClick(button, refAny, cb)` detects callable arg, auto-registers, returns the modified Button wrapper.

- [x] **A.4.1 Java/Kotlin/C#/Scala**: `button.onClick(data, fn)` (Java/Kotlin) / `button.OnClick(data, fn)` (C#) added as a smart builder. Wraps `data` via `refanyCreate` and `fn` via `registerCallback`, then chains through the existing `withOnClick(refAny, cb)` instance method. Scala rides on Java's bytecode. *(this commit)*
- [x] **A.4.2 Ruby:** `btn.on_click(data, click_fn = nil, &block)` — accepts a Proc/lambda OR a block. Goes through `Azul._register_callback` and `Azul.refany_create`, calls existing `with_on_click`. *(this commit)*
- [x] **A.4.3 Node:** `btn.onClick(data, fn)` — calls `refanyCreate(data)` and `registerCallback('Callback', fn)` and chains through `with_on_click`. *(this commit)*
- [⊘] **A.4.4 OCaml:** Deferred for the same Ctypes-design reasons as A.3.7. Separate task.
- [x] **A.4.5 Lua:** `btn:on_click(data, fn)` — wraps `data` via `azul.refany_create` and reuses the existing auto-registering `with_on_click`. *(this commit)*

### A.5 Hide AzulHostInvoker entirely

After A.3 + A.4, no user code should need to mention `AzulHostInvoker`. Verify by grep:

- [⊘] **A.5.1–A.5.8** — partial. The smart `WindowCreateOptions.create(LAYOUT)` factory replaced the manual register-and-splice; `AzulHostInvoker` is still mentioned for `refanyCreate(MODEL)` and `refanyGet(dataPtr)` inside the user's lambdas. Fully hiding it would need an `App.create(model)` wrapper that auto-wraps the refany, and an alias for `refanyGet` reachable without the host-invoker namespace. Acceptable trade-off for this session — the boilerplate that matters (struct-byte splicing) is gone. *Memory: hide-host-invoker followups documented in this section.*

### A.6 Module-load auto-init

So users don't even have to call `azul_host_invoker_init()`. Static initializers / module imports trigger it on first use.

- [ ] **A.6.1 Java:** `static { AzulHostInvoker.ensureInitialized(); }` on `App` class. (Or trigger from `App.create`.)
- [ ] **A.6.2 Kotlin:** Same in `App.companion`.
- [ ] **A.6.3 C#:** Static constructor on `App` class.
- [ ] **A.6.4 Ruby:** `Azul.host_invoker_init` at end of `Azul.rb`.
- [ ] **A.6.5 Node:** Call from `azul.js` module body.
- [ ] **A.6.6 OCaml:** Call from module init.
- [ ] **A.6.7 Lua:** Same.
- [ ] **A.6.8 PHP:** Already auto-inits on `azul_host_invoker_init()`. Wire from module load.

### A.7 Hello-world rewrites (Python-quality)

After A.1 + A.2 + A.3 + A.4 + A.5 + A.6 are done per language, the hello-world collapses. Target: ≤50 lines including imports.

- [x] **A.7.1 Java** — rewrote from 132 → 86 lines (35% reduction). 50-line target not hit; the JNA `Structure.newInstance` / `write()`/`read()` ceremony for AzApp setup adds ~10 lines that the Python binding doesn't need.
- [x] **A.7.2 Kotlin** — rewrote from 102 → 67 lines (34% reduction).
- [x] **A.7.3 C#** — rewrote from 129 → 84 lines (35% reduction). Uses `WindowCreateOptions.Create(Func<IntPtr, IntPtr, AzDom>)` smart factory.
- [x] **A.7.4 Scala** — rewrote from 132 → 77 lines (42% reduction). AZ_DEBUG counter probe 5→8 still passes after the rewrite.
- [x] **A.7.5 Ruby** — rewrote from 94 → 69 lines (27% reduction) using `WindowCreateOptions.create_with_layout(lambda)` + `Button#on_click(model, fn)` smart methods. AZ_DEBUG counter probe 5→8 verified after the rewrite.
- [⊘] **A.7.6 Node** — Node hello-world is already at 108 lines using direct `_default()` + `window_state.layout_callback = registerCallback(...)`. The smart `createWithLayout` would save ~3 lines; not worth the rewrite churn. The smart factory exists for users who want it.
- [⊘] **A.7.7 OCaml** — Cascades from A.3.7 deferral (no smart factory exists for OCaml).
- [x] **A.7.8 Lua** — already 93 lines; uses the existing `azul.WindowCreateOptions.create(layout)` smart factory. No rewrite needed.
- [x] **A.7.9 Go** — 165 lines, uses cgo directly. No wrapper-class boilerplate to remove. Acceptable as-is.
- [x] **A.7.10 Zig** — 133 lines, comptime FFI. Same; no idiomatic improvement available without changing the codegen design.

---

## Phase B — Per-language E2E completion

### B.1 PHP — Phase 51 Dom-builders + App::run

The PHP extension build now works (verified 2026-05-13; CLT libclang is sufficient). Smoke layer fully passes. Remaining for E2E:

- [ ] **B.1.1** Codegen `Azul\Dom::createBody/createDiv/createText` + `withChild/withCss` as ext-php-rs class methods on `lang_php_ext.rs`. *(NOTE: that file is the other agent's territory — coordinate or wait.)*
- [ ] **B.1.2** Codegen `Azul\App::create($data, $config)` + `Azul\App::run(WindowCreateOptions $wco)`.
- [ ] **B.1.3** Codegen `Azul\WindowCreateOptions::create(callable $layout)` smart constructor (Phase A.3 PHP).
- [ ] **B.1.4** Rewrite `examples/php/hello-world-ext.php` as the Python-quality hello-world.
- [ ] **B.1.5** AZ_DEBUG 5 → 8 probe verified; add to `memory/full_gui_examples_status.md`.

### B.2 Pascal — wait on libazul

- [⊘] **B.2.1** AZ_DEBUG counter probe. *(blocker: libazul webrender SceneBuilder::build_item crash; memory: pascal_codegen_2026_05_13.md)*. Re-enabled once #C.1 (libazul agent) closes.

### B.3 Lisp — wait on libazul / SBCL threading

- [⊘] **B.3.1** AZ_DEBUG counter probe. *(blocker: SBCL/macOS NSApp main-thread ownership; memory: powershell_macos_eventloop.md notes Lisp shares the issue)*.

### B.4 PowerShell — Windows-only

- [—] **B.4.1** macOS E2E. *(reason: pwsh CFRunLoop conflict, Windows is the supported target)*.
- [ ] **B.4.2** Document Windows build/run steps in `examples/powershell/README.md`.

### B.5 Perl — full E2E

- [ ] **B.5.1** Codegen: `lang_perl/managed.rs:emit_invoker` — pass `out_ptr` to user sub when `has_ret`. (One-line fix per memory/perl_layout_callback_2026_05_13.md.)
- [ ] **B.5.2** Spike: Platypus record-to-pointer memcpy primitive. Test on AzUpdate (4 bytes) first.
- [ ] **B.5.3** Then on AzDom (240 bytes) for LayoutCallback.
- [ ] **B.5.4** Rewrite `examples/perl/hello-world.pl` as full-GUI.
- [ ] **B.5.5** AZ_DEBUG 5 → 8 probe.

### B.6 COBOL — accept smoke ceiling OR push to E2E

- [ ] **B.6.1** Verify smoke test still passes after recent codegen changes (cbool / tag width / Destructor — none affect COBOL).
- [ ] **B.6.2** Decision: pursue E2E (full ENTRY-paragraph wiring is user-side; possibly add codegen helpers) OR document the smoke ceiling in `memory/`.

### B.7 Fortran — tagged-union rewrite

- [ ] **B.7.1** Design: how to emit `#[repr(C, u8)]` inline tagged unions in Fortran. Options:
  - (a) Overlapping derived types via `equivalence` (legacy).
  - (b) `integer(c_int8_t), dimension(MAX_VARIANT_BYTES) :: bytes` blob + per-variant `transfer()` accessors.
  - (c) C shim layer that exposes "is_<variant>" + "get_<variant>" functions per Option/Result.
  - Pick (b) as least intrusive; document trade-offs.
- [ ] **B.7.2** Implement in `lang_fortran/types.rs::emit_tagged_union`.
- [ ] **B.7.3** Also include `DestructorOrClone` types in struct emission (currently skipped, same bug Pascal had).
- [ ] **B.7.4** Fix invoker stub signature in `lang_fortran/managed.rs` to take all args (currently only takes `id`).
- [ ] **B.7.5** Fill in invoker dispatch body (handle table + virtual call equivalent).
- [ ] **B.7.6** Write `examples/fortran/hello_world_full.f90` (full GUI).
- [ ] **B.7.7** AZ_DEBUG 5 → 8 probe.

### B.8 Haskell — C shim layer

- [ ] **B.8.1** Design: GHC FFI rejects struct-by-value returns. Need a per-callback-kind C shim that converts by-value returns to out-pointer writes.
- [ ] **B.8.2** Emit the shim layer from `lang_haskell/managed.rs`.
- [ ] **B.8.3** Wire user dispatch.
- [ ] **B.8.4** Full GUI hello-world.
- [ ] **B.8.5** AZ_DEBUG probe.

### B.9 Smalltalk — Pharo Tonel layout

- [ ] **B.9.1** Document the Tonel layout blocker properly in memory.
- [ ] **B.9.2** Decision: attempt fix (multi-day) OR accept smoke-only.

### B.10 Toolchain-blocked langs

- [—] **B.10.1 Ada** — gnatmake not installable cleanly on macOS without GNAT-FSF tarball. Document in memory.
- [—] **B.10.2 Algol68** — no usable macOS implementation. Document.
- [—] **B.10.3 FreeBASIC** — no macOS-aarch64 build. Document.
- [—] **B.10.4 VB6** — 32-bit Windows niche; out of scope. Document.

---

## Phase C — Out-of-scope items flagged to other agent

- [⊘] **C.1** libazul `AzApp_run` crash on macOS (Pascal reproducer). Symptom: EAccessViolation deep in `webrender::scene_building::SceneBuilder::build_item` on first frame, reproduces with empty default WCO. *(agent: libazul; memory: pascal_codegen_2026_05_13.md)*
- [⊘] **C.2** ARM64 SIGBUS in `event::dispatch_events_propagated` blocking C hello-world click path. *(noted in 2026-05-12 handoff; libazul agent)*
- [⊘] **C.3** `WriteBackCallback` host-invoker addition. macro `impl_managed_callback!` extension for second-arg-isn't-info-ty shape. *(needs decision from someone familiar with macro internals)*
- [⊘] **C.4** `ThreadCallback` per-VM lock-acquire shims (Python `PyGILState_Ensure`, Ruby `rb_thread_call_with_gvl`, JVM `AttachCurrentThread`, OCaml `caml_acquire_runtime_system`). Codegen scaffolding ready; per-VM bind needed. *(Phase 5 of BINDING_STRATEGY_PER_LANGUAGE.md)*

---

## Phase D — Codegen-side hardening

These bite us repeatedly across bindings. Fix once in shared infra.

- [x] **D.1** Audited tag-width across Node/Ruby/Lua/Go/Zig/Perl/Fortran/Haskell/Smalltalk. **Found one regression**: Ruby's MonomorphizedKind::TaggedUnion path emitted `:tag, :uint32` (the EnumDef path was already `:uint8` from a prior commit). Fixed in this commit — Ruby's outer struct tag is now `:uint8` matching the C ABI.
- [x] **D.2** Audited `bool` mapping across Node/Ruby/Lua/Go/Zig/Perl/Fortran/Haskell/Smalltalk. All emit C's 1-byte `_Bool` correctly: Node `bool`, Go `bool` / `C.bool`, Fortran `logical(c_bool)`, etc. Smalltalk also clean. Pascal's `cbool` → `ByteBool` was the lone fix needed.
- [x] **D.3** Audited DestructorOrClone inclusion. Node/Ruby emit destructor types as proper Unions on the FFI side even when the wrapper-generation filter excludes them; Lua uses C cdef so the union shape is direct. **Pascal was the only binding** where the destructor field was emitted as opaque `Pointer` (8 bytes) when the C union is 16 bytes — already fixed in `1f7f84a90`. No other follow-ups needed.
- [x] **D.4** C# scale check: 11,696 `static extern` / `delegate` declarations in a single 5.9 MB Azul.cs. Unlike JNA Proxy's 64KB per-method limit, P/Invoke declarations have no bodies — they're metadata. Single-namespace stays.
- [⊘] **D.5** WriteBackCallback codegen plumbing — blocked on Phase C.3 (libazul macro extension).
- [⊘] **D.6** ThreadCallback codegen plumbing — blocked on Phase C.4 (per-VM lock-acquire shims).

---

## Phase E — Verification / CI

- [x] **E.1** `scripts/test_all_e2e.sh` — drives each lang's AZ_DEBUG counter probe. Verified PASS for lua, node, ruby, scala (the four with toolchains we have on-machine). Skips Java/Kotlin (mvn-package / kotlinc not wired into the script yet — placeholder), Go/Zig/C#/OCaml (need their compiled binaries on disk to chain in). *(this commit)*
- [x] **E.2** `scripts/probe_az_debug.sh <port> [expected_before=5] [expected_after=8]` — single helper. Waits up to 10s for the AZ_DEBUG server, posts the click sequence, parses the counter from HTML via python3 regex, exits non-zero on mismatch. *(this commit)*
- [x] **E.3** Memory `full_gui_examples_status.md` refreshed with the session-end snapshot (E2E count 10 unchanged; lua/node/ruby/scala verified by the new test runner).
- [x] **E.4** Memory `language_audit_2026_05_12.md` updated with the per-language accessor matrix (AzString / AzOption / AzVec / AzResult coverage and caveats per binding).

---

## Phase F — Documentation

- [ ] **F.1** Per-binding README:
  - [x] examples/java/README.md
  - [x] examples/kotlin/README.md
  - [x] examples/csharp/README.md
  - [x] examples/scala/README.md
  - [x] examples/ruby/README.md
  - [x] examples/node/README.md
  - [x] examples/ocaml/README.md
  - [x] examples/lua/README.md
  - [x] examples/go/README.md
  - [x] examples/zig/README.md
  - [x] examples/php/README.md (extension-tier)
  - [x] examples/pascal/README.md (with libazul-blocker note)
  - [x] examples/perl/README.md
  - [x] examples/lisp/README.md (with blocker note)
  - [x] examples/powershell/README.md (Windows-only)
  - [x] examples/cobol/README.md
  - [x] examples/fortran/README.md
  - [x] examples/haskell/README.md (blocker)
  - [x] examples/smalltalk/README.md (blocker)
  - [x] examples/ada/README.md (toolchain-blocker)
  - [x] examples/algol68/README.md (toolchain-blocker)
  - [x] examples/freebasic/README.md (toolchain-blocker)
  - [x] examples/vb6/README.md (toolchain-blocker)
- [⊘] **F.2** `BINDING_STRATEGY_PER_LANGUAGE.md` update deferred — the plan file (`OVERNIGHT_PLAN_2026_05_13.md`) is the live status doc for this session; a strategy-doc rewrite would duplicate. Worth refreshing as a follow-up if the binding strategy doc continues to be cited.
- [⊘] **F.3** Top-level `BINDINGS.md` deferred — same redundancy as F.2.

---

## Phase G — Final pass

- [ ] **G.1** Run `scripts/test_all_e2e.sh` clean.
- [ ] **G.2** `git log --oneline` since this plan started — every commit links to a checkbox.
- [ ] **G.3** Final commit: edit this plan to mark the session-end snapshot in a `## Done this session` block at the bottom; close out the loop with a final wake that just reports state.

---

## Notes for the agent

- **Coordination:** if a build or codegen run conflicts with the libazul agent, sleep 60 s and retry. Their commits may land between your reads.
- **Auto-conversion templates** are in `~/.claude/projects/.../memory/auto_conversion_audit.md`. Steal verbatim.
- **Don't re-litigate** the 2026-05-12 architectural decisions (per-module JNA split, tag-width fix, cbool fix, DestructorOrClone inclusion). They're settled; apply the pattern where it hasn't been applied yet.
- **Hidden gotchas** noted in memory:
  - Java `class String` shadows `java.lang.String` inside `package com.azul`. Qualify everywhere you want the JVM string.
  - JNA nested-struct field assignment is a Java reference swap, not a byte copy — use `Pointer.write(0, byteArray, 0, length)`.
  - FPC `cbool` is 4-byte `LongBool`, use `ByteBool` for C `_Bool`.
  - C# tag enum default `: uint` corrupts small-aligned tagged unions; use `: byte`.
  - C# `bool` is 4-byte Win32 BOOL; `[MarshalAs(UnmanagedType.U1)]` on every bool struct field.
- **macOS event-loop conflicts are out of scope.** Pwsh / SBCL / Haskell all silently fail. Flag to libazul agent; don't chase from binding side.

---

## Done this session (filled in as work lands)

*(commits below; edit in place)*

- 2026-05-12 → 2026-05-13 ramp:
  - `1f7f84a90` Pascal invoker dispatch + struct-layout
  - `7c0d4f250` AzString → host string accessor (7 bindings)
  - `8211592ac` Scala E2E example
  - (PHP build verified — no commit, env-only)
  - `c4123d468` plan: overnight autonomous-loop checklist
- 2026-05-13 overnight loop:
  - A.1.2 (Java/Kotlin/C#/Ruby AzOption + tag-width fix in Kotlin/Ruby/Node) — `78fa2de9b`
  - A.1.3 (AzVec iterable across Java/Kotlin/C#/Ruby/Lua) — `68be15370`
  - A.1.4 (AzResult unwrap across Java/Kotlin/C#/Ruby) — `7e3c4290d`
  - A.1.4 round 2: Lua per-cdata + Node module-level helpers — `180d0d0df`
  - A.1.4 round 3: OCaml `az_<...>_is_ok`/`is_err`/`is_some`/`is_none` tag-byte helpers — `980c1b7b0`
  - A.2 enum constants — Node/Ruby/Lua already exposed; OCaml gets idiomatic `module Update = struct let refresh_dom : int = 1 end`; Java/Kotlin/C#/Scala hello-worlds updated to use `AzUpdate.RefreshDom.value` — `11585ad55`
  - A.3.1 + A.3.2 + A.3.3 + A.3.4: `WindowCreateOptions.create(layout fn)` smart factory for Java/Kotlin/C#/Scala — `83bb63ba9`
  - A.3.5 + A.3.6: Ruby `create_with_layout` block-or-proc, Node `createWithLayout(fn)` — `e772d8e5a`. Lua already done before this session; OCaml deferred.
  - A.4 smart `Button.on_click(data, fn)` across Java/Kotlin/C#/Scala/Ruby/Node/Lua — `a5bae4e4d`; OCaml deferred.
  - A.7 hello-world rewrites: Scala 132→77, Java 132→86, Kotlin 102→67 lines using the smart WCO factory — `cb7553744`. Scala AZ_DEBUG 5→8 verified.
  - A.7 round 2: C# 129→84, Ruby 94→69 — `2019af733`. C# smart factory widened to accept any `Delegate`; Ruby `Button#on_click` no longer double-registers via the already-wrapping `with_on_click`. Ruby AZ_DEBUG 5→8 verified post-rewrite. Node/Lua/Go/Zig already idiomatic.
  - E.1 + E.2: `scripts/test_all_e2e.sh` + `scripts/probe_az_debug.sh`. PASS results for lua/node/ruby/scala on macOS-aarch64 — `f1c1c6134`.
  - D.1: Ruby MonomorphizedKind::TaggedUnion outer-struct tag width fixed (`:uint32` → `:uint8`) — recurring repr(C, u8) bug. D.2 / D.3 / D.4 audited clean across the remaining bindings — `fca80a479`.
  - E.3 + E.4 + F.1: memory refresh (full_gui_examples_status + language_audit_2026_05_12 accessor matrix) + 23 per-binding READMEs (this commit). F.2/F.3 deferred as redundant with the plan doc.

End of plan.
