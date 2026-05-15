# Memory-safety audit: Ruby / Node / Lua / OCaml codegens

**Date:** 2026-05-15
**Scope:** Followup to commits `62094b885` (JVM/CLR consume-after-by-value) and `75a1fbcd2` (JVM/CLR Option/Result heap leak). Audits whether the same two latent bug classes (and their close cousins) exist in the four dynamic-binding codegens. **Read-only audit; no code modified.**

**Key files inspected**

- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_ruby/{wrappers,managed,types,functions}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_node/{wrappers,managed,types,functions,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_lua/{wrappers,managed,mod}.rs`
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_ocaml/{wrappers,managed,types,functions}.rs`

**Verdict at a glance**

| Issue | Ruby | Node | Lua | OCaml |
|---|---|---|---|---|
| 1. Consume-after-by-value (self) | OK | OK | **MISSING** | **MISSING (helper defined, never called)** |
| 1b. Consume-after-by-value (owned wrapper args) | OK | OK | **MISSING** | **MISSING** |
| 2. Option/Result outer struct freed | **NO** (leaks) | **NO** (leaks) | **NO** (leaks) | n/a — bare struct returned, user manages |
| 2b. Option/Result wrapper-payload cloned before outer-free | **NO** (would dangle) | **NO** (would dangle) | **NO** (would dangle) | n/a |
| 3. AzVec iterator clones each element | **NO** (FFI overlay) | **NO** (koffi borrow) | **NO** (cdata borrow) | n/a (no iterator emitted) |
| 4. Smart factory consumes RefAny/wrapper args | partial | partial | partial | partial |
| 5. Other consume sites | several | several | several | several |

The Lua and OCaml bindings are the worst — neither emits any consume call at all, despite both runtimes having `__gc` / `Gc.finalise` registered on every wrapper.

---

## RUBY

Ruby is the language whose mechanism inspired the JVM/CLR patch series (`Azul._consume` undef-finalizes + nils `@ptr`). It is **mostly** wired up, but with three specific gaps.

### 1. Consume after by-value transfer — OK

- `Azul._consume(val)` is defined in `lang_ruby/managed.rs:96-107`. Correctly:
  1. Calls `ObjectSpace.undefine_finalizer(val)` (rescues StandardError if not a finalizer-registered object).
  2. `val.instance_variable_set(:@ptr, nil)` to defang any subsequent method calls.
- `wrappers.rs:522-530` builds `consumed_names` from owned-by-value wrapper-typed args (`ArgRefKind::Owned`, skipping callback args). Threads into the call-site emission at `wrappers.rs:684,698,708,724,734,756,769,789` (every branch of both `emit_method_body_instance` and `emit_method_body_static`).
- For `returns_self_type && consumes_self`, line `wrappers.rs:710-716` also undefines the receiver's finalizer and nils `@ptr` — the JVM/CLR `__consume` equivalent on `self`.

**No action needed for Ruby on the consume side.**

### 2. Option/Result payload extraction — leaks outer Option/Result

- `lang_ruby/types.rs:615-621` (AzOption.to_opt) returns `self[:payload][:Some][:payload]` — **never calls `AzOption<T>_delete`**.
- `lang_ruby/types.rs:638-651` (AzResult.unwrap) same shape — **never calls `AzResult<T,E>_delete`**.
- For an `Optional<String>`-style return, this leaks the inner AzString's `vec.ptr` buffer on every call (mirrors JVM/CLR I.5.1 bug pre-`75a1fbcd2`).
- The returned payload borrows from the Option's FFI::Struct memory. For wrapper-class payloads (`Optional<Dom>`-shaped), this is even worse: when the outer `_ret` Ruby variable goes out of scope, the inner pointers it borrowed are also gone. The returned wrapper instance dangles.

**Severity:** medium-high — same JVM/CLR I.5.1 bug, applies to every Option/Result-returning method that Ruby auto-unwraps via `_ret.to_opt` / `_ret.unwrap`.

**Where to fix:** `lang_ruby/types.rs:615-621` and `:638-651`. Mirror the JVM `format_option_delete_call_*` / `format_clone_call_*` pattern from commit `75a1fbcd2`. For wrapper-class payloads, must `_clone` the payload before `_delete` of the Option/Result so the wrapper owns independent allocations.

### 3. AzVec iterators — borrow-not-clone for struct elements

- `lang_ruby/wrappers.rs:285-294` (`emit_rb_each_if_vec`, struct-element branch):
  ```
  yield Native::AzFoo.new(buf + i * elem_size)
  ```
  This is an **FFI::Struct overlay** — it borrows the bytes at `buf + i*size`, no copy. When the Vec wrapper is finalized (or its `_delete` called), the buffer is freed and every yielded element becomes a dangling overlay.
- Primitives (`wrappers.rs:282`) are read by value via `read_uint32` etc. — those are safe.
- `lang_ruby/types.rs:480-494` `to_a` accessor on AzVec\<T\>: same issue; struct fallback returns `self[:ptr] + i*size` (a raw FFI::Pointer arithmetic offset, not a copy).

**Severity:** medium — bug only triggers when a Vec wrapper is closed before the caller finishes iterating over a captured collection of elements. Mirrors the JVM/CLR Vec-iterator latent leak flagged in commit `62094b885`'s "pre-existing latent leaks NOT addressed".

**Where to fix:** `wrappers.rs:285-294` and `types.rs:480-494`. If the element type has a `_clone`, emit `yield <Wrapper>.new(Native.az_<elem>_clone(overlay))` so the yielded element owns its allocation.

### 4. Smart factory consume — Ruby

- `Button#on_click(data, fn)` (`wrappers.rs:140-165`): wraps `data` via `Azul::RefAny.wrap(data)` then calls `self.<setter>(data_ref, fn)` — the underlying setter is a normal method call, so its emitted body will consume `data_ref` via the `consumed_names` path. **OK.**
- `WindowCreateOptions.create_with_layout(fn)` (`wrappers.rs:181-199`): doesn't take a wrapper arg, only a callable. **OK.**

### 5. Other consume sites — Ruby

- Closure return path in `managed.rs:275-281`: when the user's callback returns a wrapper instance (e.g. a `Dom` from a layout callback), the invoker memcopies the bytes into `out_ptr` then `Azul._consume(ret)`. **OK.**
- `managed.rs:267-287` numeric-vs-wrapper-vs-FFI::Struct dispatch on `ret`: the FFI::Struct branch (line 282-287, plain `ret.to_ptr` with no wrapper) does NOT consume. If a user callback returns an FFI::Struct directly (uncommon but possible), no consume is needed since FFI::Struct values don't have finalizers — that's actually fine.

---

## NODE (koffi / bun:ffi / Deno)

### 1. Consume after by-value transfer — OK

- `_consume(val)` defined in `lang_node/wrappers.rs:75-87`:
  ```
  val.constructor._registry.unregister(val);
  val._ptr = null;
  ```
- `consumed_wrapper_args` (`wrappers.rs:818-824`) collects `ArgRefKind::Owned` args. Threaded into every branch of `emit_instance_method`:
  - `returns_self` builder path: `wrappers.rs:732-740` consumes args + `this._ptr = null` + unregisters `this` from registry.
  - void-return path: `wrappers.rs:751-753` consumes args.
  - Option/Result idiom path: `wrappers.rs:756-760` consumes args before wrapping.
  - Plain return: `wrappers.rs:761-766` consumes args.
- Closure return path in `managed.rs:285-290`: when a callback returns a wrapper instance, unregisters from registry + nulls `_ptr`. **OK.**

**Caveat:** `emit_instance_alias` (DeepCopy/clone path, `wrappers.rs:773-810`) and `emit_static_factory` (`wrappers.rs:826-860`) DO NOT consume their args. Static factories that take owned-by-value wrappers (e.g. `App.create(refany, app_config)` if mapped via this path) would leak. Verify: search for any static factory whose IR `args` contain a wrapper-class arg with `ArgRefKind::Owned`.

**Severity:** low-medium — DeepCopy doesn't consume self per design (mirrors JVM `consumes_self=false`), but static-factory args is a real gap. File `wrappers.rs:826-860`, method `emit_static_factory` — needs a `consumed_wrapper_args` block like the instance method.

### 2. Option/Result outer struct freed — NO, leaks

- `lang_node/mod.rs:166-198` defines `optionToNullable(opt)` and `resultUnwrap(res, label)`. Both:
  - Decode the koffi-side struct.
  - Return `opt.Some.payload` / `res.Ok.payload`.
  - **Never call `lib.AzOption<T>_delete` / `lib.AzResult<T,E>_delete`.**
- Bug identical to Ruby #2 and JVM/CLR pre-`75a1fbcd2`.
- For wrapper-payload Option/Results: payload is a koffi-decoded object whose internal heap pointers point into the original AzOption's payload memory. Returning that object and dropping the AzOption leaves the user with a dangling pointer.

**Severity:** medium-high.

**Where to fix:** `lang_node/mod.rs:166-198`. The helpers can't know the concrete type at call site, so the codegen at `lang_node/wrappers.rs:754-760` needs to be type-aware: emit a `lib.AzOption<T>_delete(_ret)` after extracting, and if the payload is a wrapper class, clone first via `lib.Az<Payload>_deepCopy(...)`. This is a bigger change than the JVM fix since Node's helpers are module-level utilities, not per-type methods.

### 3. AzVec iterators — koffi borrow, no clone

- `lang_node/wrappers.rs:587-602` (`emit_node_iterator_if_vec`):
  ```
  yield buf[i];
  ```
  For primitive elements: `buf[i]` reads a value. Safe.
  For struct elements: `buf[i]` returns a koffi struct view that aliases the buffer memory. If the Vec wrapper is freed (explicit `.delete()` or registry finalizer), the views dangle.

**Severity:** medium. Same latent class as Ruby #3.

**Where to fix:** `wrappers.rs:587-602`. If element type has a `_clone` export, emit `yield new Wrapper(lib.AzElement_deepCopy(buf[i]._ptr_or_address))` so the yielded element owns its allocation.

### 4. Smart factory consume — Node

- `Class.smart_callback_setter` smart method (`wrappers.rs:278-304`): wraps `data` via `refanyCreate(data)`, registers callback via `registerCallback(...)`, calls `this.<setter>(__data, __cb)`. The setter is a normal instance method, so its `_consume(__data)` and `_consume(__cb)` happen via `consumed_wrapper_args`. **OK** (assuming the setter's args show `ArgRefKind::Owned`).
- `WindowCreateOptions.createWithLayout(fn)` (`wrappers.rs:322-330`): only takes a fn, returns `new WindowCreateOptions(opts)`. **OK.**

### 5. Other consume sites — Node

- `lang_node/wrappers.rs:751-753`: void-return path consumes args — comments call out `koffi cannot write back through T*`, which suggests these calls may be no-ops as well as already-broken on the C side. Not a memory issue per se.
- The koffi runtime's `_azStringDecode` / `_azString` helpers are pass-through for non-string types; no consume needed there.

---

## LUA (LuaJIT FFI)

LuaJIT's FFI uses `ffi.metatype` to attach `__gc` to the **ctype**. The handler fires on every cdata instance of that type. **There is no `_consume` helper anywhere in the Lua codegen**, so by-value transfer is universally broken.

### 1. Consume after by-value transfer — MISSING

- `lang_lua/managed.rs` defines no equivalent of `Azul._consume`. Search for `_consume` / `consume` in `lang_lua/`: zero hits.
- `lang_lua/wrappers.rs:504-510` emits:
  ```
  function Class_methods:method(...) return C.<fn>(self, ...) end
  ```
  A pure varargs passthrough. When the C function takes `self` by value (DeepCopy methods, with-builders) or an arg by value, LuaJIT byte-copies the struct into the C call. The original Lua cdata's `__gc` later runs `AzClass_delete(self)` on the moved-out bytes → **double free**.
- Static methods (`wrappers.rs:583-740`, `emit_static_method`) likewise pass owned-by-value wrapper args through without consume.

**Severity:** HIGH — every `App.create(refany, app_config)`-shaped call, every `body:with_child(label)` builder chain double-frees on GC. This is the same root cause that caused the macOS resize crash described in `libazul_resize_crash_2026_05_13.md` (and likely contributes to crashes in other host-binding hello-worlds).

**Where to fix:**
1. Add `azul._consume(cdata)` to `lang_lua/managed.rs`. Implementation candidate:
   ```lua
   function azul._consume(c)
     if type(c) == 'cdata' then
       -- Defang the __gc by zeroing internal pointers so _delete is a no-op.
       -- AzString / AzVec layouts: nil out .vec.ptr / .ptr.
       -- General struct: needs per-type knowledge — alternative below.
     end
   end
   ```
   Pure-Lua approach is awkward because `ffi.metatype` `__gc` can't be unregistered per instance. Two viable options:
   - **(a)** Codegen emits a `azul._<type>_consume(cdata)` per wrapper that zeros the bytes the type's `_delete` would read (e.g. set `cdata.vec.ptr = nil` for AzString). Per-type codegen.
   - **(b)** Switch from `ffi.metatype` to a Lua-table wrapper with a `__gc` proxy (significant refactor; loses the byte-level FFI ergonomics).
2. Once a `_consume` exists, wire it into `wrappers.rs`. Three call sites:
   - `emit_instance_method` lines 504-510 and 530-568: emit `azul._consume(arg)` for each owned-by-value wrapper arg AND for `self` when the method takes self by value.
   - `emit_static_method` lines 583-740: emit `azul._consume(arg)` for each owned-by-value wrapper arg.
   - `_register_callback` invoker (managed.rs:135-175): when the user's closure returns a wrapper cdata, consume after writing it through out_ptr.

### 2. Option/Result outer struct freed — NO

- `lang_lua/wrappers.rs:362-385` (Option methods on tagged-union enum):
  ```
  function <T>_methods:to_opt()
      if self.Some.tag == 0 then return nil end
      return self.Some.payload
  end
  ```
- `lang_lua/wrappers.rs:388-416` (Result methods): same shape, returns `self.Ok.payload`.
- Neither calls `AzOption<T>_delete` / `AzResult<T,E>_delete` after extracting.
- For Option<AzString>: the returned payload's `vec.ptr` is owned by the Option's payload memory. The wrapper has `__gc` registered via `ffi.metatype(... __gc = AzOption<T>_delete)` (wrappers.rs:422-427), so the outer Option WILL be freed eventually. But the returned payload borrows from that memory — by the time the caller uses it, the AzOption's `__gc` may have fired and freed the inner Vec. **Use-after-free.**

**Severity:** medium-high. Mirrors JVM/CLR I.5.1, but worse because LuaJIT's GC is more aggressive than the JVM's.

**Where to fix:** `lang_lua/wrappers.rs:362-385` and `388-416`. Options:
- Clone-payload-first: `local p = C.AzPayload_deepCopy(self.Some.payload); return p` and let the AzOption's own `__gc` free the outer. The cloned `p` becomes an independent cdata (LuaJIT will attach the payload's metatype).
- For AzString payloads: decode to a Lua string via `ffi.string(p.vec.ptr, p.vec.len)` then return the Lua string (no shared memory).

### 3. AzVec iterators — cdata borrow

- `lang_lua/wrappers.rs:241-253` (`to_lua_array`):
  ```
  for i = 0, tonumber(self.len) - 1 do
      t[i + 1] = self.ptr[i]
  end
  ```
  For primitive `self.ptr` (e.g. `*uint8`), `self.ptr[i]` is a value — safe.
  For struct-pointer `self.ptr` (e.g. `*AzDom`), `self.ptr[i]` returns a cdata view aliasing the Vec's buffer. When the Vec's `__gc` runs and the buffer is freed, every entry in `t` dangles.

**Severity:** medium.

**Where to fix:** `wrappers.rs:241-253`. For struct elements, do `t[i+1] = C.AzElement_deepCopy(self.ptr + i)` so each entry owns its allocations (and inherits `AzElement`'s `__gc`).

### 4. Smart factory consume — Lua

- Smart callback setter `:<smart>(data, fn)` (`wrappers.rs:204-223`):
  ```
  local data_ref = azul.refany_create(data)
  return self:<setter>(data_ref, fn)
  ```
  The underlying setter is a regular instance method → also lacks consume per issue #1. Same gap.
- `WindowCreateOptions.create` layout-callback special-case (`wrappers.rs:684-708`): builds via `_default()` + field splice and returns `_opts`. No consume needed since args are not wrappers.
- `LayoutCallback::create`-style passthrough (`wrappers.rs:656-669`): same.

### 5. Other consume sites — Lua

- Pretty much **every emitted method body** is a leak/double-free site for owned-by-value wrappers. The whole binding lacks the mechanism.

---

## OCAML (Ctypes + Foreign)

OCaml is the trickiest case because the helper **exists** but is **never called by the codegen**.

### 1. Consume after by-value transfer — MISSING (helper defined, never called)

- `azul_consume` defined in `lang_ocaml/managed.rs:158-162`:
  ```ocaml
  let azul_consume (a : 'a) : unit =
    Obj.set_field (Obj.repr a) 1 (Obj.repr true)
  ```
  Sets the `disposed` field to `true` on any wrapper record. The `Gc.finalise` callback checks `not a.disposed` before calling `_delete` (`wrappers.rs:255-264`), so this correctly defangs the finalizer.
- **The codegen never emits a call to `azul_consume`.** Search confirms: `azul_consume` appears only in `managed.rs:158` (definition) and `managed.rs:472` (interface). `lang_ocaml/wrappers.rs` has zero hits.
- `emit_method_impl` (`wrappers.rs:584-700`):
  - Line 649-666: detects `self_by_value` (passes `self.raw` directly) — but never calls `azul_consume self` after the C call.
  - Line 673-680: builds `call_args`, never consumes any of them.
  - Line 693-694: `returns_self && has_wrapper` constructs a fresh wrapper via `make_<type>` — but the OLD `self`'s `Gc.finalise` is still armed. Builder chains like `Dom.with_child body label` double-free both `body` and `label` on GC.

**Severity:** HIGH — every consuming-builder chain and every smart-factory with wrapper-typed args double-frees. The helper exists precisely because the original author identified this bug ("manifested as a SIGABRT in U8Vec::drop reachable from App.run → MacOSWindow::new_with_options_internal" — see comment at `managed.rs:144-149`); the codegen just never landed the consumer-call emission.

**Where to fix:** `lang_ocaml/wrappers.rs` — `emit_method_impl` around lines 644-699 needs the same `consumed_wrapper_args` walk as Node/Java/C#:
1. Detect `self_by_value` (already at line 650-654) → emit `let () = azul_consume self in` before the call, or `let r = <call> in azul_consume self; r` after.
2. For each `user_args` with `ArgRefKind::Owned`, emit `azul_consume <argname>` after the call. Skip when the arg is a primitive / AzString string (already routed through `azul_az_string`) / callback (already a struct).
3. For `returns_self && has_wrapper`: ensure `azul_consume self` happens before re-wrapping.

### 2. Option/Result payload extraction — Not auto-unwrapped

OCaml's binding does NOT auto-unwrap Option/Result at the wrapper boundary. Tagged unions like `AzOption<T>` are represented as opaque byte blobs (`types.rs:507-512`); the codegen emits `is_some` / `is_none` / `is_ok` / `is_err` tag-byte helpers (`types.rs:536-567`) but **no payload extractor**.

- User code receives the bare `az_option_xxx Ctypes.structure` and must either:
  - Match by tag and read the variant payload directly via FFI (no extraction helpers emitted).
  - Free via a manually-imported `AzOption<T>_delete`.
- There's no `_delete` call anywhere in the auto-emitted path. The structure value is OCaml-managed-memory on the stack, but its internal heap pointers (`vec.ptr` etc. in Some payloads) leak.

**Severity:** low-medium — leaks the inner payload's allocations on every Option/Result-returning call, but no double-free (no auto-unwrap-then-extract pattern).

**Where to fix (when payload extractor is added):** `lang_ocaml/wrappers.rs:583-700`. Same pattern as JVM `75a1fbcd2`: when a method returns an Option/Result, decode the tag, extract+clone the payload, then delete the outer struct.

### 3. AzVec iterators — none emitted

OCaml's codegen does NOT emit a Seq / list iterator for AzVec wrappers. (Searched `wrappers.rs` and `types.rs` for `Seq.`, `iter`, `to_list`, `to_seq` — no matches.) Users go through Ctypes ptr arithmetic directly. No latent leak here because there's no iterator to be buggy.

**No action.**

### 4. Smart factory consume — OCaml

- `azul_window_create_options_with_layout` (`managed.rs:194-218`): builds via `ffi_az_window_create_options_default()` + field splice and returns the struct. **OK** — no wrapper args.
- Per-kind `azul_register_<X>_callback` (`managed.rs:240-256`): takes a closure (`'a`), allocates a handle, returns the `Az<X>` struct. **OK** — no wrapper args.
- The codegen's emitted `<Class>.create` does take wrapper args (`App.create config`), and those are NOT consumed today. Same gap as issue #1.

### 5. Other consume sites — OCaml

- Per-kind invoker closure (`managed.rs:395-425`): when the user's closure returns a struct (e.g. `Dom` from a LayoutCallback), the invoker extracts the raw structure and writes via `typed_out <-@ ret`. **No `azul_consume`** is called on the returned wrapper, so if the user returned a wrapper record (e.g. `Dom.with_child ...`), the wrapper's `Gc.finalise` will later double-free.

**Severity:** medium — affects every callback that returns a wrapper.

**Where to fix:** `lang_ocaml/managed.rs:405-425`. After writing via `typed_out <-@ ret`, emit `azul_consume <ret-original-binding>`. Needs the codegen to track whether the user's closure is registered through a wrapper record or a bare struct.

---

## Cross-language summary of recommended fix order

**Highest impact (mirrors JVM/CLR `62094b885`):**

1. **Lua** — wire `azul._consume` (define + call). Touches `lang_lua/managed.rs` (add helper) and `lang_lua/wrappers.rs` (emit calls in `emit_instance_method`, `emit_static_method`, callback-return path). Probably contributes to multiple host-binding crashes today.
2. **OCaml** — emit `azul_consume` calls. Touches only `lang_ocaml/wrappers.rs:584-700` and `lang_ocaml/managed.rs:395-425`. Helper already defined. Same severity as Lua but smaller diff.
3. **Node** — static factories with wrapper args. Touches `lang_node/wrappers.rs:826-860` — add a `consumed_wrapper_args` block.

**Medium impact (mirrors JVM/CLR `75a1fbcd2`):**

4. **Ruby** — Option/Result outer-struct delete + wrapper-payload clone. `lang_ruby/types.rs:615-621` (`to_opt`) and `:638-651` (`unwrap`). Mirror the JVM `format_option_delete_call_*` / `format_clone_call_*` helpers.
5. **Node** — Option/Result outer-struct delete + wrapper-payload clone. `lang_node/mod.rs:166-198` (helpers) and `wrappers.rs:754-760` (emission). Helpers need to be made per-type, or replaced with codegen-emitted per-method extraction blocks (mirroring JVM more closely).
6. **Lua** — Option/Result outer-struct delete + wrapper-payload clone. `lang_lua/wrappers.rs:362-385` and `388-416`. Note Lua's `__gc` will free the outer eventually, but the returned payload would dangle in the interim; clone is the safer fix.
7. **OCaml** — Option/Result payload extractor + outer delete. `lang_ocaml/wrappers.rs:583-700` — add an Option/Result idiom branch (today there is none). Lower priority since OCaml doesn't auto-unwrap.

**Lower impact (Vec iterator clones — pre-existing class flagged in `62094b885`):**

8. **Ruby** — `wrappers.rs:285-294` (`emit_rb_each_if_vec` struct branch) and `types.rs:480-494` (`to_a` struct fallback). Clone each yielded element when element type has a `_clone` export.
9. **Node** — `wrappers.rs:587-602` (`emit_node_iterator_if_vec`). Same: clone struct-typed elements.
10. **Lua** — `wrappers.rs:241-253` (`to_lua_array`). Same.
11. **OCaml** — no iterator emitted, no fix needed.

---

## Notes / caveats

- All four bindings correctly handle the `_consume` semantics for AzString conversion (string args are wrapped in `_az_string` helpers that allocate fresh AzStrings; the original string is just a host-native string with no finalizer to worry about).
- Ruby's host-invoker return path is the only one of the four that consistently consumes returned wrappers (`managed.rs:275-281`). Node has it in one place (`managed.rs:285-290`); Lua and OCaml don't.
- The Lua `__gc` mechanism makes the per-instance consume tricky — the cleanest fix is to write `0` / `NULL` into the fields the type's `_delete` would dereference (e.g. for `AzString`, nilling `self.vec.ptr` makes `AzString_delete` a no-op). This requires per-type knowledge but is a smaller codegen change than refactoring away from `ffi.metatype`.
- The OCaml `Obj.set_field`-based `azul_consume` is unsafe in the technical sense (assumes every wrapper record has `disposed` at field index 1), but is structurally uniform across all generated wrappers (`wrappers.rs:246`: `{ mutable raw; mutable disposed : bool }` — `raw` is index 0, `disposed` is index 1). Safe so long as the record shape is preserved.
