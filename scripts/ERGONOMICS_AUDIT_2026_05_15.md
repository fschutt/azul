# Azul Bindings — Hello-World Ergonomics Audit
**Date:** 2026-05-15
**Reference:** Python `examples/python/hello-world.py` (37 LOC, including blanks).
**Scope:** Compare every codegen-emitted hello-world against the Python gold standard. Identify the C-esque leakage that remains, propose codegen-side fixes, and weight them by leverage.

---

## TL;DR — Top 5 highest-leverage codegen wins

| # | Change                                                     | Bindings impacted             | Effort       |
|---|------------------------------------------------------------|-------------------------------|--------------|
| 1 | Hide the raw struct ↔ pointer marshal at App.run / wco     | Java, Kotlin, Scala, C#       | 1 day        |
| 2 | Wrap RefAny in a host-side smart pointer (`Data<T>`)        | Java, Kotlin, Scala, C#, Ruby, Node, Lua, OCaml | 2 days   |
| 3 | Static-typed `Option<T>` / `Result<T,E>` payload extraction | Java, Kotlin, Scala, C#, OCaml | multi-day   |
| 4 | Drop `azul._az_string` / `azul.raw_dom` / `data:clone()` from hello-worlds via codegen-side auto-conversion | Ruby, Lua, OCaml, Node | 1 day    |
| 5 | Emit `App.run(window)` wrapper that owns the App by-value & swallows the pointer dance | Java/Kotlin/Scala/C# | 0.5 days |

These five collectively bring the static-typed bindings from ~70-85 LOC to ~45-55 LOC and the dynamic bindings (Ruby/Node/Lua) into the 40-50 LOC band.

---

## Python — the reference (37 LOC)

```python
from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def layout(data, info):
    label = (Dom.create_text(str(data.counter))
             .with_css("font-size:50px;"))
    button = (Dom.create_div()
              .with_css("flex-grow:1;")
              .with_child(Dom.create_text("Increase counter"))
              .with_callback(EventFilter.Hover(HoverEventFilter.MouseUp()), data, on_click))
    body = (Dom.create_body()
            .with_child(label)
            .with_child(button))
    return body.style(Css.empty())

def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom()

model = DataModel(5)
window = WindowCreateOptions.create(layout)
app = App.create(model, AppConfig.create())
app.run(window)
```

What makes it 37 LOC and ergonomic:

1. **No host-invoker visible.** `data` *is* the user object, both inside the layout fn (`data.counter`) and at `App.create(model, ...)`. No `refany_create` / `refany_get` ceremony.
2. **No struct marshaling.** The user never reaches for "the raw struct" — every method returns / accepts the wrapper class.
3. **Fluent builder.** `Dom.create_body().with_child(label).with_child(button)` is a value-returning chain. No mutators.
4. **Enums as values.** `Update.RefreshDom()` returns directly; no `(int)` cast or `.value` lookup.
5. **`with_callback(filter, data, fn)` takes a plain Python function.** No `register_callback`, no SAM wrapper, no `mk_X / set_X` trampoline.
6. **No window-state-field assignment** for the simple case — `WindowCreateOptions.create(layout)` is a single-arg factory; size/title aren't set explicitly because Python's reference example omits them.

**Architectural caveat:** Python's binding is a Rust *native extension* via PyO3 (`lang_python.rs` emits `#[pyclass]` + `#[pymethods]`, NOT FFI cdef). This means the wrapper *is* the Rust type — the host-invoker / handle-table indirection that every FFI-driven binding pays for simply doesn't exist on the PyO3 path. **Other bindings cannot match Python at the architecture level — but the user-facing surface can be made nearly indistinguishable through codegen-emitted wrappers.**

---

## Cross-cutting wins

These codegen changes pay off across multiple bindings simultaneously. Listed in order of `(leverage × feasibility)`.

### CC-1. RefAny → host smart-pointer (`Data<T>`)
**Affected:** Java, Kotlin, Scala, C#, Ruby, Node, Lua, OCaml — every binding except Python.

