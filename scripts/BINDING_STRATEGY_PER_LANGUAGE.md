# Per-language binding strategy

The honest decision tree for how each language binding should reach
Python-quality hello-world (37 lines, plain class as model, fluent
builder API, `data.counter += 1` in a callback).

---

## Revised conclusion (after pushback)

**One `libazul.so` serves every language.** No PyO3, no napi-rs, no
magnus, no jni-rs dependencies.

The previous draft of this doc claimed pure FFI was a "dead end" for
Node/Ruby/Java because of GC/thread issues. That was wrong. The honest
list of "things FFI cannot do" is two items, and both have simple
fixes:

### Thing 1: Cross-thread callbacks need VM lock acquisition

Only **one** callback kind in azul fires off-main-thread:
`ThreadCallback`, invoked by `Thread::create` (layout/src/thread.rs:846)
on a `std::thread::spawn`-ed worker. Confirmed by reading the source:
`HOST_INVOKER_KINDS` deliberately excludes `ThreadCallback`; the 19
other kinds (Layout, Callback, ButtonOnClick, VirtualView, etc.) all
fire on the main `App.run` thread.

The fix is **2-3 C function bindings per host VM**, declared in each
language's `azul.{lua,rb,py,js,pl,…}` wrapper just like any other
`extern "C"` symbol. The VM-lock dance happens entirely inside the
thread-callback thunk; the rest of the binding stays pure FFI.

| VM | Acquire / release | Library |
|---|---|---|
| **CPython** | `PyGILState_Ensure()` → `PyGILState_Release(state)` | `libpython3.X.{so,dylib}` |
| **MRI Ruby** | `rb_thread_call_with_gvl(fn, data)` | `libruby.{so,dylib}` |
| **OpenJDK** | `JavaVM::AttachCurrentThread` / `DetachCurrentThread` (via JavaVM function table) | the JVM itself |
| **CLR / .NET** | nothing — `[UnmanagedCallersOnly]` delegates auto-trampoline from any thread | n/a |
| **Node V8** | `napi_call_threadsafe_function(...)` *or* sidestep entirely: queue on a Mutex + signal main loop via `setImmediate` polling | `libnode` or pure-JS queue |
| **Bun** | `bun:ffi` `JSCallback` is already thread-safe | n/a |
| **Deno** | `Deno.UnsafeCallback.threadSafe(...)` | n/a |
| **OCaml** | `caml_acquire_runtime_system()` / `caml_release_runtime_system()` | `libcaml.{so,dylib}` |
| **SBCL** | foreign callables auto-attach (`sb-alien:define-alien-callable :foreign`) | `sbcl-runtime` |
| **Pharo Smalltalk** | UFFI auto-marshals callbacks to main image thread | n/a |
| **Lua / LuaJIT** | **cannot** be called from worker thread (single-threaded interpreter) — must use writeback-only pattern | n/a |
| **Perl** | same as Lua (no `usethreads`) | n/a |
| **PHP** | same as Lua (typical non-TSRM builds) | n/a |
| **Go / Zig / Pascal / FreeBASIC / Ada / Fortran / COBOL / VB6 / Algol68** | no lock needed (callback is a real C fn pointer, runs in native context) | n/a |

For the langs marked "cannot": users pass a **Rust extern "C"** function
to `Thread::create` (worker side, does native work), and the main-thread
`WriteBackCallback` is the host-language one. This matches the Rust
`async.rs` example pattern and is more honest about the threading model
anyway — the worker shouldn't touch the UI.

### Thing 2: Compile-time type safety across the boundary

Native extensions check at Rust compile time that
`fn click(model: &MyDataModel) -> Update` is wired to a RefAny actually
carrying MyDataModel. Pure FFI does the dispatch at runtime through an
id table. Functionally equivalent, just type-unsafe in the FFI case.

**This isn't a capability issue.** It's a polish issue. Pure-FFI
bindings have always done runtime dispatch (PHP/Lua/Perl/etc.) and
nobody complains.

---

## What was wrong with my earlier list

Re-stated honestly:

