# Wiring the Red (Red/System) binding into azul-doc

The Red generator (`doc/src/codegen/v2/lang_red/`), the example
(`examples/red/hello-world.red`), and the guide
(`doc/guide/en/hello-world/red.md`) are **new files** that touch no shared code.
This document lists the central edits needed to actually generate, deploy, and
CI-test the binding. Each edit mirrors the existing **Fortran** wiring (Fortran
is the closest sibling: host-invoker pattern, single generated file, no package
manifest).

> Status reminder: the binding is **ALPHA / unverified** — no Red toolchain was
> available to compile-check it. Wire it at the ALPHA tier only; do NOT add it to
> `SHIPPED_LANGS`, `tabOrder`, or `FRONTPAGE_LANGUAGES` until a `redc` build of
> the counter runs. See `scripts/RED_FFI_FINDINGS.md`.

---

## 1. `doc/src/codegen/v2/mod.rs`

Register the module and add the generator entry point (mirror `generate_fortran`
at ~line 312).

```rust
pub mod lang_red;                 // near the other `pub mod lang_*;` (~line 71-90)

/// Generate Red/System (red-lang.org) bindings as String. Returns
/// `azul.reds` source.
pub fn generate_red(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_red::generate(&ir, &config)
}
```

## 2. `doc/src/codegen/v2/generator.rs`

In `codegen all` (the numbered emit sequence around line 385, after the Fortran
block), write the single output file:

```rust
// NN. Red/System (red-lang.org) bindings.
println!("[NN/..] Generating Red bindings...");
Self::write_string(
    super::lang_red::generate(ir, &CodegenConfig::c_header())?,
    &codegen_dir.join("azul.reds"),
)?;
```

Bump the `[NN/total]` counters as the surrounding blocks do. No Makefile/manifest
is emitted (Red compiles a single `#include`d file, like the Zig binding needs no
build manifest).

## 3. `api.json`

Three edits (none touch the whitelist/tabOrder):

- **`installation.languages`** — add a `"red"` entry mirroring the `"fortran"`
  object (~line 1053). Per-platform `steps` should `curl` `libazul.{so,dylib}` /
  `azul.dll`, `azul.reds`, and `hello-world.red`, then
  `redc -r hello-world.red && ./hello-world`.
- **example map** (~line 2974, the `"fortran": "fortran/hello_world.f90"` block)
  — add `"red": "red/hello-world.red"`.
- Do **NOT** add `"red"` to the top `tabOrder` array (lines 10-25): that array is
  the SHIPPED set. Red stays ALPHA.

Apply via the autofix workflow if api.json edits are schema-validated in this
repo; otherwise hand-edit these three spots only.

## 4. `doc/src/dllgen/deploy.rs`

Add Red to `BINDING_FILES` (the non-whitelist staging table, ~line 758). Two
entries, next to the `--- fortran ---` block:

```rust
// --- red (Red/System) ---
BindingFile { dst: "azul.reds",       src: "azul.reds",            source: BindingSource::Codegen },
BindingFile { dst: "hello-world.red", src: "red/hello-world.red",  source: BindingSource::Examples },
```

`azul.reds` comes from `target/codegen/` (Codegen); `hello-world.red` from
`examples/red/` (Examples). Missing sources are warnings, not errors, so this is
safe to add before the first codegen run.

## 5. `scripts/e2e_language_matrix.sh` — matrix recipe + ALPHA tier

- Add `red` to `ALL_LANGS` (~line 90), keeping alphabetical-ish order (e.g. after
  `python`/`ruby`, wherever it reads cleanly).
- Do **NOT** add it to `SHIPPED_LANGS` — `tier_of` then returns `alpha`
  automatically, and the CI `--gate-shipped` gate will not block on it.
- Add a recipe function (mirrors the smoke recipes like `lang_cobol`):

```sh
# ---- Red / Red/System (red-lang.org) -----------------------------------------
# Toolchain: the Red toolchain `redc` (no apt/brew package; download the ~1 MB
# binary from red-lang.org). ALPHA/unverified: constructed from the Red/System
# spec, not yet compile-checked. Expected SKIP where redc is absent.
lang_red() {
  have redc || { skip red "redc not installed (download from red-lang.org)"; return; }
  local f; f="$(log_path red)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.reds" "$REPO_ROOT/examples/red/" 2>/dev/null || true
    cp "$LIB_PATH"              "$REPO_ROOT/examples/red/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/red" || exit 1
    redc -r hello-world.red -o hello-world-e2e || exit 1
    ( LD_LIBRARY_PATH=. DYLD_LIBRARY_PATH=. ./hello-world-e2e )
  ) >"$f" 2>&1
  finish red "red ALPHA/unverified (constructed from spec; see RED_FFI_FINDINGS.md)"
}
```

Ensure the dispatcher that maps `$lang -> lang_$lang` picks this up (the script
calls `lang_<name>` by convention).

## 6. CI (`.github/workflows/rust.yml` or the matrix job)

There is **no official GitHub Action** for the Red toolchain. Options:

- Cache-download the `redc` binary from red-lang.org in a `run:` step
  (`curl -L <redc-url> -o redc && chmod +x redc`) and add its dir to `PATH`
  before invoking `e2e_language_matrix.sh`. Pin the release for reproducibility.
- Or leave Red as SKIP in CI until a maintained action exists. Because ALPHA
  bindings never gate `--gate-shipped`, a permanent SKIP is acceptable and does
  not affect green status.

## 7. Docs / promotion lists — leave alone for now

- Do **NOT** add `red` to `FRONTPAGE_LANGUAGES` in `doc/src/docgen/mod.rs`.
- Do **NOT** add it to api.json `tabOrder` or `SHIPPED_LANGS`.

Promotion to BETA/SHIPPED requires: `redc` compiles `azul.reds` + the counter,
the window opens and the counter increments on click (real e2e), and the
64-bit-int / union-sizing caveats in `RED_FFI_FINDINGS.md` are resolved or proven
irrelevant. At that point add the three-list sync (SHIPPED_LANGS, tabOrder,
FRONTPAGE_LANGUAGES) exactly as the 2026-07-04 promotion note describes.

## Follow-up in the generator itself

- **Exact union sizing.** `lang_red::emit_union_opaque` currently emits a
  pointer-width opaque blob (flagged `TODO2`). For ABI-correct by-value passing of
  `AzOption*`/`AzResult*`/union types, wire the shared layout pass the Fortran and
  Pascal bindings use (`lang_fortran::layout::type_layout`) and emit a
  byte-exact opaque struct (e.g. an `integer!` array of `size/4`). The counter
  demo does not exercise this (it only round-trips `AzUpdate`, a unit enum).
- **64-bit integers.** Revisit `map_owned_type`'s `i64`/`u64` → `byte-ptr!`
  mapping once a verified Red/System int64 type is available.
