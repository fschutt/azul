# Wiring the Racket binding into azul-doc + release + CI

Status: the Racket generator (`doc/src/codegen/v2/lang_racket/`), the example
(`examples/racket/hello-world.rkt`), and the guide
(`doc/guide/en/hello-world/racket.md`) are complete NEW files. This document is
the exact checklist of central-wiring edits needed to make the generator run,
ship the artifacts, and validate via CI. None of these edits are applied yet
(the task scope was NEW files only) — apply them in one follow-up commit.

Racket is **not installed locally**; the binding was constructed by reasoning +
the Racket docs and must be validated through CI (`impl blindly, validate via
CI`). Racket's `ffi/unsafe` is libffi-backed, so callbacks are C-ABI-direct
(archetype A): a `_fun` closure is a real C function pointer. The GC-retention
hazard is handled inside `lang_racket/managed.rs` (invoker closures pinned in
`live-pins`; user callbacks rooted in the `azul-handles` hash).

Guide `guide_order`: **29** (28 was the previous max; 29 is the next free).

---

## 1. Register the module — `doc/src/codegen/v2/mod.rs`

Add the module declaration next to the other `lang_*` entries (keep alphabetical
placement after `lang_python`, before `lang_reexports`, or wherever fits the
existing ordering):

```rust
pub mod lang_racket;
```

Add the public `generate_racket` helper alongside `generate_lisp`
(near the other `generate_<lang>` fns, ~line 325):

```rust
/// Generate Racket (ffi/unsafe) bindings as String. Returns `azul.rkt` source.
pub fn generate_racket(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_racket::generate(&ir, &config)
}
```

No `config.rs` change is required — the Racket generator consumes the standard
`CodegenConfig::c_header()` IR (same as lisp/lua/etc.).

---

## 2. Emit the files — `doc/src/codegen/v2/generator.rs`

In `GenerationTargets::generate_all(...)`, after the Node step (`[35/35]`),
append a new numbered step (bump the total to 36 in the log prefixes if you want
it exact; the count string is cosmetic):

```rust
// 36. Racket (ffi/unsafe) bindings — single azul.rkt + info.rkt package meta.
println!("[36/36] Generating Racket bindings...");
Self::write_string(
    super::lang_racket::generate(ir, &CodegenConfig::c_header())?,
    &codegen_dir.join("azul.rkt"),
)?;
Self::write_string(
    super::lang_racket::pkg::generate_info_rkt(&ir.api_version),
    &codegen_dir.join("info.rkt"),
)?;
```

`generate_info_rkt` threads `ir.api_version` (never hard-code the version — same
rule as rockspec/gemspec/asd).

---

## 3. Ship the artifacts — `doc/src/dllgen/deploy.rs`

Add a `racket` section to the `const BINDING_FILES` array (Racket is NOT a
whitelist language, so it must be listed; both files are `Codegen`-sourced, and
the example driver is `Examples`-sourced):

```rust
// --- racket ---
BindingFile { dst: "azul.rkt", src: "azul.rkt", source: BindingSource::Codegen },
BindingFile { dst: "info.rkt", src: "info.rkt", source: BindingSource::Codegen },
BindingFile { dst: "hello-world.rkt", src: "racket/hello-world.rkt", source: BindingSource::Examples },
```

(Update the doc-comment language list above `BINDING_FILES` to include `racket`.)

---

## 4. Install steps — `api.json` (`installation.languages["racket"]`)

Add under `0.2.0` → `installation` → `languages`, mirroring the `lisp` entry
shape (`$HOSTNAME`, `$VERSION` are substituted at render time). Racket reads
`AZ_LIB_DIR`, so the run step exports it on Unix:

```json
"racket": {
  "displayName": "Racket",
  "platforms": {
    "linux": {
      "description": "Download azul.rkt + native library, then run with Racket (ffi/unsafe is built in).",
      "steps": [
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/libazul.so" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.rkt" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.rkt" },
        { "type": "command", "content": "AZ_LIB_DIR=. racket hello-world.rkt" }
      ]
    },
    "macos": {
      "description": "Download azul.rkt + native library, then run with Racket (ffi/unsafe is built in).",
      "steps": [
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/libazul.dylib" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.rkt" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.rkt" },
        { "type": "command", "content": "AZ_LIB_DIR=. racket hello-world.rkt" }
      ]
    },
    "windows": {
      "description": "Download azul.rkt + native library, then run with Racket.",
      "steps": [
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.dll" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.rkt" },
        { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.rkt" },
        { "type": "command", "content": "racket hello-world.rkt" }
      ]
    }
  }
}
```

Edit `api.json` only via the autofix workflow if the schema requires it; this is
a pure data addition (no source-type sync), so a direct edit is acceptable per
the installation-block precedent.

