# Session handoff — 2026-05-12 evening

State at end of day. Tomorrow's agent: read this from top to bottom before doing anything else. Memory under `~/.claude/projects/-Users-fschutt-Development-azul/memory/` has the longer-form artifacts; this file is the punch list.

## What worked today (commits, in order)

```
744f1e90c codegen(lisp): tagged-union tag emits matching :uint<N>, not hard-coded :uint32
8234155a2 codegen(kotlin): per-module split + invoker dispatch — Kotlin E2E
f5a5c4a47 codegen(java): per-api.json-module AzulNative split
6e7cc7bd2 codegen(java): AzulNative via Native.register — Java E2E
d098cf92c codegen(java): C-ABI-correct struct layout (bool/enum/tag)
bd4217b66 codegen(csharp): make NativeMethods public so PowerShell can call FFI directly
48a70edde codegen(csharp): C-ABI-correct struct layout — C# E2E
616a9fd8a codegen(ocaml): C-ABI-correct tagged-union layout — OCaml E2E
```

Net gain: **Java, Kotlin, C#, OCaml** went from smoke-test to full-GUI E2E. **Lisp** got a codegen-correctness fix that unblocks struct round-trip (the runtime threading conflict remains).

## E2E status (counter 5→8 verified)

```
✓ C, C++, Rust, Python              — native paths
✓ Lua, Zig, Go, Node, Ruby           — already passed before today
✓ OCaml, C#, Java, Kotlin            — landed today
⊘ Pascal, Fortran, Perl, PHP        — smoke only; codegen invoker/wrapper gaps
⊘ Lisp                                — codegen correct; SBCL/macOS threading issue
⊘ PowerShell                          — pwsh CFRunLoop conflict (Windows only)
⊘ COBOL, Haskell, Smalltalk          — toolchain present; codegen unfinished
✗ Ada, Algol68, FreeBASIC, VB6, Scala — toolchain missing or dialect mismatch
```

Full per-language breakdown: `memory/language_audit_2026_05_12.md`.

## Architectural learnings (do not relitigate)

### 1. Module-system principle (added to BINDING_STRATEGY_PER_LANGUAGE.md today)

Every binding integrates with its host's native module system. **api.json already has 24 modules** and `ir.type_to_module` exposes them. Stop emitting monolithic `Azul.<everything>`; group by api.json module. Single-header C/C++ stays single-header by design.

The first concrete payoff: Kotlin's JNA Proxy `<clinit>` overflows the JVM's 64KB-per-method limit when one interface has ~1700 methods. Splitting per api.json module (largest is `vec` at ~888 methods) keeps every Proxy under cap. Java + Kotlin both use this now.

### 2. C-ABI tagged-union layout is a recurring bug

Bindings shipped today fixing this in 3 languages — OCaml, C#, Java. The shape:

- C side: `#[repr(C, u8)]` → 1-byte `tag` + padding + max-variant payload.
- Wrong binding: emits tag as `uint32_t` / `Java int` / `C# uint` / OCaml's `int` (4 bytes); reads garbage at offset 0..3.
- Fix: emit tag as the actual repr width (`uint8_t` / `byte`). Check `e.repr` from the IR.

Same class of bug bit Java's `boolean` (4-byte Win32 BOOL) and Kotlin's external-fun in companion objects. **Sanity check for any new binding**: compute `sizeof(AzAppConfig)`, `sizeof(AzWindowCreateOptions)`, `sizeof(AzFullWindowState)` in the host language and compare against the C-side numbers from a tiny C program against `target/codegen/azul.h`. If those don't match, the wrapper is wrong before you start.

### 3. Callback dispatch via host-invoker pattern is standard now

For every JVM/managed-FFI binding the pattern is:
1. Host registers a low-level `Az<Kind>InvokerCallback` (raw pointer signature) per callback kind, **once** at init.
2. libazul's per-kind static thunk calls the invoker with `(id, arg0, …, outPtr)`.
3. Invoker looks up `id` in a `_handles` `Map<u64, Object>`, casts to the matching SAM interface, calls `.invoke(...)`.
4. User registers their lambda via `register<Kind>Callback(fn)` which adds to `_handles`.

**Kotlin's invoker dispatch was a no-op placeholder** today — `synchronized(handles) { handles[id]?.let { /* user-side dispatch */ } }` literally discarded the user function. Fixing that (mirror Java) was half of the Kotlin E2E commit. If any other language is in the "smoke passes, layout never fires" state, **check the invoker body first**.

### 4. macOS event-loop conflicts are real and out of scope here

PowerShell + Lisp + likely Haskell all silently fail because their runtimes have already initialized `NSApplication`. libazul's `AzApp_run` can't co-host. This is **not fixable in any binding**; it's a libazul winit/objc2 work item (NSApp-aware run loop).

