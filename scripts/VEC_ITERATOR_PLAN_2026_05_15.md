# Vec Iterator → Host-Native Idiom Plan (2026-05-15)

**Started:** 2026-05-15
**Goal:** every codegen-emitted AzVec<T> wrapper yields elements that survive the Vec being closed. Today's iterator implementations across Java/Kotlin/C#/Ruby walk the Vec's internal buffer and overlay element wrappers on top of that buffer. When the user `close()`s / disposes the Vec, those element wrappers carry dangling pointers and segfault or double-free on first use.

This plan mirrors the fix pattern landed in **`75a1fbcd2`** (Option/Result payload extraction): when the payload has a wrapper class, call its `_clone` C-export so the new wrapper owns independent heap allocations, then optionally drop the original container.

## How the autonomous loop uses this file

Same conventions as `OVERNIGHT_PLAN_2026_05_13.md`:

- `[ ]` open — fair game
- `[x]` done — link the commit
- `[⊘]` blocked — append `(blocker: <one line>, memory: <file.md>)`
- `[—]` won't fix this session
- A `→ depends on #N` tag means: don't start until item N is closed
- Verify before checking. Smoke test: iterate vec, close vec, use element ⇒ no segfault, no double-free, no garbled bytes.

---

## Phase V0 — Shared design

### V0.1 Three element shapes drive three emit paths

For every Vec wrapper, the codegen classifies its element T into exactly one of:

1. **Primitive** — `u8 | i8 | u16 | i16 | u32 | i32 | u64 | i64 | usize | isize | f32 | f64 | bool`.
   *Treatment:* eager bulk-copy into the host's native typed array (Java `byte[]`/`int[]`, C# `byte[]`/`int[]`, Node `Buffer`, Ruby native `Array<Integer>`, Lua FFI array, OCaml `Bytes`/`int array`, Haskell `[Word8]`/`[Int]`). No clone, no destructor work. The resulting host array is fully independent of the Vec's lifetime — closing the Vec is fine.

2. **Wrapper-class element** — element has an emitted `wrapper_class_name(T)` AND the IR has `FunctionKind::DeepCopy` (i.e. `AzT_clone` is exported).
   *Treatment:* per-element clone via `AzT_clone(ptr + i*elemSize)`. Each yielded element wraps a freshly heap-allocated `AzT` that the host wrapper owns. Closing the Vec is safe; the element wrappers continue to work and free themselves through their own `close()` / `Dispose()` / GC finalizer.

3. **POD element** — element has a wrapper class (or a typedef / enum) but **no `_clone` export** (Copy types in Rust: small POD structs, unit enums, integer typedefs).
   *Treatment:* by-value byte copy. The element's wrapper struct is small and contains no pointers, so a `memcpy` of `elem_size` bytes into a fresh allocation is enough. This is the same shape as JNA's `Structure.newInstance(ByValue.class, fresh_ptr).read()`. Closing the Vec is safe.

### V0.2 Shared IR-driven predicates (Rust side)

Add to `doc/src/codegen/v2/managed_lang_helpers.rs`:

```rust
/// Returns the element type T if `s` is a codegen-emitted Vec wrapper.
/// Replaces the 5 copy-pasted `detect_vec_elem_type_*` helpers in the
/// per-language modules.
pub fn detect_vec_elem_type(s: &StructDef) -> Option<&str>;

/// One of Primitive | Wrapper | Pod. Drives the per-language emit path.
pub enum VecElemShape<'a> {
    Primitive { rust_name: &'a str, byte_size: usize },
    Wrapper   { rust_name: &'a str, has_clone: bool },
    Pod       { rust_name: &'a str },  // wrapper without _clone, OR enum/typedef
}
pub fn classify_vec_elem<'a>(elem_rust: &'a str, ir: &CodegenIR) -> VecElemShape<'a>;

/// True when `AzT_clone` is exported (FunctionKind::DeepCopy with
/// class_name == T). Used by Wrapper + Pod-fallback paths.
pub fn has_clone_export(elem_rust: &str, ir: &CodegenIR) -> bool;

/// Returns the C-ABI `Az<T>_clone` symbol name when has_clone_export is
/// true. Per-language modules format the callsite — this one only owns
/// the symbol.
pub fn clone_export_name(elem_rust: &str, ir: &CodegenIR) -> Option<String>;
```