| Earlier claim | Reality |
|---|---|
| "koffi can't write back through `T *`, so `add_child` is broken" | True for koffi specifically, but `with_child` works. Wrappers emit the consuming form. Solved by codegen, not by architecture. |
| "FinalizationRegistry double-frees moved-from structs" | True if the codegen doesn't unregister on consuming moves. Fix is one line in `emit_instance_method`: `Class._registry.unregister(this); this._ptr = null; return new Class(_next);`. |
| "Callback struct returns drop on the floor" | True if the codegen's host-invoker thunk only writes scalars. Fix: emit `koffi.encode(outPtr, '<RetType>', ret)` for struct returns. ~5 lines per thunk. |
| "GC mismatch with id table" | The id table is exactly how Python/Lua/Ruby do it today and it works. The "Py handle inside RefAny" model is a polish, not a requirement. |
| "Idiomatic Option/Result/String needs PyO3-style trait bridging" | Polish. Wrapper adapter methods do the same thing one type at a time. |

---

## What we actually do

### Strategy: thin pure-FFI wrapper, every language

For each language, the generator emits:

1. **`azul.{lua,rb,js,pl,pm,py-shim,etc.}`** — a single source file. Loads
   `libazul`. Declares every C struct and function symbol. Wraps each
   IR struct in an idiomatic class with finalizer. Routes user
   callbacks through the host-invoker pattern (which already works for
   19 of 20 kinds).

2. **`<lang>_api.rs`** — already emitted by `managed.rs`. The static C
   thunks compiled into libazul itself dispatch to the host-invoker
   registered with `AzApp_setCallbackInvoker`. Only the `ThreadCallback`
   thunk needs the VM-lock dance from Thing 1 above.

