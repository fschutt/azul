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
- [ ] **A.1.3 AzVec<T> → host iterable**. Element marshalling per type — IR has the element type. Returns native iterator/list/array:
  - Java: `implements Iterable<T>` + `Iterator<T>`
  - Kotlin: `operator fun iterator(): Iterator<T>`
  - C#: `IEnumerable<T> ToList()`
  - Ruby: `include Enumerable; def each ... end`
  - Node: `[Symbol.iterator]()`
  - OCaml: `let to_list self = ...`
  - Lua: `function _methods:to_lua_array()` returning a Lua table
- [ ] **A.1.4 AzResult<T,E>.unwrap() → throws/raises on Err**. Per-language:
  - Java/Kotlin: `T unwrap() throws AzulError`
  - C#: `T Unwrap()` throws
  - Ruby: `def unwrap; raise ... if err?; ok end`
  - Node: `unwrap() { if (...) throw ...; return this.ok; }`
  - OCaml: `let unwrap = function Ok v -> v | Err _ -> failwith ...`
  - Lua: `function _methods:unwrap() if self.is_err then error(...) else return self.ok end end`

### A.2 Enum constants instead of raw integers

Today many bindings return raw `1` and tag it with a comment `// Update.RefreshDom`. Need to surface unit-only enum variants as named constants:

- [ ] **A.2.1 Node:** `azul.Update.RefreshDom` etc. instead of `return 1; // Update.RefreshDom`.
- [ ] **A.2.2 Ruby:** `Azul::Update::RefreshDom`.
- [ ] **A.2.3 OCaml:** `Azul.Update.RefreshDom` (already partly there via the `Tag` constructor mapping).
- [ ] **A.2.4 Lua:** `azul.Update.RefreshDom`.
- [ ] **A.2.5 Audit Java/Kotlin/C#/Scala for raw-int returns** in hello-world; replace.

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