- [ ] **V0.2.1** Extract `detect_vec_elem_type` to `managed_lang_helpers.rs`. Migrate the 5 callsites (`lang_haskell/types.rs:477`, `lang_kotlin/wrappers.rs:384`, `lang_java/wrappers.rs:520`, `lang_ruby/wrappers.rs:302`, `lang_csharp/wrappers.rs:443`). *Effort: 1h. Risk: low; all 5 detectors are byte-identical.*
- [ ] **V0.2.2** Add `classify_vec_elem` + `has_clone_export` + `clone_export_name`. No callsite changes yet. *Effort: 1h.*
- [ ] **V0.2.3** Unit test: feed every `Az*Vec` struct from the live IR through `classify_vec_elem`; assert the histogram matches expectations from a quick `grep -E 'Az[A-Z][a-zA-Z]*Vec' api.json` scan. *Effort: 0.5h.*

### V0.3 Per-language matrix (snapshot 2026-05-15)

| Lang     | Current iterator state                                | Memory bug?   | Primitive fast path? | Target |
|----------|-------------------------------------------------------|---------------|-----------------------|--------|
| Java     | `Iterable<T>.iterator()` clone-via path landed (this session) | FIXED       | no                 | typed array primitive path remains |
| Kotlin   | `Iterable<T>.iterator()` clone-via path landed                | FIXED       | no                 | same |
| Scala    | rides on Java bytecode                                | inherits      | inherits             | inherits |
| C#       | `IEnumerable<T>.GetEnumerator()` clone-via path landed        | FIXED       | no                 | typed array primitive path remains |
| Ruby     | `Enumerable + each` yielding `Native::AzT.new(buf+offset)` | **YES** dangling for struct, OK for primitives | yes (per-elem `read_uint8` etc.) | clone wrapper |
| Node     | `*[Symbol.iterator]()` yielding `buf[i]` (koffi)      | depends on koffi’s Buffer/struct semantics — needs verification | partial (koffi `buf[i]` decodes primitives to JS Number; structs decode to JS object detached from native buf) | confirm primitive path; clone wrapper |
| Lua      | `__len` only (no element indexing yet)                | n/a — no per-element access today | n/a | new: `__index` + `ipairs` adapter; clone for wrapper |
| OCaml    | deferred (per plan I.1.9)                             | n/a — no iterator today | n/a | new: per-Vec `to_list` helper with clone |
| Haskell  | `<lower>VecToList :: <X>Vec -> IO [<T>]`              | **PARTIAL** — `peekElemOff __p` returns wrapper carrying a pointer into the Vec's buffer (Storable for wrapper newtypes is shallow); after `_delete` the pointers dangle | no | clone wrapper / bulk peek for primitives |
| Python   | PyO3 auto-exposes `__iter__` via `__len__`/`__getitem__`. Status: verify on a wrapper-element Vec. | unknown | partial (PyO3 maps `Vec<u8>` to `bytes`) | verify |
| Go       | no iterator (cgo direct) — out of scope               | n/a           | n/a                   | n/a    |
| Zig      | comptime FFI direct — out of scope                    | n/a           | n/a                   | n/a    |
| Perl/PHP/COBOL/Fortran/Pascal/Lisp/Ada/Algol68/Smalltalk/VB6/PowerShell | various: no iterator OR blocked OR toolchain-blocked | n/a | n/a | out of scope this plan |

**Gap summary** (post first JVM/CLR pass): Ruby-struct path, Node-wrapper path, Lua (no iteration), OCaml (no iteration), Haskell (Storable shallow peek). 2 unconfirmed languages (Python verify).

---

## Phase V1 — Java (clone-via path landed)

- [x] **V1.1** Refactor `emit_jvm_vec_iterator` (`lang_java/wrappers.rs:551`) to clone via `format_clone_call_jvm` (commit `<this session>`).
- [ ] **V1.2** Primitive bulk-array sibling methods (`toByteArray`, `toIntArray`, …) via JNA `getXxxArray`. *Effort: 1.5h.*
- [ ] **V1.3** Pod byte-copy fallback path (no `_clone` available). Uses `JNA Memory`. *Effort: 1.5h.*
- [ ] **V1.4** Smoke test: `examples/java/test_vec_iter_safety.java` — iterate vec, close vec, use element. *Effort: 2h.*

---

## Phase V2 — Kotlin / Scala (clone-via path landed)

- [x] **V2.1** Refactor `emit_kt_vec_iterator` (`lang_kotlin/wrappers.rs:901`) (commit `<this session>`).
- [ ] **V2.2** Emit primitive `toByteArray()` / `toIntArray()` sibling methods. *Effort: 1h.*
- [ ] **V2.3** Smoke test. *Effort: 1h.*
- [ ] **V2.4** Verify Scala bytecode interop. *Effort: 1h.*

---

## Phase V3 — C# (clone-via path landed)

- [x] **V3.1** Refactor `emit_cs_vec_enumerator` (`lang_csharp/wrappers.rs:866`) (commit `<this session>`).
- [ ] **V3.2** Emit primitive bulk-array sibling methods (`ToByteArray()`, `ToIntArray()`, …) via `Marshal.Copy`. *Effort: 1h.*
- [ ] **V3.3** Smoke test. *Effort: 1h.*
- [ ] **V3.4** Audit other `Marshal.PtrToStructure` callsites in `lang_csharp/wrappers.rs:863-905` for dangling shape. *Effort: 1h.*