3. **Per-VM lock acquisition** — codegen emits a small shim in the
   wrapper file that binds `PyGILState_Ensure` / `rb_thread_call_with_gvl`
   / whichever applies (or "no-op" for VMs that don't need it). The
   `ThreadCallback` invoker calls into the shim before dispatching.

### Codegen fixes that are *actually* needed

Listed by language, in the order they bite. None of these are
architectural; they're all single-codegen-file fixes.

#### Node (`lang_node/wrappers.rs`)
- [ ] Instance methods returning Self: wrap result in `new Class(...)`,
  unregister `this` from FinalizationRegistry, null `this._ptr`.
- [ ] `add_*`/`set_*` methods that have a matching `with_*`: route
  through the consuming form internally so the `this._ptr` reflects
  mutations. (koffi-only quirk.)
- [ ] Host-invoker thunk for LayoutCallback (and any other kind with a
  struct return): `koffi.encode(outPtr, '<RetType>', ret)` for object
  returns, not just scalars.
- [ ] No more `process.on('uncaughtException', ...)` workaround in
  examples — instead, catch inside the thunk before it bubbles to
  libffi.

#### Ruby (`lang_ruby/wrappers.rs`)
- [ ] `self.class.new(...)` in class methods → just `new(...)` (already
  applied, blocked on codegen regen).
- [ ] Finalizer: unregister via `ObjectSpace.undefine_finalizer(self)`
  before consuming moves.

#### Java/Kotlin (`lang_jvm/...`)
- [ ] JNA struct-by-value returns: use `[StructLayout]` annotations or
  switch to manual JNI shims auto-emitted by the codegen. JNA can be
  made to work; investigate `Structure.ByValue`.

#### C# (`lang_csharp/...`)
- [ ] `[StructLayout(LayoutKind.Sequential)]` per IR struct, already
  emitted. Verify tagged-union enums are correctly laid out.
- [ ] `[UnmanagedCallersOnly]` for the ThreadCallback fn pointer (Thing 1
  resolution).

#### OCaml (`lang_ocaml/wrappers.rs`)
- [ ] Surface `Dom` / `Button` / `WindowCreateOptions` / `App` wrapper
  modules — currently only `String` is exposed in `azul.ml`.
- [ ] Map `AzOption<T>` → OCaml `T option`, `AzResult<T,E>` → `(T, E) result`.

#### Per language with worker-thread support: ThreadCallback thunk
- [ ] Python: bind `PyGILState_Ensure` / `_Release`, wrap thunk.
- [ ] Ruby: bind `rb_thread_call_with_gvl`, wrap thunk.
- [ ] Java: cache `JavaVM*` at JNI_OnLoad, `AttachCurrentThread` in thunk.
- [ ] OCaml: bind `caml_acquire_runtime_system` / `_release`.

#### Per language without worker-thread support
- [ ] Lua / Perl / PHP / Pharo: ThreadCallback thunk runs the Rust-only
  callback. Document that user-side cannot pass a host-language
  function to `Thread::create`; use a Rust shim + writeback.

---

## Per-language status table

| Lang | Wrapper-layer state | Host-invoker plumbing | Thread-callback handling | Hello-world state |
|---|---|---|---|---|
| Python | A-tier (PyO3 native ext) | done | done (`Python::attach`) | 37 lines, gold standard |
| Lua | done | done | writeback-only (LuaJIT single-threaded) | 92 lines, verified E2E |
| Go | direct cgo | n/a (native fn pointers) | n/a (native fn pointers) | 165 lines, mirrors C |
| Zig | direct C ABI | n/a | n/a | verified E2E |
| Node | wrapper has 3 codegen bugs above | done | TODO (use `napi_call_threadsafe_function` or main-loop poll) | 160 lines with workarounds — rewrite after codegen fix |
| Ruby | `self.class.new` bug + finalizer race | done | TODO (bind `rb_thread_call_with_gvl`) | smoke test only |
| Java/Kotlin | JNA struct returns broken | done | TODO (bind `AttachCurrentThread`) | smoke test only |
| C# | manual marshal, tagged-union layout TBD | done | n/a (`[UnmanagedCallersOnly]`) | smoke test only |
| OCaml | only `String` wrapper exposed | done | TODO (bind `caml_acquire_runtime_system`) | smoke test only |
| Perl | works | done | writeback-only | smoke test only — needs Dom/Button wrappers |
| Lisp | works | done | foreign callables auto-attach | smoke test only |
| PowerShell | embeds C# | follows C# | follows C# | full sketch, untested |
| Pascal/FreeBASIC/Ada/Fortran/COBOL/Algol68/VB6 | works | done where applicable | n/a | smoke test or attempt |
| Haskell | rejects struct returns | n/a | n/a | smoke test only — needs C shim layer |
| Smalltalk | Tonel layout blocker | done | n/a (UFFI auto-marshals) | smoke test only |
| PHP | A-tier (ext-php-rs) in progress | n/a | n/a | other agent owns |

---

## Migration plan (revised)

### Phase 1 — Node codegen fixes (~4-6 hours)

Three codegen patches:

1. `emit_instance_method` for Self-returning methods: wrap + unregister
   + null `_ptr`.
2. `emit_managed.rs` host-invoker thunks: `koffi.encode(outPtr, ..., ret)`
   for struct returns.
3. `emit_instance_method` for `add_*` / `set_*` mutators: route through
   matching `with_*` form, replace `this._ptr`, re-register.

Regen, rewrite `examples/node/hello-world.js` to use the wrapper API
(`new Dom().with_child(...)` style), verify E2E.

### Phase 2 — Ruby codegen fixes (~3-4 hours)

1. Fix `self.class.new` (already done, blocked on regen).
2. Add `ObjectSpace.undefine_finalizer` to consuming-move sites.
3. Surface Dom/Button/WCO/App wrappers in the user-facing API.
4. Rewrite `examples/ruby/hello-world.rb` to Python quality.

### Phase 3 — OCaml + Lisp + Perl wrapper surfaces (~2-3 hours each)

Each is "smoke test, needs wrappers." Surface the missing
Dom/Button/WindowCreateOptions/App classes/modules.

### Phase 4 — Java/Kotlin/C# (~1 day each)

JNA struct returns + tagged-union layout. The pure-FFI path stays;
codegen just needs more elbow grease for JVM type encoding.

### Phase 5 — ThreadCallback thunks (per language, ~1-2 hours each)

Per-VM lock-acquire shim in each language's host-invoker init.

### Phase 6 — Hello-world rewrites (final pass)

Each smoke-test → Python-quality 30-50-line GUI.

---

## What does NOT change

- The C ABI in `dll/`.
- The IR (`doc/src/codegen/v2/ir/`).
- `examples/c/hello-world.c` (the reference).
- `lang_python.rs` (PyO3 native ext is what it is; not migrating away).
- `lang_php_ext.rs` (ext-php-rs path; other agent's work).

Native-extension generators (Python, PHP-ext) and pure-FFI generators
(everyone else) coexist forever. The only `libazul` is the prebuilt
one all bindings already load.

---

## Status 2026-05-12

- Phase 0 (survey + revised plan): done.
- Phase 1 (Node fixes): not started.
- Phase 2 (Ruby fixes): `self.class.new` codegen edit applied, regen
  pending (blocked on prior ENOSPC; should retry after disk freed).
