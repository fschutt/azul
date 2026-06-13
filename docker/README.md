# azul web base image — pre-lifted WASM Docker base

This directory holds the Docker base image that ships a **warm lift cache**
so azul web apps (`AZ_BACKEND=web`) start fast instead of spending minutes
lifting the azul library to WASM on the first request.

> **Status: working, with one robustness item open.** The web lift pipeline
> itself is validated end-to-end on both aarch64/macOS and x86_64/Windows.
> The `AZ_LIFT_CACHE_DIR` override that this image needs (formerly "change
> #1") is now implemented (`lift_cache_root` in `transpiler_remill.rs`). The
> remaining items are the headless prelift harness (change #2) and the
> ASLR-stable cache key (change #1b) — until those land the cache warms on
> first request rather than at build time. Read the "Feasibility verdict"
> below before wiring this into a release.

---

## Feasibility verdict (empirical, with file:line evidence)

### 1. Is the lift cache content-addressed per-function and shareable?

**Partially yes — with one important caveat.**

The on-disk lift cache key is content-addressed by the function's machine
bytes, which is exactly what makes a shared library cache possible:

- `dll/src/web/transpiler_remill.rs:4402` — `lift_cache_path(rewritten_bytes, lift_addr)`
  builds the key as
  `fnv1a64_hex(rewritten_bytes) + "_" + hex(lift_addr) + "_v" + LIFT_CACHE_VERSION`
  and stores it at `std::env::temp_dir()/az-lift-cache/<key>.lifted.ll`
  (`transpiler_remill.rs:4403-4410`).
- The cached artifact is the **raw lifted LLVM IR (`.lifted.ll`)**, not the
  final WASM. A hit is read back at `transpiler_remill.rs:740-745`; a miss
  is written at `transpiler_remill.rs:793-799`.
- The comment at `transpiler_remill.rs:730-734` confirms the design intent:
  "the IR is synth-addressed so it stays valid across restarts + dll relinks
  that don't touch this fn's machine bytes."

So for a fixed `libazul.so`, lifting library function F produces the same
`fnv1a64(bytes)` regardless of which app is running — the byte hash is
app-independent. **That half is genuinely shareable.**

**CAVEAT — the `lift_addr` component of the key is NOT guaranteed stable
across apps.** `lift_addr` is the function's `synthetic_addr`
(`transpiler_remill.rs:1648-1650`, `:2141-2145`), computed in
`dll/src/web/symbol_table.rs:598` (`assign_synthetic_addresses`) as:

```
synthetic_addr = synth_base(image) + (native_loc - native_base(image))
```

- `native_loc - native_base` is the function's **offset within its image**,
  which cancels the ASLR slide → stable (`symbol_table.rs:666-670`).
- BUT `synth_base(image)` is assigned by sorting all **non-system** loaded
  images by their (ASLR-slid) `native_base` and handing out 1 MiB-aligned
  bands in that order (`symbol_table.rs:642-653`,
  `FIRST_SYNTH_BASE=0x10000`).

On a Linux container the non-system images are the **user executable** and
**libazul.so** (`is_system_image` filters `/usr/lib`, `/lib`, ld-linux, vDSO,
etc. — `symbol_table.rs:964-979`). Their relative load order is
ASLR-dependent, so libazul can land at `synth_base=0x10000` in one run and
`0x100000+` in another. When it does, **every library function's `lift_addr`
shifts and the cache key misses** — the warm cache silently fails to hit and
the app re-lifts the whole library anyway.

> This is the one place the original proposal ("always produces the same
> cache entry regardless of the user app") is **not** true today. See
> **Required code change #1** for the minimal fix.

### 2. What gets cached, and how is hit/miss decided?

- **Cached object:** raw lifted IR per function (`<stem>.lifted.ll`).
- **Hit/miss key:** `fnv1a64(post-rewrite machine bytes) + lift_addr + version`
  (`transpiler_remill.rs:4402-4410`). It is the **bytes hash**, not the
  symbol name — good (name mangling churn doesn't invalidate it).
- **A hit skips only the `remill-lift-17` subprocess** — described as "the
  slowest per-fn step" (`transpiler_remill.rs:731`). The downstream
  `opt -O2` → `llc -mtriple=wasm32` → `wasm-ld` passes
  (`produce_object_from_lifted_ir`, `transpiler_remill.rs:808+`) **still run
  every time**, because the `.o`/`.wasm` artifacts live in a **per-process**
  scratch dir `azul-web-transpiler-<pid>` (`transpiler_remill.rs:597-601`)
  and the `.o` `object_cache` is **in-memory only**
  (`transpiler_remill.rs:609`, `:2104`) — neither persists across runs.

  So the warm cache saves the single most expensive step but **not** the full
  per-function cost. Real speedup is large but not "instant"; see
  **Optional change #3** to also persist the WASM.

- **The cache is subprocess-path only.** It is gated on `!use_native`
  (`transpiler_remill.rs:735`). The default web path IS the subprocess path
  (`use_native_remill()` requires both the `web-transpiler-static` build
  feature AND `AZ_NATIVE_REMILL=1`, and is off by default —
  `transpiler_remill.rs:644-647`). Good: the cache applies to the path we
  ship. The non-default native path has no on-disk cache at all.

### 3. Is libazul-with-remill built in CI today?

**No.** No workflow references `web-transpiler-static`, `AZ_NATIVE_REMILL`,
`remill`, or `build_remill` (grep of `.github/workflows/` is empty). The
`build_binaries` job builds `libazul.so` with `--features="build-dll"` only
(`.github/workflows/rust.yml:655-680`, build cmd at the cross/native build
steps). The removed `transpile_blueprint_remill` job was for
`experiments/transpile-blueprint`, not the dll.

**Good news:** the SUBPROCESS cache (the one this image warms) does **not**
require `web-transpiler-static`. It only needs the *external* lifter
toolchain (`remill-lift-17` + LLVM/LLD `wasm-ld`) present at runtime, which
this image installs. So we can warm the cache against the **standard**
`build-dll` libazul — no new libazul variant is strictly required for the
default path. (A `web-transpiler-static` libazul would only matter if we
ever wanted to warm the in-process native path, which currently miscompiles
the whole-library lift; see `use_native_remill` docs at
`transpiler_remill.rs:625-647`.)

### 4. How is the library lift triggered (to run it once at build time)?

The library lift runs **automatically at `run_web` startup**, not via a
dedicated command:

- `dll/src/web/mod.rs:883` calls `lift_eventloop_mini_wasm()` which lifts
  every `EVENTLOOP_SYMBOLS` entry (`mod.rs:45`, `:756-777`) plus their
  transitive Rust deps via `lift_with_transitive_deps_ex`
  (`transpiler_remill.rs:3781`) — this is the bulk of the library lift.
- Layout callbacks are lifted via `lift_layout_callbacks` (`mod.rs:709`).

There is **no standalone "prelift" command**. To warm the cache at image
build time we must run a real (tiny) azul app under `run_web` once and let
it exit. The cleanest lever is `AZ_E2E` (an existing headless one-shot
driver — referenced in `agentic.rs` and the e2e scripts) so the server
renders + dispatches one synthetic event and terminates instead of blocking.

**This harness does not exist yet** — see **Required code change #2**.

---

## Required code changes (small, library-side)

These are NOT made here (new files only); they are specified for the parent
to apply in `dll/src/web/`.

### Change #1 — stable, overridable cache location + ASLR-independent key

1. **DONE.** `lift_cache_root()` in
   `dll/src/web/transpiler_remill.rs` now honours an `AZ_LIFT_CACHE_DIR` env
   var instead of hardcoding `std::env::temp_dir().join("az-lift-cache")`,
   and both `lift_cache_path` (raw IR) and `obj_cache_path` (lifted objects)
   route through it. The image bakes the cache at `/opt/azul/lift-cache` and
   points apps at it directly via `AZ_LIFT_CACHE_DIR`, without hijacking the
   global `TMPDIR`.

   ```rust
   fn lift_cache_root() -> PathBuf {
       match std::env::var_os("AZ_LIFT_CACHE_DIR") {
           Some(p) if !p.is_empty() => PathBuf::from(p),
           _ => std::env::temp_dir().join("az-lift-cache"),
       }
   }
   ```

2. **(1b — still open) Drop `lift_addr` from the cache key, or anchor it to a per-image
   base-relative offset.** Because `synth_base` is ASLR-order-dependent
   (see caveat above), including the absolute `lift_addr` makes the key
   non-portable across runs/apps. The lifted IR is already named
   `@sub_<lift_addr_hex>` and gets *rewritten* to canonical names downstream
   (`rewrite_sub_names_to_canonical`, `transpiler_remill.rs:884`), so the IR's
   *content* is what matters, not the address baked into the filename. The
   robust key is `fnv1a64(bytes) + image_relative_offset + version`, where
   `image_relative_offset = native_loc - native_base` (the ASLR-stable part).
   Alternatively, assign `synth_base` deterministically (e.g. always give
   libazul the same fixed band by sorting on image **path/basename** instead
   of `native_base`) — a 1-line change to the `sort_by_key` at
   `symbol_table.rs:642`.

   **Without one of these, the warm cache will hit only when libazul happens
   to get the same `synth_base` it had at build time — i.e. unreliably.**

### Change #2 — a headless "prelift" harness

Add a tiny binary/example that links libazul, builds a trivial DOM, and runs
`run_web` once under `AZ_E2E` so the full `EVENTLOOP_SYMBOLS` +
layout-callback lift executes and then the process exits. Suggested:
`examples/rust/prelift.rs` (or `cargo run -p azul-dll --bin az-prelift`).
The Dockerfile's Stage 3 invokes it. It needs an `AZ_E2E` JSON that drives
exactly one render+dispatch cycle then stops; a starter
`scripts/m9_e2e/prelift-warm.json` should be added alongside.

### Change #3 (optional, bigger win) — persist the final WASM, not just IR

To make derived apps *truly* fast (skip `opt`/`llc`/`wasm-ld` too), add a
second on-disk cache for the linked per-function `.wasm` keyed the same way
(`fnv1a64(bytes) + image-relative-offset + version`). Today only the IR is
persisted (`transpiler_remill.rs:740-799`) and the `.o`/`.wasm` scratch is
per-pid (`:597-601`). This is the difference between "lift step skipped" and
"whole function served from cache". Out of scope for the first cut.

---

## How CI publishes this image

See `.github/workflows/dockery.yml`. It builds `docker/Dockerfile` and pushes
to `ghcr.io/fschutt/azul-web-base` on tag/release with `packages: write` +
`GITHUB_TOKEN`, tagging by version and `latest`.

## How a user extends it

```dockerfile
FROM ghcr.io/fschutt/azul-web-base:0.1.0
COPY my-app /usr/local/bin/my-app
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
CMD ["/usr/local/bin/my-app"]
```

The first request still lifts the app's own callbacks; azul-library functions
are served from the baked cache when its key matches (reliably once the
ASLR-stable key, change #1b, lands).