Do **NOT** add Racket to `FRONTPAGE_LANGUAGES` (`doc/src/docgen/mod.rs`) — it is
an ALPHA-tier binding until CI is green.

---

## 5. e2e matrix — `scripts/e2e_language_matrix.sh`

### 5a. Add to `ALL_LANGS` (~line 90)

Insert `racket` into the alphabetical list:

```sh
ALL_LANGS=(
  ada algol68 c cobol cpp csharp fortran freebasic go haskell java kotlin
  lisp lua node ocaml pascal perl php powershell python racket ruby rust
  scala smalltalk vb6 zig
)
```

### 5b. Tier: ALPHA (automatic)

Do **not** add `racket` to `SHIPPED_LANGS` or `BETA_LANGS`. `tier_of` returns
`alpha` for anything not listed, so Racket is ALPHA by default (smoke/e2e that
never gates `--gate-shipped`). Promote to SHIPPED only after the counter e2e is
green on ≥1 OS with truthful steps + this guide (mirror the 2026-07-04 promotion
rule).

### 5c. Add the `lang_racket()` recipe (near `lang_lisp`, ~line 1022)

```sh
# ---- Racket ------------------------------------------------------------------
# Toolchain: racket (CI: Bogdanp/setup-racket). ffi/unsafe is built in, so no
# extra package install. Racket closures are real C fn-ptrs (archetype A); the
# counter callbacks route through the host-invoker. Callbacks are GC-retained
# by azul.rkt (module-level azul-handles hash + live-pins list).
lang_racket() {
  have racket || { skip racket "racket not installed (Bogdanp/setup-racket, or apt/brew racket)"; return; }
  local f; f="$(log_path racket)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.rkt" "$REPO_ROOT/examples/racket/" 2>/dev/null || true
    cp "$CODEGEN_DIR/info.rkt" "$REPO_ROOT/examples/racket/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/racket/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/racket" || exit 1
    AZ_LIB_DIR=. racket hello-world.rkt
  ) >"$f" 2>&1
  finish racket "racket build/run failed (see log)"
}
```

`have`, `skip`, `finish`, `log_path`, `CODEGEN_DIR`, `LIB_PATH`, `REPO_ROOT`
are the existing matrix helpers (copy the `lang_lisp` shape). The dispatch loops
at lines 1312/1399/1414 iterate `NORM_LANGS` derived from `ALL_LANGS`, so no
extra dispatch wiring is needed once `racket` is in `ALL_LANGS` and a
`lang_racket` function exists.

---

## 6. CI — `.github/workflows/rust.yml`

### 6a. Add `racket` to the `scripting` family langs string (~line 1476)

```yaml
langs: "python lua ruby node csharp java kotlin ocaml go zig ada algol68 cobol fortran freebasic haskell lisp pascal perl php powershell racket scala smalltalk vb6"
```

### 6b. Install Racket in the scripting-toolchain step (~line 1560)

Racket ships `ffi/unsafe`, so the toolchain install is just the interpreter.
Prefer the dedicated action for a pinned, cross-OS install; add before the
matrix run for `matrix.family.key == 'scripting'`:

```yaml
- name: Install Racket (for the racket binding)
  if: matrix.family.key == 'scripting'
  continue-on-error: true
  uses: Bogdanp/setup-racket@v1.11
  with:
    version: 'stable'
```

`Bogdanp/setup-racket` supports ubuntu / macos / windows runners and puts
`racket` on `PATH`. (Alternatively, add `racket` to the best-effort apt/brew
block: `sudo apt-get install -y racket` / `brew install --cask racket` /
`choco install racket -y` — but the action is more reliable across OSes.)

No LuaRocks-style publish job is needed for the ALPHA stage. A future `raco pkg`
publish job (analogous to the `luarocks` job at rust.yml:4208) can package
`azul.rkt` + `info.rkt` once the binding is promoted.

---

## Summary of files touched by the wiring (follow-up commit)

| File | Edit |
|------|------|
| `doc/src/codegen/v2/mod.rs` | `pub mod lang_racket;` + `generate_racket()` |
| `doc/src/codegen/v2/generator.rs` | step 36: write `azul.rkt` + `info.rkt` |
| `doc/src/dllgen/deploy.rs` | `racket` section in `BINDING_FILES` |
| `api.json` | `installation.languages["racket"]` |
| `scripts/e2e_language_matrix.sh` | `ALL_LANGS` + `lang_racket()` recipe (ALPHA auto) |
| `.github/workflows/rust.yml` | `scripting` langs string + `Bogdanp/setup-racket` |

NEW files already created (no wiring, safe to land independently):

- `doc/src/codegen/v2/lang_racket/{mod,types,functions,managed,wrappers,pkg}.rs`
- `examples/racket/hello-world.rkt`
- `doc/guide/en/hello-world/racket.md`
- `scripts/WIRING_racket.md` (this file)