**Symptom.** The user writes `AzulHostInvoker.refanyCreate(MODEL)` (Java/Kotlin/Scala) or `HostInvoker.RefanyCreate(_model)` (C#) or `Azul::RefAny.wrap(model)` (Ruby) or `refanyCreate(model)` (Node) or `azul.refany_create(model)` (Lua) or `Azul.azul_refany_create model` (OCaml). Then inside callbacks the inverse `refanyGet(dataPtr)` followed by a downcast (`as MyDataModel` / `instanceof MyDataModel` / `is MyDataModel`).

**Fix.** Emit a generic `Data<T>` (Java/Kotlin/Scala/C#) / `RefAny[T]` (typed wrapper) per binding that:
- Constructs at `Data.of(model)` — internally registers via host-handle table.
- Inside callbacks, the codegen-emitted invoker trampoline already has the handle id; it can downcast to `T` and pass `data: T` to the user fn directly. The user's callback signature becomes `(MyDataModel, info) -> Update` instead of `(IntPtr, IntPtr) -> int`.
- Maps 1:1 to api.json: every callback typedef's first arg is `*const RefAny` (this is universal). The codegen already emits a per-kind invoker thunk; that thunk already calls `refanyGet(arg0)`. Promote the user-supplied callback signature from "raw FFI" to "the host-handle's stored object, casted to a user-declared T".

**api.json metadata:** complete (callback typedef arg 0 is `RefAny *` for the kinds in `HOST_INVOKER_KINDS`).
**Effort:** 1 day for the dynamic-typed bindings (Ruby/Node/Lua — just stash the type and skip the user `as` cast); 2 days for static-typed (Java/Kotlin/Scala/C#) because they need a typed `Data<T>` wrapper + a generic-typed invoker callback signature.

This single change deletes 5-10 lines of host-invoker visibility from every full-GUI hello-world. Maps to plan item A.5 ("Hide AzulHostInvoker entirely") which closed only partially.

### CC-2. Inline AzDom-return through the invoker out-pointer
**Affected:** Java, Kotlin, Scala (and to a lesser extent C# — its `Marshal.StructureToPtr` is one line; the JVM trio's JNA dance is 4 lines).

**Symptom.** Every JVM layout callback ends with:
```java
AzDom.ByValue bodyRaw = (AzDom.ByValue) Structure.newInstance(AzDom.ByValue.class, body.rawPointer());
bodyRaw.read();
outPtr.write(0, bodyRaw.getPointer().getByteArray(0, bodyRaw.size()), 0, bodyRaw.size());
```
That's a 3-line ritual *per branch* (and the JVM hello-worlds have a `null`-check branch). The user shouldn't write `Structure.newInstance / read / getByteArray / outPtr.write` at all.

**Fix.** The codegen-emitted `LayoutCallbackInvokerCallback` (the SAM) should accept a return value of type `Dom` (the wrapper class) and itself do the `rawPointer → outPtr.write` marshal. The user just `return body`.

The shape: when the IR's callback typedef has `return_type != "void"`, emit a per-kind SAM with signature `(long id, Pointer data, Pointer info) -> WrapperT` *instead of* the current `(long, Pointer, Pointer, Pointer) -> void`. The thunk wrapper inside `AzulHostInvoker.set<X>Invoker` reads `rawPointer()` from the returned wrapper and copies the bytes into the out-pointer itself.

**api.json metadata:** complete (return_type is known per callback typedef).
**Effort:** 0.5 day for JVM (single helper in `lang_java/managed.rs` + `lang_kotlin/managed.rs`); 0.5 day for C# (same helper in `lang_csharp/managed.rs`). Scala rides on Java.

### CC-3. Static-typed `Optional<T>` / `Result<T,E>` payload extraction
**Affected:** Java, Kotlin, Scala, C#, OCaml (plan items I.5.1, I.5.2, I.5.6 — all open).

**Symptom.** Tag-byte accessors (`isSome` / `isOk`) and `unwrap()` exist but **do not surface a typed payload at the call site**. So users in static-typed bindings still pattern-match the Az-prefixed inner struct's tag field manually:

```csharp
// instead of:    Optional<string> name = AzOptionString.AsNullable(...);
// users today write a peek of the AzOptionString.tag byte and a Marshal.PtrToStructure cast.
```

**Fix.** Codegen emits per-variant typed payload extractors:
- `Optional<T> asOptional()` (Java/Kotlin)
- `T? AsNullable()` (C#)
- `t option` (OCaml)

Using api.json: for an `OptionX`, the IR encodes `enum_fields: [{None: ...}, {Some: {type: T}}]` with `repr: "C, u8"`. The codegen knows the payload type T and the tag-byte offset (0). The alignment-dependent offset of the payload (after tag + alignment-padding) is computable from the type's declared layout — every binding already needs this for its struct emission anyway.

**api.json metadata:** complete (the `Some: {type: T}` schema is the typed payload). Alignment of T can be inferred from `derive`s + size of fields; alternatively, generate a small libazul-side C accessor (`Az<Option>_intoSome(*Az<Option>, *T)`) once and route all bindings through it.

**Effort:** 2-3 days per binding (memory entry's estimate; same on this audit). Per-binding offset-aware emission is the meat of the work; the IR's `is_option_shape` / `is_result_shape` predicates (in `ir_builder.rs` lines 2162/2185) already do the detection — emitter just needs to consume them.

**Highest leverage path:** add a per-Option / per-Result C-ABI helper export from libazul itself (`bool AzOptionString_getSome(const AzOptionString*, AzString* out)`). Then every binding gets the typed extraction by calling that one C function — no per-binding offset math, no per-language layout knowledge. This is a 0.5-day codegen-on-the-libazul-side change that unlocks all 5 affected bindings.

### CC-4. Replace remaining `_az_string` / `raw_dom` / `consume` user-facing leakage
**Affected:** Ruby, Lua, Node (for `_azString`), OCaml (for `raw_*` + `azul_consume`).

**Symptom.** Hello-worlds still call:
- Ruby: `Azul._az_string('Hello World')`, `Azul::Native.az_style_font_size_px(32.0)`, `window.ptr[:window_state][:title] = ...`
- Node: `azul._azString('Hello World')`, `lib.AzWindowCreateOptions_default()` direct
- Lua: `azul._az_string('Hello World')`, `data:clone()` for refany clone (manual reference-count management)
- OCaml: `Azul.raw_dom(...)`, `Azul.raw_app_config(...)`, `Azul.azul_consume(app_config)`, `Ctypes.from_voidp Azul.az_ref_any data_ptr`

**Fix.**
- **Auto-AzString**: every wrapper method that takes `AzString` should accept the host-native string directly. The codegen already does this for *most* methods; the long-tail (button/Dom factories that have explicit string args) should be audited and lifted to call site. Where Ruby/Lua/Node still leak `_az_string`, it's because the per-field assignment `window_state.title = ...` skips the wrapper layer. Fix: emit a `WindowCreateOptions#with_title(s)` smart method (or wrap the whole `window_state` POD in a builder).
- **OCaml `raw_*`**: stop requiring the user to extract `raw_dom (wrapper)` before passing to a method that takes another wrapper. The wrapper-method emitter should accept `wrapper`s directly and unbox internally.
- **OCaml `azul_consume`**: the move-by-value problem (Gc.finalise firing on a struct whose bytes were moved into libazul). The fix is to consume the wrapper automatically when its raw struct is passed to a function that takes-by-value. The IR knows which args are by-value vs by-pointer; thread that through.
- **Lua `data:clone()`**: the user shouldn't need to manually clone the RefAny before passing to `Button.set_on_click(data, fn)`. The smart `Button.on_click(data, fn)` builder (J.1 / A.4) should handle the refcount bump internally.

**api.json metadata:** complete (TypeCategory::String is already populated; arg-by-value vs by-pointer is in the IR via `ref_kind`).
**Effort:** 0.5-1 day. Several plan items (A.1.1, J.3) are precursors; the user-facing trim is mostly emitter follow-up.

### CC-5. CssProperty tagged-union surfaced as wrappers
**Affected:** Java, Kotlin, Scala, C#, Ruby, Node, Lua (every full-GUI binding).

**Symptom.** Hello-worlds still emit raw nested calls into the tagged-union construction:
```ruby
font_size_px = Azul::Native.az_style_font_size_px(32.0)
font_size_prop = Azul::Native.az_css_property_font_size(font_size_px)
```
```javascript
CssProperty.font_size(StyleFontSize.px(32.0))   // technically wrapped, but 3 levels deep
```
```kotlin
.withCss("font-size: 32px;")   // string-based, the *good* path, but only because libazul parses it again
```

**Fix.** Emit a fluent `Css.fontSize(32.0).build()` or a Pythonic kwarg helper (`Dom.create_div(font_size: 32)`). Most bindings already prefer the string-based `.with_css("font-size: 32px;")` path (Java/Kotlin/Scala/Python/C# do); standardising on that one and *removing the typed-CssProperty path from hello-worlds* would shave 2-3 lines from Ruby/Node and clean up Lua.

**api.json metadata:** the CSS string parser is in libazul already (`AzCss_fromString`); this is mostly a docs / hello-world rewrite.
**Effort:** 0.5 day (mostly hello-world rewrites; codegen change is to expose `with_css(String)` as the recommended path and demote the typed-CssProperty path to a power-user API).

### CC-6. Lua / Ruby / OCaml: fluent `with_*` over mutating `add_*` / `set_*`
**Affected:** Lua, OCaml.

**Symptom.** Lua hello-world uses `label_wrapper:add_css_property(...)` (mutator returning nil) instead of `:with_css_property(...)` (returning self). OCaml does worse: `let button = Azul.Button.with_button_type button 1 in ...` re-binds line by line because the return-value chain isn't preserved.

**Fix.** Lua: change the codegen-emitted `_methods` table to make `add_*` *return self* (or just promote `with_*` as the recommended path — the Lua wrapper already emits both). OCaml: same — emit a fluent surface (`Button.create "Increase counter" |> Button.with_button_type 1 |> ...`) and use it in hello-world.

**api.json metadata:** complete (every `with_*` method's IR is a DeepCopy / Method that returns the owning class).
**Effort:** 0.5 day. Pure hello-world rewrites in some cases; codegen-side it's one ".return self" change in Lua's emitter.

---

## Per-language audit

### 1. Haskell — 64 LOC (current, smoke-only because Phase H.2 / C.1 blocked)

**File:** `/Users/fschutt/Development/azul/examples/haskell/HelloWorld.hs`

LOC vs Python: 64 vs 37 (1.7×). But Haskell's smoke layer doesn't run a real GUI — it just exercises the `registerLayoutCallbackTypeCallback` shim, prints status lines, then exits. A *true* full-GUI Haskell hello-world is blocked by:
- **H.2 (Storable plumbing for nested `window_state.layout_callback` splice).** Open in the plan.
- **C.1 libazul macOS webrender crash.** Blocks AZ_DEBUG verification anyway.

**Remaining C-esque patterns:**

1. **`Ptr (T.RefAny ())` phantom-type in the user callback.** User writes `myLayout :: Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> IO T.Dom`. The `Ptr (T.RefAny ())` is a typed pointer to a C struct — user must `peek` / pass through `azul_refany_get`-equivalent helpers to extract their model.
   - **Fix:** emit a higher-order register helper `registerLayoutCallback :: (MyData -> IO Dom) -> IO (FunPtr ())` that bakes a `RefAny` → `MyData` extraction into the inner trampoline. Needs api.json to know which arg is the user-data RefAny (it's always arg 0 for the kinds in `HOST_INVOKER_KINDS`).
   - **api.json metadata:** complete.
   - **Effort:** 1 day. Generalizes to Java/Kotlin/Scala/C#/OCaml as well (cross-cutting CC-1).

2. **`alloca $ \buf -> do c_AzDom_createBody_via buf; peek buf`** — manual outptr/peek for struct-by-value returns.
   - **Fix:** the codegen-emitted Haskell `Dom.createBody :: IO Dom` should hide the `alloca + via + peek` triplet. Phase B.8.1 landed the `_via` shim; H.6 added `azStringToString` round-trip. The pattern needs one more wrapper layer that does the alloca implicitly.
   - **api.json metadata:** complete (return type is known).
   - **Effort:** 1 day.

3. **`nullFunPtr` check after `registerLayoutCallbackTypeCallback`.** User has to defensively branch on `cbPtr == nullFunPtr`. The helper should throw an exception (or return `Maybe`).
   - **Effort:** trivial (0.5h).

4. **Phantom-type `RefAny ()` is the lone surviving J.5 hardcode.** Documented in the plan; Haskell-specific.

**Effort to Python-parity:** Haskell needs (a) the `registerLayoutCallback :: (MyData -> IO Dom) -> ...` higher-order helper, (b) H.2 to wire App.run end-to-end, (c) Maybe/Either payload extraction for Option/Result (H.4/H.5 partial — H.4/H.5 payload extraction follow-up). Then Haskell would land around 40-45 LOC.

---

### 2. C# — 72 LOC

**File:** `/Users/fschutt/Development/azul/examples/csharp/hello-world.cs`

LOC vs Python: 72 vs 37 (1.9×).

**Remaining C-esque patterns:**

1. **Layout callback signature** — `(IntPtr dataPtr, IntPtr infoPtr) => AzDom`. User must `HostInvoker.RefanyGet(dataPtr) as MyDataModel` inside. Same pattern across Java/Kotlin/Scala.
   - **Fix:** CC-1 above. Generic-typed `Data<T>` lets user write `(MyDataModel m, IntPtr info) => Layout(m)`.
   - **Effort:** 2 days (counts across JVM trio + C#).

2. **`HostInvoker.RefanyCreate(_model)`** at line 60 to construct the app's user data. CC-1 again — `App.Create(_model, AppConfig.Create())` should accept the model directly and refany-wrap internally.

3. **Manual `Marshal.AllocHGlobal(Marshal.SizeOf<AzApp>())` + `Marshal.StructureToPtr(appRaw, appPtr, false)` + `NativeMethods.AzApp_run(appPtr, rawWco)` + `Marshal.FreeHGlobal`.** Six lines of C#-specific pointer ceremony.
   - **Fix:** CC-5 — emit a `App` wrapper class with `Run(WindowCreateOptions wco)` instance method that owns the pointer internally. Today the codegen emits `AzApp_run` as a static extern but doesn't synthesise a `class App { public void Run(WindowCreateOptions wco) { ... } }` wrapper that does the marshal-pin-free dance.
   - **api.json metadata:** complete (the function's class_name is `App`, its first arg is `&mut App`).
   - **Effort:** 0.5 day.

4. **`wco.Raw` extraction** + `new Dom(button.Dom())` re-wrap at the .Raw boundary. Reflects the underlying wrapper-vs-AzStruct duality. The user shouldn't have to extract `.Raw` at all — `App.Create / App.Run` should accept wrappers transparently.

5. **`(int)AzUpdate.RefreshDom`** cast. Plan item A.2.5 landed this as a workaround for an enum-vs-int mismatch in the SAM signature. CC-2 (inline `AzDom`-by-return) makes the cast unnecessary because the SAM would accept `Update` directly.

**Effort to Python-parity:** ~3-4 days of focused codegen if CC-1 + CC-2 + CC-5 + per-binding cleanup (4-5 lines saved each) all land. Final hello-world reaches ~45 LOC.

---

### 3. Java — 83 LOC

**File:** `/Users/fschutt/Development/azul/examples/java/HelloWorld.java`

LOC vs Python: 83 vs 37 (2.2×). Java is the worst of the static-typed group because JNA needs the `Structure.newInstance + read + write` triplet that C# packages into `Marshal.StructureToPtr` in one line.

**Remaining C-esque patterns:**

1. **Three lines of JNA struct splice per layout branch** (lines 48-50, 66-68 in hello-world).
   ```java
   AzDom.ByValue bodyRaw = (AzDom.ByValue) Structure.newInstance(AzDom.ByValue.class, body.rawPointer());
   bodyRaw.read();
   outPtr.write(0, bodyRaw.getPointer().getByteArray(0, bodyRaw.size()), 0, bodyRaw.size());
   ```
   **This is the single biggest source of C-esque leakage in Java.** Six lines just to push two return DOMs through the layout-callback out-pointer.
   - **Fix:** CC-2 — the SAM emitted by `lang_java/managed.rs::emit_invoker_callbacks` should accept `Dom` (the wrapper class) and do the splice internally. User writes `return body;`.
   - **api.json metadata:** complete.
   - **Effort:** 0.5 day. **Highest single-binding leverage on the audit.**

2. **`SAM signature` is `(long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> void`.** Should be `(MyDataModel data, LayoutCallbackInfo info) -> Dom` once CC-1 + CC-2 land.

3. **`AzulHostInvoker.refanyGet(dataPtr)` + `instanceof` check** inside every callback. Same as C# / Kotlin / Scala. CC-1.

4. **`Structure.newInstance(AzWindowCreateOptions.ByValue.class, wco.rawPointer()); rawWco.read(); ...; AzulNativeApp.AzApp_run(app.getPointer(), rawWco);`** — six lines of JNA app-construction ceremony, line 75-81. CC-5.

5. **`AzUpdate.RefreshDom.value`** (the `.value` accessor on the enum). Java enum doesn't auto-coerce to int; the codegen surfaces an explicit `.value` int field. Acceptable Java idiom but not free.

6. **`package com.azul` + `import com.sun.jna.{Pointer, Structure}`** plus the per-binding `java.lang.String.valueOf(m.counter)` workaround for the `String` shadow problem (noted in plan "Notes for the agent" → `Java class String shadows java.lang.String`).
   - **Fix:** rename the codegen-emitted `String` wrapper class to `AzString` or `AzulString` to stop shadowing `java.lang.String`. Plan item not explicitly tracked.
   - **Effort:** 0.5 day. Generalizes to Kotlin/Scala/C# (the latter doesn't have the shadow but follows naming convention).

**Effort to Python-parity:** ~3-4 days. CC-1 + CC-2 + CC-5 + String-shadow rename brings Java to ~50-55 LOC.

---

### 4. Kotlin — 64 LOC

**File:** `/Users/fschutt/Development/azul/examples/kotlin/HelloWorld.kt`

LOC vs Python: 64 vs 37 (1.7×). Best of the JVM trio because Kotlin's SAM/lambda syntax is terser than Java's.

**Remaining C-esque patterns:**

1. **Same `writeDom(outPtr!!, dom)` helper** as Java's JNA splice (3 lines). User wrote a helper to avoid duplicating, but the codegen should hide it. CC-2.

2. **Same `AzulHostInvoker.refanyGet(dataPtr)` + `is MyDataModel` Smart cast** as Java. CC-1.

3. **`AzWindowCreateOptions.ByValue::class.java` cast** + same `Structure.newInstance` ceremony at App-init (lines 57-63). CC-5.

4. **`AzUpdate.RefreshDom.value`** same as Java. (Kotlin enums can carry properties; could be cleaner if emitted as `enum class AzUpdate(val value: Int) { ... }` with `operator fun toInt()` — Kotlin allows the cast.)

5. **`m.counter.toString()`** explicit. Python writes `str(data.counter)`; this is just a language idiom difference.

**Effort to Python-parity:** ~2 days (rides on the Java cross-cutting work). Final ~40-45 LOC.

---

### 5. Scala — 72 LOC

**File:** `/Users/fschutt/Development/azul/examples/scala/HelloWorld.scala`

LOC vs Python: 72 vs 37 (1.9×). Scala uses the Java bytecode directly (no separate `lang_scala/` codegen), so every Java improvement maps automatically.

**Remaining C-esque patterns:** identical to Java. Scala's `new AzulNativeManaged.LayoutCallbackInvokerCallback { override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit = ... }` is the SAM-instantiation idiom — could collapse to a Scala 3 method-handle (`(id, dataPtr, infoPtr, outPtr) => ...`) once Scala-3 codegen output is verified.

**Specific Scala wins** that are NOT just "Java passthrough":
- **`match` statement on `refanyGet(dataPtr)`** is actually nice (idiomatic Scala) — keep it.
- **`java.lang.String.valueOf(m.counter)`** same workaround for the String-shadow. Renaming the wrapper kills it.

**Effort to Python-parity:** 0 incremental cost; rides on Java.

---

### 6. OCaml — 77 LOC

**File:** `/Users/fschutt/Development/azul/examples/ocaml/hello_world.ml`

LOC vs Python: 77 vs 37 (2.1×).

**Remaining C-esque patterns:**

1. **`Ctypes.from_voidp Azul.az_ref_any data_ptr`** — the user has to manually cast `unit Ctypes.ptr` to a typed `az_ref_any structure ptr`. Same `unit Ctypes.ptr -> typed pointer` pattern across both callbacks (lines 26, 35).
   - **Fix:** emit per-kind callback wrappers that take `unit Ctypes.ptr -> unit Ctypes.ptr -> ...` and internally do the `from_voidp` + `azul_refany_get` cascade, handing the user the unwrapped model directly. CC-1.
   - **api.json metadata:** complete.

2. **`Azul.raw_dom (Azul.Dom.create_body ())` ×5 places.** The user is unwrapping a `dom` record back to its `az_dom Ctypes.structure` field to pass through `with_child` / `App.create` / `App.run`. This is the **OCaml-specific accessor leakage** — the codegen emitted a high-level `dom` record AND requires the user to extract `.raw` (via the `raw_dom` helper) before passing back to another method.
   - **Fix:** the wrapper-method emitter (`lang_ocaml/wrappers.rs`) should accept *wrapped* args, not raw `Ctypes.structure`. The OCaml binding has a structural mismatch: high-level wrappers vs the low-level `Ctypes.structure`-typed function args. Refactoring the emitter to make `Dom.with_child : dom -> dom -> dom` (instead of `dom -> az_dom Ctypes.structure -> dom`) takes 2-3 days.
   - **api.json metadata:** complete.

3. **`Azul.azul_consume app_config`** — explicit consume marker before App.run. Memory entry calls this out as a known OCaml-specific gotcha (Gc.finalise firing after move-by-value). The user has to know to call it.
   - **Fix:** make the wrapper method that consumes-by-value automatically mark the input consumed (set `consumed = true` flag in the wrapper, skip the finaliser). Plumbed via the IR's `ref_kind` field on each arg.
   - **Effort:** 0.5 day.

4. **`azul_window_create_options_with_layout layout`** — a special-case binding-side helper that exists *because* the OCaml smart `WindowCreateOptions.create` was deferred (A.3.7 in plan). The user's code knows about it; cleanup is reachable once the codegen-side smart factory lands.

5. **Manual `match Azul.azul_refany_get ref_ptr with | None -> ... | Some m -> ...`** — OCaml is nicer here than the JVM trio because the pattern-match is native, but still: the user is doing the option-unwrap manually because the codegen doesn't auto-extract the typed payload (I.5.6 open in plan).

**Effort to Python-parity:** OCaml is the messiest — the structural wrapper-vs-raw split needs a focused refactor. ~1 week of dedicated codegen work to bring OCaml to ~45 LOC. The plan estimates this at 1 week ("OCaml tagged-union rewrite") and it sounds right.

---

### 7. Ruby — 62 LOC

**File:** `/Users/fschutt/Development/azul/examples/ruby/hello-world.rb`

LOC vs Python: 62 vs 37 (1.7×). The best-ergonomics dynamic-typed binding.

**Remaining C-esque patterns:**

1. **`data = Azul::RefAny.wrap(model)` followed by `Azul::RefAny.unwrap(data_ptr)`** inside callbacks (lines 20, 23, 30). The user explicitly wraps/unwraps. CC-1.

2. **`Azul::Native.az_style_font_size_px(32.0)` + `Azul::Native.az_css_property_font_size(font_size_px)`** — the CssProperty tagged-union doesn't have a Ruby wrapper class; user calls the C symbols directly. CC-6.

3. **`window.ptr[:window_state][:title] = Azul._az_string('Hello World')`** + 4 more `.ptr[:...][:...]` assignments (lines 55-59). The user is reaching into FFI struct fields by symbol-key, bypassing the wrapper layer.
   - **Fix:** emit a fluent `WindowCreateOptions#with_title(s)`, `#with_size(w,h)`, `#with_decorations(...)`, `#with_background_material(...)` builders that consume host-native types. CC-4.
   - **api.json metadata:** complete (each is a typed field on `WindowCreateOptions.window_state.*`).
   - **Effort:** 0.5 day per binding (Ruby + Lua + Node share the same pain).

4. **`Azul::Dom.new(button.dom)`** to re-wrap the AzDom struct returned by `.dom` — Ruby's wrapper class needs a constructor from the raw struct. Should be automatic (Ruby could emit a `.dom` method on Button that wraps internally).

**Effort to Python-parity:** ~1 day (CC-1 + CC-4 + dom rewrap). Final ~40-45 LOC.

---

### 8. Node — 101 LOC

**File:** `/Users/fschutt/Development/azul/examples/node/hello-world.js`

LOC vs Python: 101 vs 37 (2.7×). Plan-tail snapshot table says Node had only a -6% reduction this session; this is the worst-ergonomics full-GUI binding by LOC.

**Remaining C-esque patterns:**

1. **`const lib = azul.__lib;`** then `lib.AzWindowCreateOptions_default()`, `lib.AzRefAny_clone(data)` — the user is reaching into the raw `__lib` (koffi) interface because the wrapper layer is incomplete.
   - The hello-world's comment block explicitly notes "we build the window via `AzWindowCreateOptions_default()` plus direct field assignment rather than `WindowCreateOptions.create(layout)` because the latter goes through `AzWindowCreateOptions_create`, which expects an `AzLayoutCallbackType` (a raw fn pointer) and would discard the host-invoker `ctx`". So the *smart factory exists* (A.3.6 closed) but is bypassed in hello-world for unrelated reasons.
   - **Fix:** the smart factory should be the canonical path; the user shouldn't need `lib.AzRefAny_clone(data)` either (the codegen should clone on demand inside `with_on_click`).

2. **`refanyCreate`, `refanyGet` imported at top.** CC-1.

3. **`window.window_state.layout_callback = registerCallback('LayoutCallback', layout)`** — direct field assignment instead of through a builder. CC-4.

4. **`azul._azString('Hello World')`** — the `_`-prefixed helper is meant to be private. Same as Ruby's `Azul._az_string`. CC-4.

5. **`window.window_state.title = azul._azString('Hello World')` + 4 more direct field assignments** — same pattern as Ruby's `.ptr[:window_state][:title]`. CC-4.

6. **`uncaughtException` handler** — defensive boilerplate for koffi-callback errors. The codegen could wrap every emitted JS-callable thunk in a try/catch that logs cleanly. Currently the comment block in hello-world admits "Without this, an uncaught exception from inside a koffi-registered callback aborts the process (SIGABRT)".
   - **Fix:** the per-kind invoker thunk emitted by `lang_node/managed.rs` should wrap user callback in `try { … } catch (e) { console.error(...); return DoNothing; }`. Move the error guard inside the codegen.
   - **Effort:** 0.5 day. Removes 3 lines from hello-world (lines 86-88).

7. **`return Dom.create_body()`** at the null-data branch and `return ... body()` at success — the layout return is a JS object whose internal struct is what koffi marshals. Should be clean already but commenters note the moving-receiver semantics ("each call moves the receiver"). Not a hello-world line cost, but a footgun for users.

**Effort to Python-parity:** ~1.5 days (CC-1 + CC-4 + per-callback try-catch). Final ~55-60 LOC.

---

### 9. Lua — 94 LOC

**File:** `/Users/fschutt/Development/azul/examples/lua/hello-world.lua`

LOC vs Python: 94 vs 37 (2.5×).

**Remaining C-esque patterns:**

1. **`button:set_button_type(...)` then `button:set_on_click(...)` then `local button_dom = button:dom()`** — mutator-style API. Python uses fluent `Button.create(...).with_button_type(...).on_click(...).dom()` chain.
   - **Fix:** CC-6. The codegen emits both `add_*` (mutator) and `with_*` (consuming-and-returning) methods. Lua's hello-world picked the mutator path; switching to `with_*` saves ~4 lines.
   - **Effort:** 0.5h (hello-world rewrite). Or: codegen-side, make the `_methods` table's `add_*`/`set_*` versions also return self.

2. **`button:set_on_click(data:clone(), on_click)`** — explicit refany clone. The smart `Button.on_click(data, fn)` (A.4.5 closed) should hide it; the hello-world bypassed it.
   - **Effort:** trivial rewrite.

3. **`azul._az_string('Hello World')`** + 5 lines of `window.window_state.flags.decorations = ...` direct-field assignment. CC-4.

4. **`azul.refany_get(data)`** inside the click handler / layout — CC-1.

5. **`tostring(m.counter)`** explicit conversion. Same minor language idiom thing.

6. **`local body = azul.Dom.create_body()` + `body:add_child(label_wrapper)` + `body:add_child(button_dom)` + `return body`** — 4 lines to chain three children. Python does it in one fluent chain.

**Effort to Python-parity:** ~1 day (CC-1 + CC-4 + CC-6 rewrite). Final ~60 LOC.

---

### 10. Perl — 49 LOC (smoke-only)

**File:** `/Users/fschutt/Development/azul/examples/perl/hello-world.pl`

Not a real hello-world — smoke test exercising `Azul::FFI::AzString_fromUtf8` and `Azul::refany_create`. Full GUI blocked by plan B.5.1-5 (open: Platypus record-to-pointer spike).

**Remaining C-esque patterns** in the full-GUI version (when it exists):
- The same host-invoker visibility (CC-1).
- Perl-specific: `unpack('J', pack('P', $src))` to extract a string pointer. The codegen should hide this in an `Azul::str_to_ptr($s)` helper.
- B.5.1 already lifts the `out_ptr` passthrough — B.5.2 (Platypus record-to-pointer) is the blocker.

**Effort to Python-parity:** ~2 days of focused codegen work (Platypus record spike + invoker dispatch + hello-world rewrite). Documented in `memory/perl_layout_callback_2026_05_13.md`.

---

### 11. Fortran — 65 LOC (smoke-only)

**File:** `/Users/fschutt/Development/azul/examples/fortran/hello_world.f90`

Smoke test. Full GUI blocked by plan B.7.* (open: tagged-union codegen rewrite).

**Remaining C-esque patterns:** the Fortran binding emits `AzOption*` / `AzResult*` as opaque 12-byte `tag + c_ptr` records (per `memory/fortran_codegen_2026_05_13.md`) — needs an inline-blob-with-`transfer()`-accessors rewrite (plan B.7.1 option b). After B.7.* lands, expect ~70 LOC.

---

### 12. Pascal — 157 LOC

**File:** `/Users/fschutt/Development/azul/examples/pascal/hello-world.pas`

LOC vs Python: 157 vs 37 (4.2×). Worst on the audit, but blocked by libazul (C.1 webrender crash on `AzApp_run`).

**Remaining C-esque patterns:**
- The Pascal binding requires the user to subclass `TAzLayoutCallbackInvoker` (lines 60-63) — a per-callback-kind class hierarchy. Same shape that Java had pre-SAM. The codegen could emit a `TAzAnonLayoutCallback` factory that accepts a Pascal `procedure` reference. Pascal has anonymous procedures (`reference to procedure`); the binding doesn't use them.
- `MakeAzString` helper (lines 70-76) — user-supplied. Could be codegen-emitted.
- Manual `wco.window_state.layout_callback := layout_cb;` + 4 more direct-field assignments. CC-4.

**Effort to Python-parity:** Pascal needs the anonymous-procedure refactor + smart WCO factory + CC-4. ~3 days. Blocked by C.1.

---

### 13. COBOL — 50 LOC (smoke-only)

**File:** `/Users/fschutt/Development/azul/examples/cobol/hello-world.cob`

Smoke-only by design. Plan B.6.2 documents the smoke ceiling: COBOL has no closures + `CALL ... RETURNING` doesn't accept TYPEDEF records, so per-kind dispatchers must live in user `PROCEDURE DIVISION` code (~200 LOC of scaffolding). Accepted as `[—]` won't fix.

---

### 14. PHP — 59 LOC (smoke-only)

**File:** `/Users/fschutt/Development/azul/examples/php/hello-world.php` (FFI path) and `examples/php/hello-world-ext.php` (extension path, 142 LOC).

FFI path: smoke-only by language constraint (PHP-FFI rejects closure-to-fnpointer cast). Extension path: blocked by B.1.3 (smart factory needs libazul-side `AzApp_setLayoutCallbackInvoker` or ext-php-rs Zval-decode). See `memory/php_b13_smart_factory.md`.

**Effort to Python-parity:** B.1.3 + C.1 needed first. After that, ~3 days.

---

### 15. Smalltalk — 54 LOC (smoke-only)

**File:** `/Users/fschutt/Development/azul/examples/smalltalk/HelloWorld.st`

Smoke-only. Blocker is `memory/smalltalk_tonel_blocker.md` — codegen emits one combined `Azul.st` instead of a Tonel package directory. Plan B.9.2 accepts the smoke ceiling.

---

## Implementation roadmap (recommended order)

Order chosen for `(lines saved × number of bindings touched / hours)`.

| Phase | Item | Bindings | Lines saved (est.) | Hours |
|-------|------|----------|--------------------|-------|
| 1 | CC-2: Inline `AzDom`-return through invoker out-ptr | Java, Kotlin, Scala, C# | 5-6 per binding | 4 h |
| 2 | CC-5: `App.Run(wco)` instance wrapper hiding pointer dance | Java, Kotlin, Scala, C# | 4-6 per binding | 4 h |
| 3 | CC-4: `WCO#with_title / with_size / with_decorations` builders + drop `_az_string` | Ruby, Node, Lua | 4-5 per binding | 6 h |
| 4 | CC-1: Generic-typed `Data<T>` + typed-user-callback signature | All FFI bindings | 2-3 per binding | 1.5 days |
| 5 | CC-3a: Libazul-side `Az<Option>_intoSome(out)` C exports | All FFI bindings | 3-4 per binding (Option/Result-heavy code paths) | 0.5 days libazul + 1 day per binding |
| 6 | CC-3b: Per-binding typed `Optional<T>` / `T?` / `t option` extraction | Java, Kotlin, Scala, C#, OCaml | (callback signature cleanup) | rides on #5 |
| 7 | CC-6: Lua fluent `with_*` over `add_*` in hello-world; OCaml `\|>` chain rewrite | Lua, OCaml | 2-3 per binding | 2 h (hello-world rewrites) |
| 8 | Java `String` class rename to `AzulString` to stop shadowing `java.lang.String` | Java, Kotlin, Scala | 1-2 per binding | 4 h (codegen + per-binding test) |
| 9 | OCaml: stop requiring `raw_dom` extraction at user-facing boundaries | OCaml | 5-7 | 2-3 days |
| 10 | Haskell H.2: Storable plumbing for nested layout_callback splice | Haskell | unblocks full-GUI E2E | 1 day (+ C.1 libazul) |

Phases 1-4 alone (~3 days of codegen work) bring every E2E-passing static-typed binding to ~50 LOC and every dynamic-typed binding to ~45 LOC — within 1.3× of Python.

## Phases NOT to pursue

- **PyO3-like native extensions** for languages with poor extension stories (Ruby, OCaml). Tried before; abandoned because the FFI path is simpler and more portable. The user's stated goal of "as well-integrated as Python" should be read as **surface ergonomics**, not architecture.
- **Replacing the host-invoker pattern with per-VM-pinned-thunks** (Cython/CFFI-style). The architecture decision is settled (`memory/host_invoker_pattern.md`). Don't relitigate.
- **Tagged-union payload extraction by per-binding offset math.** Use the libazul-side C accessor approach (CC-3a) instead. Faster, less brittle, generalizes.

## What api.json already exposes

Sufficient metadata for every cross-cutting fix above:
- `derive` on every struct/enum → `TypeTraits` (`is_partial_eq`, `is_hash`, `is_debug`, `is_clone`).
- `repr` on every tagged union → tag byte width.
- `enum_fields[<variant>]: {type: T}` → typed payload schema.
- `callback_info.callback_wrapper_name` on every function arg → host-invoker kind.
- `ref_kind` on every arg → by-value vs by-pointer (for consume-on-pass detection).
- `TypeCategory::{String, Option, Result, Vec, RefAny, ...}` (already populated; J.3 / J.4 closed most hardcodes).

## What api.json does NOT carry

The two remaining gaps:
1. **Per-variant alignment-aware payload byte offset.** Not in api.json directly; computable from field sizes + Rust's `#[repr(C, u8)]` layout rules, but error-prone per language. Better solved by adding libazul-side C accessors (CC-3a).
2. **"Which arg is the user-data RefAny" for non-host-invoker callbacks.** Today HOST_INVOKER_KINDS implicitly assumes arg 0 is the user-data RefAny. For custom downstream callbacks added via `impl_managed_callback!`, this is also true by convention. Not a gap — convention is consistent.

## Risk and validation

- Every cross-cutting change above maps to plan items still open (I.5.1-2, A.5, A.7.6 etc.) or to closed-but-only-partially-realized items (A.5.1-8, J.5's lone holdout).
- The `scripts/test_all_e2e.sh` runner (E.1/E.2) drives AZ_DEBUG 5→8 verification for 4 bindings on-machine; every codegen change above should re-run this suite before commit.
- Memory entries `memory/auto_conversion_audit.md` and `memory/language_audit_2026_05_12.md` are the durable per-binding status; update them as each item lands.

---

*Audit complete. Recommended next session: tackle CC-2 + CC-5 together (4-5 hours, 4 bindings benefit immediately, AZ_DEBUG re-verification of each).*