Both `powershell_macos_eventloop.md` and the Lisp note in `language_audit_2026_05_12.md` document this. Don't chase it from the binding side.

### 5. Joint Kotlin+Java compilation requires Gradle

I tried emitting AzulNative.java alongside Azul.kt; manual `kotlinc + javac` can't resolve the cross-dependencies (Kotlin needs Java's static-method names, Java needs Kotlin's struct classes). Gradle's `kotlin-jvm` plugin handles this — if anyone goes that direction, just use Gradle.

For now Kotlin is pure-Kotlin via the per-module split. Works fine.

## Big open item caught tonight

**libazul ARM64 SIGBUS** in `azul::desktop::shell2::common::event::PlatformWindow::dispatch_events_propagated::h9d926ba841d08043` at `event.rs:3120`. Looks like pointer-authentication failure on a corrupted function pointer (x20 holds `0xa9057bfdd10183ff` — high PAC bits set). Reproduces from the C hello-world clicking the button; will hit every other binding once they reach the click path.

This is **libazul-side**, not codegen. Tomorrow's agent should NOT spend cycles trying to fix it from the binding side; flag it to the libazul agent.

## Next-session priority list

Ordered by leverage / feasibility:

1. **Pascal invoker-stub dispatch.** Same shape as the Kotlin fix today. `lang_pascal/managed.rs` has empty `if id = 0 then ;` invoker bodies; fill in handle-lookup + dispatch + outPtr write. ~1 hour. **Caveat: Pascal's `procedure of object` calling convention — use `TMethod` records (Code+Data) for handle storage.**

2. **Fortran invoker-stub dispatch.** Same as Pascal; lang_fortran/managed.rs likely has the same placeholder. ~1 hour.

3. **Perl wrappers.** `lang_perl/wrappers.rs` is missing the DESTROY hook on wrapper packages, the smart WCO constructor, and per-method consume-on-move semantics. ~2 hours. Codegen change + regen + write/update `examples/perl/hello-world.pl`.

4. **AzString → host-string auto-conversion across all bindings.** Highest UX gain. Each binding adds **one method** to the AzString wrapper that decodes `vec.ptr` / `vec.len` as UTF-8. Templates per language in `memory/auto_conversion_audit.md`. ~30 min per binding × ~10 bindings.

5. **PHP `php-extension` route resume.** The Cargo feature needs `LIBCLANG_PATH` + `dynamic_lookup` RUSTFLAGS; halted by ENOSPC on `/private/tmp` previously. With today's `target/` cleanup we have 45GB free again — should build now.

6. **Scala example dir.** ~30 min if reusing per-module JVM bytecode from Java/Kotlin. Scala 3.8 + scalac is installed.

7. **Haskell App.Run wiring.** `lang_haskell` codegen has the smoke prelude but explicitly notes "Full App.run wiring requires C shim wrappers for struct-by-value returns". Bigger lift than Pascal/Fortran. ~half a day.

Items you should **NOT** spend cycles on without explicit user direction:

- C SIGBUS / event.rs:3120 — libazul-side, hand off
- PowerShell-macOS / Lisp App.Run — runtime conflict, libazul-side
- Ada / FreeBASIC / VB6 / Algol68 — toolchain not installable cleanly on macOS
- AzVec / AzOption / AzResult auto-conversion — 2-3 days of work; do AzString first to validate the approach

## Concurrent-agent rules (unchanged)

- Tomorrow's agent owns: `examples/<lang>/`, `scripts/`, `doc/src/codegen/v2/lang_<x>/` **except `lang_php_ext.rs`**.
- Other agent owns: `core/`, `layout/`, `dll/`, `doc/src/codegen/v2/{ir,generator,…}`, `lang_php_ext.rs`, all of `examples/c/hello-world.c`-style native reference files.
- On build conflict: sleep 60s and retry; don't fight.
- Never destructive git ops (no force-push, no reset --hard, no branch -D).

## Exit condition reminder (the only bar that counts)

A hello-world is "done" ONLY when:
```
DYLD_LIBRARY_PATH=. AZ_DEBUG=<port> ./hello-world &
curl -X POST localhost:<port> -d '{"op":"get_html_string"}'      # counter=5
curl -X POST localhost:<port> -d '{"op":"click","selector":".__azul-native-button"}' × 3
curl -X POST localhost:<port> -d '{"op":"get_html_string"}'      # counter=8
```

Anything less — smoke-test passing, window renders but counter doesn't increment, "alive but event-loop disconnected" — is NOT done.

## State of the working tree

Clean. `target/` was wiped tonight; fresh `target/release/libazul.dylib` exists from the rebuild. Every `examples/*/libazul.dylib` was refreshed.

End of handoff.
