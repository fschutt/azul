# Memory-safety audit: Haskell / Perl / Pascal / Go / Zig / Fortran / Smalltalk / COBOL / PHP codegens

**Date:** 2026-05-15
**Scope:** Followup to commits `62094b885` (JVM/CLR consume-after-by-value) and `75a1fbcd2` (JVM/CLR Option/Result heap leak). Audits whether the same two latent bug classes (and close cousins) exist in the nine "niche" host-side codegens. **Read-only audit; no code modified.**

**Key files inspected**

- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_haskell/{wrappers,functions,types,cshim,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_perl/{wrappers,managed,types,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_pascal/{wrappers,managed,types,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_go/{wrappers,types,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_zig/{wrappers,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_fortran/{wrappers,managed,types,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_smalltalk/{wrappers,types,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_cobol/{wrappers,managed,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_php/{wrappers,managed,mod}.rs`

**Verdict at a glance**

| Lang      | Finalizer mechanism | Consume after by-value (self)         | Consume after by-value (wrapper arg)   | Option/Result outer-free | Vec→iterable     | Smoke-only?            |
|-----------|---------------------|---------------------------------------|----------------------------------------|--------------------------|------------------|------------------------|
| Haskell   | `bracket` + `dispose<X>` | **MISSING** (bracket always disposes) | n/a (user passes raw ptrs manually)    | **NO** (no idiom)        | borrowed peek    | full GUI               |
| Perl      | `DESTROY` blessed-ref | **MISSING**                          | **MISSING**                            | **NO** (no idiom)        | n/a              | smoke (E2E blocker)    |
| Pascal    | `destructor Destroy; override;` + `FOwned` flag | **MISSING** (FOwned never cleared) | **MISSING**                            | **NO** (no idiom)        | n/a              | smoke (libazul crash)  |
| Go        | `runtime.SetFinalizer` + `Close()` | **MISSING**                | **MISSING**                            | **NO** (no idiom)        | n/a              | full GUI               |
| Zig       | manual `defer x.deinit()` | **MISSING (+ compile error)**       | **MISSING**                            | **NO** (no idiom)        | n/a              | full GUI               |
| Fortran   | F2003 `final ::` + `owned` flag | **MISSING (also factory-return double-free)** | **MISSING**                | **NO** (no idiom)        | n/a              | smoke (B.6 ceiling)    |
| Smalltalk | `WeakRegistry` + `finalize` | **MISSING**                       | **MISSING**                            | **NO** (no idiom)        | n/a              | smoke (B.9 ceiling)    |
| COBOL     | none (user manual)  | n/a                                   | n/a                                    | n/a                      | n/a              | smoke (B.6 ceiling)    |
| PHP       | `__destruct()`      | **MISSING**                          | **MISSING**                            | only payload borrow      | n/a              | smoke (B.1.3 blocker)  |

Eight of nine generators emit a deferred finalizer of some kind. **Every one of them is missing the `__consume`/`closed = true`/`FOwned := False`/`runtime.SetFinalizer(self, nil)` pattern from commit `62094b885` on by-value-consuming call sites.** This is the same latent double-drop the JVM/CLR commit fixed; the JVM/CLR fix was not ported.

Pascal additionally has a transitive form of the bug: every wrapper holds the FFI record `FRaw` *by value*, so the record bytes are copied at every assignment, but the inner pointers (Vec.ptr buffers, RefAny.handle, etc.) are *shared* with whoever Rust handed it to. The first delete frees the inner allocations; the second delete double-drops.

None of the nine emits the Option/Result extract-then-delete pattern. Where the language carries Option/Result types (typically as variant records / cdata / structs), the user must call `_delete` themselves; the wrapper-side getters that crack the payload either return a borrow into the outer struct's memory (potential dangling) or copy-by-value the bytes-but-not-the-inner-allocations (per-call heap leak of the inner buffer + tag byte).

---

## 1. HASKELL

**Finalizer mechanism:** `Control.Exception.bracket` pairs `pure (Foo raw)` with `\h -> FFI.c_AzFoo_delete (unFoo h)`. There is NO `ForeignPtr`/`newForeignPtr` registration — the `withFoo` smart constructor unconditionally invokes `dispose<Foo>` on scope exit. The `disposeFoo` helper is also exported for explicit early disposal.

**Code refs:**
- `doc/src/codegen/v2/lang_haskell/wrappers.rs:232-276` (`emit_bracket_constructor`) — the bracket pattern with unconditional `_delete`.
- `doc/src/codegen/v2/lang_haskell/wrappers.rs:278-292` (`emit_dispose`) — `disposeFoo h = FFI.c_AzFoo_delete (unFoo h)`.

### Issues found

**1.1 — Consume-after-by-value: critical (double-free).**
The user passes ownership manually by calling `unFoo h` and handing the `Ptr T.Foo` to an FFI symbol. If the C-ABI function takes the struct by value (DeepCopy-shaped `_with*` constructors; owned-by-value args), Rust drops the bytes; the surrounding `bracket` will still call `disposeFoo h` (=`c_AzFoo_delete (unFoo h)`) on scope exit. **Double-free on every `withFoo` / `_with*` chain that the user wraps in `bracket`.**

Severity: critical. The Haskell wrapper layer has no notion of "consumed" and no way to suppress the bracket's cleanup. The fix is either (a) emit per-method bracket variants whose release form is a no-op when self/args are consumed, or (b) introduce a `consume :: Foo -> IO ()` helper that pokes a tombstone into a mutable `IORef Bool` cell on the wrapper, with the bracket release checking the tombstone (mirrors the Pascal/Java pattern).

Code ref: `doc/src/codegen/v2/lang_haskell/wrappers.rs:269-274` — the `with<X> raw action = bracket (pure (X raw)) (\h -> c_Az<X>_delete (unX h)) action` line is unconditional.

**1.2 — Option/Result outer struct: major (per-call heap leak).**
Phase H.4/H.5 emits `<x>IsNone`/`<x>IsSome` / `<x>IsOk`/`<x>IsErr` tag accessors at `types.rs:750-810`, but no payload extractor that frees the outer Option/Result after decoding. Users who care about the payload either reach into the Storable instance (which copies the bytes of the variant record but leaves the inner Vec.ptr / Dom.styled-node-vec / etc. heap allocations unfreed) or call `dispose<Foo>` on the outer Option themselves. The codegen does not document the latter.

Severity: major (every Option/Result-returning FFI call leaks its inner payload's heap unless the user manually disposes).

Code ref: `doc/src/codegen/v2/lang_haskell/types.rs:693-810` — only `IsNone`/`IsSome`/`IsOk`/`IsErr` tag readers are emitted; no `<x>ToMaybe` / `<x>ToEither` decoder that runs `c_AzOption*_delete` afterwards.

**1.3 — Vec → list helper produces borrowed elements: minor (use-after-free on element keep-alive).**
`types.rs:505-537` emits `<vec>ToList :: AzVec -> IO [Elem]` that runs `mapM (peekElemOff __p) [0..n-1]`. Since the elements peek into the live Vec buffer, they are independent value-level Haskell records — but if the element type itself owns heap (e.g. a `Dom` inside a `DomVec`, an inner `RefAny`, an inner `AzString.Vec.ptr` buffer), only the outer struct is copied. After the Vec is disposed, every returned element's inner pointers dangle. Not all Vec element types own inner heap, so this only triggers for the recursive-payload Vecs (`DomVec`, `StyledNodeVec`, ...).

Severity: minor. The peek-and-keep pattern is fine for POD-element Vecs but quietly broken for the heap-owning element types.

Code ref: `doc/src/codegen/v2/lang_haskell/types.rs:534` — `mapM (peekElemOff __p) [0 .. __n - 1]`.

---

## 2. PERL

**Finalizer mechanism:** Perl `DESTROY` method on a blessed scalar reference. The wrapper class stores the underlying opaque pointer in `$$self` and `DESTROY` runs `Azul::FFI::AzFoo_delete($$self)` when refcount hits 0.

**Code refs:**
- `doc/src/codegen/v2/lang_perl/wrappers.rs:116-127` (`emit_class_wrapper`) — the DESTROY method.
- `doc/src/codegen/v2/lang_perl/wrappers.rs:97-104` — the bless-scalar-ref constructor stores the pointer.

### Issues found

**2.1 — Consume-after-by-value: critical (double-free).**
After a by-value-consuming C ABI call, `$$self` still holds the same pointer; Perl's deterministic refcount-driven `DESTROY` will later call `AzFoo_delete` on the now-Rust-owned bytes. The codegen never undefines `$$self` (no `undef $$self` or equivalent guard) and never undefines the finalizer. **Double-free on every `with_*` chain and on every owned-wrapper arg.**

Severity: critical.

Code ref: `doc/src/codegen/v2/lang_perl/wrappers.rs:209-243` (`emit_method`) — the call-site emission. Receiver `$$self` is passed verbatim; no consume-side mark.

The Ruby equivalent (`Azul._consume`) sets `@ptr = nil` and calls `ObjectSpace.undefine_finalizer(val)`. The Perl analogue would be `$$self = undef` (the `DESTROY` already short-circuits on `defined $$self`) — but the codegen never emits it.

**2.2 — Option/Result outer struct: no idiomization (n/a as latent leak).**
Perl emits Option/Result as opaque-payload records (`types.rs:243`: `'uint8[64]' => 'payload'`). No payload extraction wrapper, so the user reaches into raw FFI memory if they want the value. No leak from the codegen itself; users either call `_delete` themselves (no leak) or borrow into the payload bytes (no extra heap leak — the bytes are inline, but the inner heap allocations belong to the outer Option which the user must delete).

Severity: n/a — manual user code path is correct; no auto-leak.

**2.3 — Smoke-only context (per memory notes):**
Per `perl_layout_callback_2026_05_13.md`, the layout callback path is blocked end-to-end. The codegen-emitted double-free in 2.1 will surface as soon as any `with_*` / owned-by-value wrapper-arg call works end-to-end.

---

## 3. PASCAL

**Finalizer mechanism:** `destructor Destroy; override;` calls `<Type>_delete(@FRaw)` guarded by `if FOwned then`. `FOwned: Boolean` defaults to `True` on construction. The wrapper class holds the FFI record **by value** in `FRaw: TAzFoo` (record is embedded, not heap-allocated).

**Code refs:**
- `doc/src/codegen/v2/lang_pascal/wrappers.rs:132-138` — `FRaw: TAzFoo; FOwned: Boolean;` storage.
- `doc/src/codegen/v2/lang_pascal/wrappers.rs:282-290` — destructor `if FOwned then <Type>_delete(@FRaw)`.

### Issues found

**3.1 — Consume-after-by-value: critical (double-free).**
`FOwned` is never reset to `False` anywhere in the codegen. Every constructor sets `FOwned := True` (`wrappers.rs:268,364`). No `FOwned := False` clause after a self-by-value or owned-wrapper-arg call. The destructor will dutifully call `_delete(@FRaw)` even though Rust already owns those bytes — **critical double-free.**

The framework is half-built: `FOwned` exists, the destructor checks it, but the consume side never flips it. The fix is one line per consume site (`Self.FOwned := False;` after the `func.c_name(...)` call for self-by-value, equivalent for arg-by-value).

Severity: critical.

Code refs:
- `doc/src/codegen/v2/lang_pascal/wrappers.rs:417-441` (`emit_method_impl`) — detects `self_by_value` at line 424 and passes `FRaw` (by value) at line 430; never flips `FOwned`.
- `doc/src/codegen/v2/lang_pascal/wrappers.rs:436` — wrapper-class args are passed by their raw identifier; no consume-side mark.

**3.2 — FRaw-by-value embedded record: critical (transitive double-free).**
Beyond 3.1: every Pascal assignment of a `TFoo` wrapper class copies the *pointer to the object*, but every Pascal record-by-value parameter pass copies the *bytes* of `FRaw`. The bytes contain pointers (Vec.ptr, RefAny.handle, ...) that are shared with the original allocation site. If a user writes `myDom := TDom.Wrap(otherDom.Raw)` they now have two wrappers with `FOwned := True` over the same heap allocations. Both destructors will fire.

Severity: critical (but separate from the consume-after-by-value issue — this one needs an `ARaw: PAzFoo` (pointer) wrap signature or a reference-counted record).

Code ref: `doc/src/codegen/v2/lang_pascal/wrappers.rs:259-270` — `constructor Wrap(const ARaw: TAzFoo); FRaw := ARaw; FOwned := True;` copies bytes, sets owned.

**3.3 — Wrapper-class args not auto-unwrapped: compile error, not memory bug.**
`emit_method_impl` passes user args by their identifier (`sanitize_identifier(&a.name)` at line 436). If the IR arg type is a wrapper-class-eligible struct (e.g. `child: TDom`), the parameter type the method declares is `child: TAzDom` (record). The user must pass `myChildWrapper.Raw`. This is documented at line 451 ("users can construct the wrapper class manually") but means the codegen produces no `with_*` chains directly. Same shape gap as Go.

Severity: minor / scope (not a memory bug; user-side gap).

**3.4 — Option/Result: no idiomization.**
Pascal emits Option/Result as `record case Tag of ... end;` variant records — no payload extraction wrapper. Users access fields directly. No codegen-introduced leak; the user must call the matching `_delete` themselves.

Severity: n/a.

**3.5 — Smoke-only context:**
Per `libazul_resize_crash_2026_05_13.md` and `pascal_codegen_2026_05_13.md`, the full-GUI path is blocked in libazul; the wrapper class is currently only exercised in smoke tests. Issues 3.1/3.2 will surface once the libazul side resumes.

---

## 4. GO

**Finalizer mechanism:** `runtime.SetFinalizer(self, func(x *Foo) { x.Close() })` registered at `NewFoo`. `Close()` calls `C.AzFoo_delete(&self.inner)` then `runtime.SetFinalizer(self, nil)`. Idempotent (early `if self == nil { return nil }`).

**Code refs:**
- `doc/src/codegen/v2/lang_go/wrappers.rs:299-306` — `runtime.SetFinalizer(self, …)` registered in static factories.
- `doc/src/codegen/v2/lang_go/wrappers.rs:229-241` — `Close()` clears finalizer.

### Issues found

**4.1 — Consume-after-by-value: critical (double-free).**
`emit_instance_method` detects `self_by_value` at line 379-388 and passes `self.inner` (struct value) when the IR signature takes self by value, but never clears the finalizer afterwards. The user's `defer x.Close()` (or the GC's eventual finalizer fire) will then call `C.AzFoo_delete(&self.inner)` on stale bytes.

Severity: critical. The fix is one line after the call: `runtime.SetFinalizer(self, nil)` (matches the existing `Close()` cleanup).

Code ref: `doc/src/codegen/v2/lang_go/wrappers.rs:336-413` (`emit_instance_method`) — no SetFinalizer-nil emit after the call for self-by-value.

**4.2 — Owned-by-value wrapper args: critical (double-free).**
Same shape as 4.1 for owned wrapper-typed args (e.g. `func.c_name(self.inner, child.inner)` where `child` is another `*Dom`). The codegen passes the identifier name directly at line 466 (`format_call_args`) — it doesn't even unwrap `child.inner`, so the code won't compile until the user manually unwraps; but if they do, the child's finalizer will double-free.

Severity: critical (once 4.4 is fixed).

**4.3 — Instance methods that return Self: major (leak — no finalizer registered on returned wrapper).**
`emit_instance_method` builds `ret := &Foo{ inner: call }` at line 401 but does NOT call `runtime.SetFinalizer(ret, ...)`. Returned wrappers from `with_*` / DeepCopy / etc. have no finalizer at all. Either the user explicitly closes them (rare; the codegen doesn't tell them to) or the underlying allocation leaks until process exit.

Severity: major. Mirrors the `emit_static_factory` SetFinalizer block at line 297-307.

Code ref: `doc/src/codegen/v2/lang_go/wrappers.rs:398-403` — the `ret := &Foo{ inner: ... }` followed immediately by `return ret`; no finalizer registration.

**4.4 — Wrapper-class args not auto-unwrapped: compile error.**
`format_call_args` at line 450-469 emits user args by their identifier (`sanitize_identifier(&a.name)`). Users have to manually pass `child.inner`. Not a memory bug per se but blocks 4.2 from triggering in practice today.

Severity: minor / scope.

**4.5 — Option/Result, Vec: no idiomization, no codegen-introduced leak.**

---

## 5. ZIG

**Finalizer mechanism:** Manual `defer x.deinit();`. `deinit(self: *Self)` calls `C.AzFoo_delete(&self.inner)`. Zig has no GC and no destructor.

**Code refs:**
- `doc/src/codegen/v2/lang_zig/wrappers.rs:264-270` — `pub fn deinit(self: *Self) void { C.AzFoo_delete(&self.inner); }`.

### Issues found

**5.1 — Consume-after-by-value: critical (double-free) AND compile error.**
`emit_instance_method` always passes `&self.inner` (line 384). For a C-ABI method with `self_by_value` (`AzFoo_withChild(AzFoo, AzDom)`), the cgo / `@cImport` declaration is `extern fn AzFoo_withChild(AzFoo, AzDom) AzFoo`. Passing `&self.inner` (a `*AzFoo`) where a `AzFoo` (struct value) is expected will fail to compile in Zig (no implicit deref). When the user works around it by writing `self.inner` manually, the `defer self.deinit()` they wrote earlier will then call `_delete` on stale Rust-owned bytes.

Severity: critical (two problems compounded: today's emit is uncompilable for self-by-value methods; once fixed, the deferred `deinit` becomes a double-free).

Code ref: `doc/src/codegen/v2/lang_zig/wrappers.rs:383-388` — the always-`&self.inner` line. No `self_by_value` branch.

**5.2 — Vec, Option/Result: no idiomization.**
Zig users access C.AzVec_* / C.AzOption_* / C.AzResult_* directly. No codegen-introduced leak, but no help either.

**5.3 — No "consumed" sentinel:**
Zig has no `closed` flag analogous to the JVM/CLR pattern. The fix would be a `consumed: bool = false` field on the wrapper struct, set by the codegen at consume sites, checked in `deinit`. The user's existing `defer x.deinit()` pattern would then be safe.

Severity: critical (no current safety net).

---

## 6. FORTRAN

**Finalizer mechanism:** F2003 `final ::` type-bound subroutine. Each wrapper has `type({ffi}) :: raw` and `logical :: owned = .true.`; the finalizer's body is `if (self%owned) then call AzFoo_delete(c_loc(self%raw)); self%owned = .false.; end if`.

**Code refs:**
- `doc/src/codegen/v2/lang_fortran/wrappers.rs:139-148` — `type({ffi}) :: raw; logical :: owned = .true.;`.
- `doc/src/codegen/v2/lang_fortran/wrappers.rs:215-228` — the finalizer body.

### Issues found

**6.1 — Consume-after-by-value: critical (double-free).**
Same shape as Pascal. `emit_method_body` detects `self_by_value` at line 417-422 and passes `self%raw` (by value) at line 426, but never flips `self%owned = .false.` afterwards. The finalizer at line 220 will then call `AzFoo_delete(c_loc(self%raw))` on stale bytes.

The framework is half-built — `owned` exists, the finalizer checks it, but the consume side never flips it. Fix is one line per consume site (analogous to Pascal).

Severity: critical.

Code ref: `doc/src/codegen/v2/lang_fortran/wrappers.rs:415-440` — call args built without an `owned = .false.` writeback.

**6.2 — Factory returns: critical (double-free on every constructor).**
`emit_factory_body` at line 276-327 builds `r%raw = AzFoo_create(...); r%owned = .true.; end function`. F2003 returns a function result by value: the caller's destination variable gets a *copy* of `r`'s bytes (with `owned = .true.`); then the local `r` itself is finalized, which fires `AzFoo_delete(c_loc(r%raw))` on the *same heap pointers* the caller just inherited. **Double-free on every factory return.**

Fortran 2008 has `move_alloc` and intent(out) ownership-transfer semantics, but `function ... result(r)` returning a derived type does NOT trigger them — the result is bitwise-copied.

The fix needs one of:
- Switch returns to subroutines with `intent(out) :: r` arg (move semantics).
- Set `r%owned = .false.` BEFORE the function returns (so the local's finalizer is a no-op and the caller inherits ownership).
- Heap-allocate the wrapper and return a pointer (`type(Foo), pointer :: r => null()` plus `allocate(r); r%raw = ...`).

Severity: critical. Affects every wrapped constructor call (`Foo_create`, `Foo_default`, ...).

Code ref: `doc/src/codegen/v2/lang_fortran/wrappers.rs:321-322` — `r%raw = alias(arg_list); r%owned = .true.` with no transfer mechanism.

**6.3 — Owned-by-value wrapper args: critical (double-free).**
Same as 6.1 but for owned-wrapper args — the codegen at line 396-405 emits each arg as `type(...), intent(in), value :: nm` but never resets the caller's wrapper's `owned` flag after the call. Since Fortran subprograms cannot reach back into the caller's local variables without an `intent(inout)`, the only way to do this is either (a) make wrapper args `intent(inout)` and reset there, or (b) introduce a `consume(self)` subroutine analogue and call it explicitly at the wrapper-binding site.

Severity: critical.

**6.4 — Smoke-only context:**
Per `fortran_codegen_2026_05_13.md`, Fortran is currently smoke-only; tagged unions emitted as opaque (tag + c_ptr) 12-byte structs. The double-frees in 6.1/6.2 surface as soon as any factory or `with_*` is actually called against libazul.

---

## 7. SMALLTALK

**Finalizer mechanism:** Pharo's `WeakRegistry` / `FinalizationRegistry`. The wrapper holds `handle` (FFI structure pointer). `setHandle:` enrolls with `self class finalizationRegistry`. When GC reclaims the wrapper, Pharo sends `finalize` which calls `AzulNative azFooDelete: handle` and sets `handle := nil`.

**Code refs:**
- `doc/src/codegen/v2/lang_smalltalk/wrappers.rs:144-156` — `setHandle:` enrolls in registry.
- `doc/src/codegen/v2/lang_smalltalk/wrappers.rs:181-190` — `finalize` calls `azFooDelete: handle; handle := nil`.

### Issues found

**7.1 — Consume-after-by-value: critical (double-free).**
The codegen has no consume mechanism. After a self-by-value or owned-wrapper-arg call, `handle` still holds the pointer; the finalizer will fire later and double-drop. Smalltalk equivalent of `__consume` would be `handle := nil` (the existing `finalize` already short-circuits on `handle isNil`) plus removing from the finalization registry.

Severity: critical.

Code ref: `doc/src/codegen/v2/lang_smalltalk/wrappers.rs:283-322` — the primitive call args are built without any post-call consume step. Lines 320 and 333 in particular emit the call and immediately return; no `handle := nil` for self-by-value methods.

**7.2 — Returns_self: each call leaks the source wrapper (per call) — major.**
Line 330-333 wraps the new return as `^ self class wrap: (...)`, fresh wrapper with its own finalizer. Combined with 7.1, every `with_*` call leaves the OLD wrapper armed (finalize will fire eventually → double-drop because Rust took the bytes by value).

Severity: critical (combined with 7.1).

**7.3 — Option/Result: emitted but no payload extractor, no codegen-introduced leak.**
The tagged-union helper class at line 347-387 only emits unit-variant factories; data-bearing variants are explicitly skipped with `"SKIPPED: ..."`. Users access payloads directly through the FFI structure.

**7.4 — Smoke-only context:**
Per `smalltalk_tonel_blocker.md`, Smalltalk codegen emits Tonel-syntax in one .st file; Pharo's TonelReader expects a directory-package layout. The smoke layer (manual UnifiedFFI calls) passes; full package-tree emission is deferred. The double-free in 7.1/7.2 is latent until end-to-end works.

---

## 8. COBOL

**Finalizer mechanism:** None. The wrapper module emits only a documentation block (`generate_wrapper_docs` at `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_cobol/wrappers.rs:29-78`) listing types that own native memory and showing the manual `CALL FN-AZ-FOO-DELETE` pattern. No `FREE-AZ-*` paragraph is actually emitted — only suggested in comments.

### Issues found

**8.1 — Memory safety is entirely user-managed.**
No codegen-level destructor means no codegen-level double-free, no consume issue, no Option/Result leak. Users explicitly call delete; if they forget, they leak. If they double-call, they double-free.

Severity: n/a — codegen does not introduce any memory-safety issues, but it also doesn't help avoid them.

**8.2 — Smoke ceiling note:**
Per `cobol_smoke_ceiling.md`, full E2E requires ~200 LOC of user-side ENTRY-paragraph scaffolding per app. No codegen fix available; not in scope for this audit.

---

## 9. PHP

**Finalizer mechanism:** PHP `__destruct()` magic method. Wrapper class stores `private \FFI\CData $ptr`; `__destruct()` calls `Azul::lib()->AzFoo_delete(\FFI::addr($this->ptr))`.

**Code refs:**
- `doc/src/codegen/v2/lang_php/wrappers.rs:136-153` — `$ptr` storage + constructor.
- `doc/src/codegen/v2/lang_php/wrappers.rs:210-226` — `__destruct()` with the `\FFI::addr($this->ptr)` call.

### Issues found

**9.1 — Consume-after-by-value: critical (double-free).**
PHP's deterministic `__destruct` will run on out-of-scope and call `AzFoo_delete(\FFI::addr($this->ptr))`. After a by-value-consuming FFI call (the wrapper passed itself to a `with_*` form), `$this->ptr` still references the same cdata; the destructor double-drops.

Severity: critical. The PHP-side fix is similar to Ruby's `Azul._consume`: a private `_consume()` method on each wrapper that nulls `$this->ptr` (and `__destruct` checks `$this->ptr === null` before calling delete).

Code ref: `doc/src/codegen/v2/lang_php/wrappers.rs:423-466` (`emit_instance_method`) — the call is built without any consume-side hook. Line 453 always uses `\FFI::addr($this->ptr)`.

**9.2 — Self-by-value handling: incorrect (passes pointer where value expected).**
Line 453 unconditionally emits `\FFI::addr($this->ptr)` even for C methods that take self by value. PHP FFI does not auto-dereference; this either crashes or passes garbage. Same shape as the Zig 5.1 issue.

Severity: critical (in the rare paths it doesn't crash, the double-free in 9.1 follows).

**9.3 — Tagged-union payload extractors return borrows: minor (use-after-free).**
The enum-wrapper `payload<Variant>()` method at line 348-354 returns `$this->ptr->{<field>}` — an FFI cdata reference into the *parent* Option/Result's union memory. When the parent wrapper is destructed, the inner cdata's backing memory is freed; the user's still-held reference dangles. PHP's FFI doesn't track inner-pointer keep-alive relationships.

Severity: minor (only matters when user stores the payload across the parent's lifetime).

Code ref: `doc/src/codegen/v2/lang_php/wrappers.rs:347-355` — `return $this->ptr->{php_field};`.

**9.4 — Option/Result outer struct: no payload-extract-then-free helper.**
Same as Pascal/Fortran/Smalltalk. The user can call `__destruct` explicitly (via `unset`) but the codegen never emits the pattern. Mostly a usability issue rather than a leak — PHP's `__destruct` will eventually fire when refcount hits 0.

**9.5 — Smoke-only context:**
Per `php_b13_smart_factory.md`, PHP is blocked on macOS App.run anyway; full E2E hasn't been demonstrated. The double-free in 9.1 is latent.

---

## Cross-language summary of fixes needed

All eight finalizing languages need the equivalent of commit `62094b885`:

| Lang      | Consume mechanism analogue                                              | Location in codegen                              |
|-----------|-------------------------------------------------------------------------|--------------------------------------------------|
| Haskell   | `IORef Bool` cell on the wrapper; bracket release checks before delete  | `lang_haskell/wrappers.rs:232-292`               |
| Perl      | `$$self = undef` after consume; `DESTROY` already short-circuits        | `lang_perl/wrappers.rs:209-243`                  |
| Pascal    | `Self.FOwned := False;` after consume                                   | `lang_pascal/wrappers.rs:417-461`                |
| Go        | `runtime.SetFinalizer(self, nil)` after consume                         | `lang_go/wrappers.rs:336-413`                    |
| Zig       | New `consumed: bool = false` field on wrapper; `deinit` checks it       | `lang_zig/wrappers.rs:264-270, 336-401`          |
| Fortran   | `self%owned = .false.` after consume (need `intent(inout)` self)        | `lang_fortran/wrappers.rs:329-449`               |
| Smalltalk | `handle := nil` after consume + un-enroll from finalizationRegistry     | `lang_smalltalk/wrappers.rs:210-341`             |
| PHP       | `_consume()` private method nulling `$this->ptr`                        | `lang_php/wrappers.rs:423-501`                   |

Additional language-specific critical fixes:

- **Fortran 6.2** — factory-return double-free needs a wrapper-allocation rework (allocatable result, subroutine + intent(out), or pre-return `r%owned = .false.`). Affects every constructor; this is independent of the consume fix.
- **Pascal 3.2** — `FRaw`-by-value embedded record creates a transitive-double-free path through `Wrap(ARaw)` and Pascal's default record-copy semantics. Needs a `Wrap(ARaw: PAzFoo)` (pointer) variant OR a reference-counted wrapping discipline.
- **Zig 5.1 / PHP 9.2** — self-by-value calls produce uncompilable / wrong-pointer code; needs a `self_by_value` branch that emits `self.inner` (Zig) / `$this->ptr` raw (PHP) instead of `&self.inner` / `\FFI::addr(...)`.
- **Go 4.3** — instance-method-returns-self needs the matching `runtime.SetFinalizer` registration; mirrors the static-factory branch.

Option/Result outer-free is consistently absent across all nine. This is **major (per-call heap leak)** for any Option/Result-returning method whose payload owns inner heap (AzString.Vec.ptr, AzDom.styled-node-vec, etc.). The JVM/CLR fix in commit `75a1fbcd2` (clone-then-delete for wrapper payloads; decode-then-delete for AzString; extract-then-delete for primitives) needs the same port to each language's idiomatic shape.

AzVec → host-iterable is mostly not emitted in this set (only Haskell has `<vec>ToList`); when it is, the Haskell version produces borrow-elements that dangle if any element type carries inner heap.