---

## Phase V4 — Ruby

- [ ] **V4.1** Refactor `emit_rb_each_if_vec` (`lang_ruby/wrappers.rs:285`) to clone via `Native.az_elem_clone(elem_ptr)` then wrap in fresh `FFI::MemoryPointer`. *Effort: 2.5h.*
- [ ] **V4.2** Keep the primitive `read_uintN` path. *Effort: 0.5h.*
- [ ] **V4.3** Emit `to_a` sibling. *Effort: 0.5h.*
- [ ] **V4.4** Smoke test. *Effort: 1h.*

---

## Phase V5 — Node

- [ ] **V5.1** Refactor `emit_node_iterator_if_vec` (`lang_node/wrappers.rs:587`) into the three-path emit. *Effort: 3h.*
- [ ] **V5.2** `toArray()` / `toBuffer()` sibling methods. *Effort: 1h.*
- [ ] **V5.3** Smoke test. *Effort: 1h.*

---

## Phase V6 — Lua

- [ ] **V6.1** Augment `lang_lua/wrappers.rs:241` to also emit `__index` numeric branch with primitive auto-deref / wrapper clone / pod byte-copy. *Effort: 2.5h.*
- [ ] **V6.2** Per-Vec `to_list()` sibling. *Effort: 0.5h.*
- [ ] **V6.3** Smoke test. *Effort: 1h.*

---

## Phase V7 — OCaml

- [ ] **V7.1** Add Vec detection + emit `to_list : t -> Elem.t list` with per-element clone. *Effort: 3h.*
- [ ] **V7.2** Primitive path: `to_array`. *Effort: 1h.*
- [ ] **V7.3** Smoke test. *Effort: 1h.*

---

## Phase V8 — Haskell

- [ ] **V8.1** Update `emit_vec_to_list_helper` (`lang_haskell/types.rs:505`) to clone per element via `c_AzElem_clone_via`. *Effort: 2h.*
- [ ] **V8.2** Verify wrapper Storable peek semantics match the clone-then-own invariant. *Effort: 1h.*
- [ ] **V8.3** Smoke test under GHC 9.14. *Effort: 1h.*

---

## Phase V9 — Python verification

- [ ] **V9.1** Confirm `__getitem__` + `__len__` emissions on every Vec-shaped wrapper. *Effort: 0.5h.*
- [ ] **V9.2** Smoke test. *Effort: 0.5h.*

---

## Phase V10 — By-reference iteration (deferred / lower priority)

- [⊘] **V10.1** Defer until a perf measurement justifies the borrow path.

---

## Phase V11 — Tests + verification

- [ ] **V11.1** `scripts/test_vec_iter_safety_all.sh` — per-language smoke harness. *Effort: 2h.*
- [ ] **V11.2** Memory entry `vec_iter_safety_2026_05_15.md`. *Effort: 0.5h.*
- [ ] **V11.3** Cross-link from `OVERNIGHT_PLAN_2026_05_13.md` I.1.x rows. *Effort: 0.25h.*

---

## Effort summary (post JVM/CLR pass)

| Phase | Effort  | Bindings  |
|-------|---------|-----------|
| V0    | 2.5h    | shared    |
| V1    | 5h      | Java (primitives + tests remaining) |
| V2    | 3h      | Kotlin/Scala (primitives + tests) |
| V3    | 3h      | C# (primitives + tests) |
| V4    | 4.5h    | Ruby      |
| V5    | 5h      | Node      |
| V6    | 4h      | Lua       |
| V7    | 5h      | OCaml     |
| V8    | 4h      | Haskell   |
| V9    | 1h      | Python    |
| V11   | 2.75h   | shared    |
| **Total** | **~40h** | 10 bindings |

---

## Critical files for implementation

- `/Users/fschutt/Development/azul/doc/src/codegen/v2/managed_lang_helpers.rs` — V0 predicates.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_java/wrappers.rs` — clone-via path landed.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_csharp/wrappers.rs` — clone-via path landed.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_kotlin/wrappers.rs` — clone-via path landed.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_ruby/wrappers.rs` — V4 managed-FFI template.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_node/wrappers.rs` — V5.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_lua/wrappers.rs` — V6 (new emission).
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_ocaml/wrappers.rs` — V7 (new emission).
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/lang_haskell/types.rs` — V8 patch site.
- `/Users/fschutt/Development/azul/doc/src/codegen/v2/ir.rs` — `FunctionKind::DeepCopy` predicate.

End of plan.
