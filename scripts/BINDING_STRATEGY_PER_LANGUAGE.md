# Per-language binding strategy

The honest decision tree for how each language binding should reach
Python-quality hello-world (37 lines, plain class as model, fluent
builder API, `data.counter += 1` in a callback).

---

## Concurrent-agent note

While this plan runs, **another agent is applying fixes to the main
codebase** (libazul source, codegen scaffolding, IR, etc.). Treat its
changes as benign:

- If a `cargo build` / `cargo run -- codegen` fails because the other
  agent's changes haven't settled (e.g. a half-applied refactor, an
  IR field renamed, a regen in progress), **do not try to fix it
  yourself**. Don't revert their files, don't patch through their
  half-done work.
- Wait ~60 seconds and retry the same build/regen. Their work is
  likely either still in progress or just committed and the working
  tree is now consistent.
- If the conflict is in a file you were editing, `git stash` your
  work, pull in any changes already on disk (just re-read the file —
  there's no remote to pull from; this is local-only), reapply your
  edits on top of the new state, then continue.
- Never `git reset --hard`, `git branch -D`, or `git push --force`
  to "resolve" the conflict. The other agent's commits are real work.

The other agent is NOT working on:
- `examples/<lang>/hello-world.<ext>` rewrites (that's our turf).
- `scripts/BINDING_STRATEGY_PER_LANGUAGE.md` (this doc — ours).
- Per-language wrapper file generators in `doc/src/codegen/v2/lang_<x>/`
  EXCEPT for `lang_php_ext.rs` (PHP-extension work, handed off to
  the other agent before this plan started).

If we touch the same file as the other agent (rare — mostly bounded
to `lang_<x>/wrappers.rs` for whichever lang we're polishing), and
their edit lands first, the diff is usually orthogonal (they fix IR-
plumbing bugs, we fix per-emitter wrapper bugs). Re-base mentally
and continue.

---

## Revised conclusion (after pushback)

**One `libazul.so` serves every language.** No PyO3, no napi-rs, no
magnus, no jni-rs dependencies.

The previous draft of this doc claimed pure FFI was a "dead end" for
Node/Ruby/Java because of GC/thread issues. That was wrong. The honest
list of "things FFI cannot do" is two items, and both have simple
fixes:

### Thing 1: Cross-thread callbacks need VM lock acquisition

The actual threading model in azul (read from source, not folklore):

```
                      ┌───────────────────┐
   user clicks ──────►│   Main thread     │
                      │  ─────────────    │
   layout/click/      │   App.run() loop  │
   widget callbacks   │                   │
   fire here          │   reads receiver  │
                      │   for Thread msgs │◄─── ThreadReceiveMsg::WriteBack
                      │                   │     fires WriteBackCallback
                      │                   │     on this thread
                      └───────┬───────────┘
                              │
              Thread::create(init, writeback, cb)
                              │
                              ▼
                      ┌───────────────────┐
                      │   Worker thread   │
                      │  ─────────────    │
                      │   ThreadCallback  │
                      │   (cb) fires here │
                      │                   │
                      │   sends back via  │
                      │   ThreadSender    │───► queued for main-thread pickup
                      └───────────────────┘
```

Key facts:

- `Thread::create` spawns a real `std::thread::spawn` worker
  (layout/src/thread.rs:846).
- The `ThreadCallback.cb` receives `(RefAny init, ThreadSender,
  ThreadReceiver)` and runs **on the worker thread**.
- `ThreadSender::send(ThreadReceiveMsg::WriteBack(WriteBackMsg))`
  enqueues a message. The worker can do many sends before exiting.
- Main thread's `App.run` loop polls each Thread's receiver. When it
  finds a `WriteBack` message, it invokes the carried
  `WriteBackCallback` **on the main thread**
  (layout/src/thread.rs:301-305 — doc-comment confirms "runs on the
  main UI thread").

So in azul there are **two** off-main-thread interaction points, not
one:

| Callback | Where it fires | Host-VM lock needed for host code? |
|---|---|---|
| `ThreadCallback`         | worker thread | yes (or use writeback-only pattern) |
| `WriteBackCallback`      | main thread   | no |
| `Callback` / `LayoutCallback` / 19 widget kinds | main thread | no |

`HOST_INVOKER_KINDS` currently has **none** of {`ThreadCallback`,
`WriteBackCallback`}. The 19 main-thread widget+layout kinds are
covered. So **two** callback kinds need their FFI plumbing finished:

1. **WriteBackCallback** — fires on main, so adding it to
   `HOST_INVOKER_KINDS` is purely mechanical: apply
   `impl_managed_callback!` in `azul-layout::thread`, regenerate
   per-language setters and from-host-handle constructors. No
   threading concern.

2. **ThreadCallback** — fires on worker. Adding it to
   `HOST_INVOKER_KINDS` is mechanical too, BUT the per-language host
   invoker for this kind must acquire the host VM's lock before
   calling the user's function. Solved with **2-3 C function bindings
   per host VM**, declared in each language's wrapper file just like
   any other `extern "C"` symbol.

### Per-VM lock-acquire pattern

The pure-FFI binding's ThreadCallback thunk looks like (Python
shown — every language follows the same shape):

```
extern "C" fn az_thread_callback_host_invoker(
    id: u64,
    init_data: *const RefAny,
    sender: *const ThreadSender,
    receiver: *const ThreadReceiver,
) {
    // 1. Locate the registered Python callable by id.
    let py_callable = lookup_handle(id);

    // 2. Acquire host VM lock. (Already-attached threads are fast no-ops.)
    let state = PyGILState_Ensure();

    // 3. Call host code.
    py_callable.call((init_data, sender, receiver));

    // 4. Release.
    PyGILState_Release(state);
}
```

The body is the *same shape* for every language; only the
acquire/release call differs. That symmetry is the whole point of
keeping it in the FFI layer — one pattern in 10 languages, not 10
extension crates.

| VM | Acquire | Release | Symbol library |
|---|---|---|---|
| **CPython** | `PyGILState_Ensure() → state` | `PyGILState_Release(state)` | `libpython3.X.{so,dylib}` (find via `Py_GetPath` or hardcode `python3-config --ldflags` at codegen time) |
| **MRI Ruby** | `rb_thread_call_with_gvl(fn, data)` — wraps the body, acquire+call+release in one shot | (combined) | `libruby.{so,dylib}` |
| **OpenJDK** | `(*jvm)->AttachCurrentThread(jvm, &env, NULL)` | `(*jvm)->DetachCurrentThread(jvm)` | from `JavaVM*` cached at JNI_OnLoad — but we are loaded the other direction (Rust calls into Java), so we cache `JavaVM*` once on first init via `JNI_GetCreatedJavaVMs` |
| **CLR / .NET** | nothing | nothing | `[UnmanagedCallersOnly]` delegate self-trampolines from any thread |
| **Node (N-API)** | `napi_acquire_threadsafe_function(tsfn) → napi_call_threadsafe_function(tsfn, data, blocking)` | `napi_release_threadsafe_function(tsfn, release_mode)` | `libnode` / N-API shipped with Node |
| **Bun** | `bun:ffi` `JSCallback` is thread-safe by default | n/a | n/a |
| **Deno** | `Deno.UnsafeCallback` constructed via `threadSafe(...)` | dispose on Drop | n/a |
| **OCaml** | `caml_acquire_runtime_system()` | `caml_release_runtime_system()` | the OCaml runtime (statically linked or `libcamlrun.{so,a}`) |
| **SBCL** | `define-alien-callable :from-foreign t` — runtime attaches automatically | n/a | `sbcl-runtime` (linked into the SBCL binary) |
| **Pharo Smalltalk** | UFFI marshals callbacks to main image thread via image-side queue | n/a | n/a |
| **Lua / LuaJIT** | **single-threaded interpreter — no lock exists**. ThreadCallback host-code is NOT supported; use writeback-only pattern (Rust extern "C" on worker, Lua fn on main via WriteBackCallback). | — | — |
| **Perl** | same as Lua (no `usethreads` typical) | — | — |
| **PHP** | same as Lua (no TSRM typical) | — | — |
| **Go / Zig / Pascal / FreeBASIC / Ada / Fortran / COBOL / VB6 / Algol68** | no lock needed (binding is native code) | — | — |

### Why this stays in the FFI layer (not in a per-VM extension)

The user instinct: "if we can just copy the few GIL / threading work
[per language], it's better to stay raw azul.so only, to not pull in
tons of dependencies — this way one azul.so can serve 10 languages at
once." That's correct. The 4-step thunk shape above is the only thing
that varies per VM, and the variation is small:

- **CPython**: 2-line `Py_BEGIN_ALLOW_THREADS` / `Py_END_ALLOW_THREADS`-style
  bracketing OR `PyGILState_Ensure/Release` pair.
- **Ruby**: one call (`rb_thread_call_with_gvl`).
- **JVM**: one call each side; cached JavaVM pointer.
- **OCaml**: pair.
- **Node**: ThreadsafeFunction lifecycle (~5 lines).

The cost of pure-FFI threading: ~5-50 lines of host-language code per
binding. The cost of native-extension threading: importing one of
PyO3/napi-rs/magnus/jni-rs/ocaml-rs (each a multi-thousand-LOC
dependency, with their own update cadence, version pinning, build
system invasiveness).

For our purposes, FFI wins decisively.

### WriteBackCallback host-invoker addition

`WriteBackCallback` is genuinely main-thread. The fix is mechanical
codegen plumbing (one `impl_managed_callback!` invocation in
`azul-layout/src/thread.rs`, automatic propagation through
`HOST_INVOKER_KINDS`). No threading consideration. This unlocks user-
side WriteBack handlers in every host-invoker tier language.

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

## Host-invoker tier polish pass (Phase 6 detail)

The 14 "host-invoker tier" languages each have working FFI dispatch
but stop at a refany-roundtrip smoke test. The polish pass per
language has the same shape — call it the **Polish Card**. Each card
takes 1-3 hours.

### The Polish Card (per language)

For language **X**:

1. **Wrapper surface audit** (~30 min).
   Check that `azul.X` exports: `Dom`, `Button`, `WindowCreateOptions`,
   `App`, `AppConfig`, `Css`, `CssProperty`, `CssPropertyWithConditions`,
   `StyleFontSize`, `String`, `RefAny`, plus enums `Update`,
   `ButtonType`, `WindowDecorations`, `WindowBackgroundMaterial`. If
   any are missing or behind a non-idiomatic name, file follow-up.

2. **Idiomatic-type mapping** (~30 min).
   Verify that the codegen emits adapters for:
   - `AzString` ↔ host string (UTF-8 bytes + length, copy on
     both sides).
   - `AzOptionX` ↔ host `nil`/`null`/`None`/`Optional<X>`/`Maybe X`.
   - `AzResultXString` ↔ host exception (raise/throw) or Result type.
   - `AzRefAny` + host-handle pattern: callbacks receive the host
     model object directly (the wrapper does `refany_get` for you).
   - `PartialEq` → host `==` / `eql?` / etc.
   - `Display` / `Debug` → host `to_s` / `inspect` / `__str__` /
     `toString`.

3. **Hello-world rewrite** (~30 min).
   File: `examples/X/hello-world.<ext>`. Match `examples/python/hello-world.py`'s
   shape:
   - Plain host class as model with a `counter` attribute.
   - `layout(data, info)` returns `Dom.create_body().with_child(...)`.
   - `on_click(data, info)` does `data.counter += 1; return Update.RefreshDom`.
   - `app = App.create(model, AppConfig.create())`.
   - `app.run(WindowCreateOptions.create(layout))`.
   - Title 'Hello World', 400×300, NoTitleAutoInject decorations,
     Sidebar background — same window state as the C reference.

4. **Build + verify** (~30 min).
   - Run with a 5-second timeout. Exit 124 (timed out) = GUI loop ran.
   - With `AZ_DEBUG=<port>` env var, drive the click probe:
     `curl -s -X POST http://localhost:<port>/ -d '{"op":"click","selector":".__azul-native-button"}'`
   - Confirm counter increments by reading `op=get_html_string` before
     and after.

5. **Commit** (~5 min).
   Subject: `examples/<X>: idiomatic hello-world (counter increments end-to-end)`.

6. **Memory + plan update** (~5 min).
   Update memory `full_gui_examples_status.md` to add X to "verified E2E."
   Tick checkbox in this doc's status table.

### Polish-pass order

Sorted by least-codegen-prerequisite first (cheaper wins early):

| # | Lang | Why this position |
|---|---|---|
| 1 | **Node** | Codegen fixes already designed (Phase 1); biggest user base. |
| 2 | **Ruby** | `self.class.new` codegen fix already landed; just needs regen + finalizer cleanup + hello-world. |
| 3 | **Perl** | FFI::Platypus is permissive; mostly wrappers + hello-world. |
| 4 | **Lisp** | CFFI handles struct-by-value; mostly wrappers + hello-world. |
| 5 | **OCaml** | Ctypes handles struct-by-value; wrappers + AzOption→option mapping. |
| 6 | **PowerShell** | Sketch already drafted in current hello-world.ps1; verify. |
| 7 | **C#** | tagged-union layout work needed; PowerShell rides on this. |
| 8 | **Java** | JNA struct-by-value the main work. |
| 9 | **Kotlin** | rides on Java once that's working. |
| 10 | **Pascal** | works in principle; needs the same wrapper surface as the others. |
| 11 | **Fortran** | same as Pascal. |
| 12 | **Ada** | same as Pascal. |
| 13 | **PHP** | other agent's work; pick up when they hand off. |
| 14 | **Lua** | already E2E — just needs to ride a polish review for parity. |

### What does NOT get polished in this phase

- **Algol68 / FreeBASIC / VB6**: toolchain unavailable on macOS. Skipped.
- **Smalltalk**: Pharo Tonel layout blocker. Skipped.
- **COBOL**: copybook FN-* aliases emitted but ENTRY paragraphs are
  user-side. Smoke-test-tier is the realistic ceiling.
- **Haskell**: GHC FFI rejects struct-by-value returns. Needs a
  separate C shim codegen phase (cost: ~1-2 days). Defer.

### Acceptance criteria for "polish pass complete"

- 14 host-invoker-tier languages have an `examples/<lang>/hello-world.<ext>`
  that:
  - is structurally identical to `examples/python/hello-world.py`
    (model class, layout cb, on_click cb, button, App.run)
  - has been verified E2E via the AZ_DEBUG click probe in CI or
    manual run
  - uses the language's idiomatic types (no `AzOptionString` leaks,
    exceptions for errors, native string handling)
- The plan's status table at the top of this doc reflects each one
  ticked.
- A memory entry per language confirms verified E2E.

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