- [ ] **A.3.1 Java:** Static factory on `com.azul.WindowCreateOptions`.
- [ ] **A.3.2 Kotlin:** Companion-object factory.
- [ ] **A.3.3 C#:** Static `WindowCreateOptions.Create(...)` taking `Func<...>`.
- [ ] **A.3.4 Scala:** Same as Java (rides on Java's codegen).
- [ ] **A.3.5 Ruby:** `Azul::WindowCreateOptions.create { |data, info| ... }` (block).
- [ ] **A.3.6 Node:** `azul.WindowCreateOptions.create(fn)`.
- [ ] **A.3.7 OCaml:** `Azul.WindowCreateOptions.create ~layout:fn`.
- [ ] **A.3.8 Lua:** `azul.WindowCreateOptions.create(fn)`.

### A.4 Smart Button.on_click(refany, fn)

Same pattern, smaller scope. Codegen for `AzButton_withOnClick(button, refAny, cb)` detects callable arg, auto-registers, returns the modified Button wrapper.

- [ ] **A.4.1 Java/Kotlin/C#/Scala** — single shared codegen path (`lang_jvm/...`).
- [ ] **A.4.2 Ruby:** `btn.on_click(refany) { |data, info| ... }`.
- [ ] **A.4.3 Node:** `btn.onClick(refany, fn)`.
- [ ] **A.4.4 OCaml:** `Button.on_click ~data ~fn button`.
- [ ] **A.4.5 Lua:** `btn:on_click(refany, fn)`.

### A.5 Hide AzulHostInvoker entirely

After A.3 + A.4, no user code should need to mention `AzulHostInvoker`. Verify by grep:

- [ ] **A.5.1** `examples/java/HelloWorld.java` — no `AzulHostInvoker` references.
- [ ] **A.5.2** `examples/kotlin/HelloWorld.kt` — no `AzulHostInvoker` references.
- [ ] **A.5.3** `examples/csharp/hello-world.cs` — no `AzulHostInvoker` references.
- [ ] **A.5.4** `examples/scala/HelloWorld.scala` — no `AzulHostInvoker` references.
- [ ] **A.5.5** `examples/ruby/hello-world.rb` — no `Azul.refany_create` boilerplate visible (smart App.create handles it).
- [ ] **A.5.6** `examples/node/hello-world.js` — same.
- [ ] **A.5.7** `examples/ocaml/hello_world.ml` — same.
- [ ] **A.5.8** `examples/lua/hello-world.lua` — same.

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

- [ ] **A.7.1 Java** — rewrite under 50 lines.
- [ ] **A.7.2 Kotlin** — rewrite under 50 lines.
- [ ] **A.7.3 C#** — rewrite under 50 lines.
- [ ] **A.7.4 Scala** — rewrite under 50 lines.
- [ ] **A.7.5 Ruby** — rewrite under 50 lines.
- [ ] **A.7.6 Node** — rewrite under 50 lines.
- [ ] **A.7.7 OCaml** — rewrite under 50 lines.
- [ ] **A.7.8 Lua** — already idiomatic; verify ≤50 lines.
- [ ] **A.7.9 Go** — already idiomatic; verify.
- [ ] **A.7.10 Zig** — already idiomatic; verify.

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

- [ ] **D.1** Audit every language's `emit_tagged_union` for `repr(C, u8)` tag width: Pascal ✓ (commit 1f7f84a90), Java ✓ (d098cf92c), Kotlin ✓ (8234155a2), C# ✓ (48a70edde), OCaml ✓ (616a9fd8a), Lisp ✓ (744f1e90c). **Verify Node / Ruby / Lua / Go / Zig / Perl / Lisp / Fortran / Haskell / Smalltalk** all match.
- [ ] **D.2** Audit every language's `bool` → 1-byte mapping. Pascal ✓ (cbool → ByteBool). **Verify** Ruby / Node / Lua / Go / Zig / Perl / Lisp / Fortran / Haskell / Smalltalk all emit 1-byte bool in struct fields.
- [ ] **D.3** Audit every language for `DestructorOrClone` field inclusion. Pascal ✓, Java ✓, C# ✓. **Verify** Kotlin / OCaml / Ruby / Node / Lua / Go / Zig / Perl / Lisp / Fortran / Haskell / Smalltalk.
- [ ] **D.4** Per-module split for JVM langs at scale: Java ✓ (f5a5c4a47), Kotlin ✓ (8234155a2). C# — does it need it? Single-namespace today; check.
- [ ] **D.5** WriteBackCallback codegen plumbing — once the macro decision in C.3 lands.
- [ ] **D.6** ThreadCallback codegen plumbing — per Phase 5 in BINDING_STRATEGY_PER_LANGUAGE.md.

---

## Phase E — Verification / CI

- [ ] **E.1** `scripts/test_all_e2e.sh` — for each lang with an E2E example, build + start + AZ_DEBUG probe (5 → 8) + tear down. Exit non-zero on any failure. *(Skip langs marked `[⊘]` or `[—]`.)*
- [ ] **E.2** `scripts/probe_az_debug.sh <port>` — helper that posts the click sequence + parses HTML + asserts counter.
- [ ] **E.3** Memory: refresh `full_gui_examples_status.md` at session end with final E2E-passing count.
- [ ] **E.4** Memory: refresh `language_audit_2026_05_12.md` with per-language string/RefAny/iterator/option/result accessor presence.

---

## Phase F — Documentation

- [ ] **F.1** Per-binding README:
  - [ ] examples/java/README.md
  - [ ] examples/kotlin/README.md
  - [ ] examples/csharp/README.md
  - [ ] examples/scala/README.md
  - [ ] examples/ruby/README.md
  - [ ] examples/node/README.md
  - [ ] examples/ocaml/README.md
  - [ ] examples/lua/README.md
  - [ ] examples/go/README.md
  - [ ] examples/zig/README.md
  - [ ] examples/php/README.md (extension-tier)
  - [ ] examples/pascal/README.md (with libazul-blocker note)
  - [ ] examples/perl/README.md
  - [ ] examples/lisp/README.md (with blocker note)
  - [ ] examples/powershell/README.md (Windows-only)
  - [ ] examples/cobol/README.md
  - [ ] examples/fortran/README.md
  - [ ] examples/haskell/README.md (blocker)
  - [ ] examples/smalltalk/README.md (blocker)
- [ ] **F.2** Update `scripts/BINDING_STRATEGY_PER_LANGUAGE.md` — strike done items, update the status table.
- [ ] **F.3** Top-level `BINDINGS.md` overview — one paragraph per language, link to the example dir.

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
  - A.1.2 (Java/Kotlin/C#/Ruby AzOption + tag-width fix in Kotlin/Ruby/Node)

End of plan.
