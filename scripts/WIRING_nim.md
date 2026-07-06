# Central wiring for the Nim binding

The `lang_nim` generator, the example, and the guide are self-contained
NEW files. To activate the binding you (the coordinating agent) apply the
edits below to the shared files. They are written so they can be pasted in
without conflicting with other in-flight language wirings — every edit is
an *addition* next to the existing `zig` / `freebasic` lines.

Files created by this task (do not re-create):

- `doc/src/codegen/v2/lang_nim/{mod,types,functions,wrappers}.rs`
- `examples/nim/hello-world.nim`
- `doc/guide/en/hello-world/nim.md`  (guide_order: **29** — next free after perl=28)

---

## 1. Register the module — `doc/src/codegen/v2/mod.rs`

Next to `pub mod lang_zig;` (currently line ~93) add:

```rust
pub mod lang_nim;
```

## 2. Emit `azul.nim` — `doc/src/codegen/v2/generator.rs`

In `GenerationTargets::generate_all`, right after the Zig block (the
`super::lang_zig::generate(...)` + `build_zig` writes, ~line 266-276), add:

```rust
        // Nim bindings — C-ABI-direct: emits type objects, importc/dynlib
        // proc decls, and idiomatic non-prefixed wrappers into one module.
        println!("[..] Generating Nim bindings...");
        Self::write_string(
            super::lang_nim::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.nim"),
        )?;
```

Nim needs no manifest sidecar (the `dynlib` pragma dlopens libazul at run
time), so a single `write_string` is all that is required.

## 3. Deploy the release assets — `doc/src/dllgen/deploy.rs`

In the `BINDING_FILES` table, next to the `// --- zig ---` block
(~line 813) and the `// --- promotion candidates ---` example line
(~line 857), add a Nim block:

```rust
    // --- nim ---
    BindingFile { dst: "azul.nim", src: "azul.nim", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.nim", src: "nim/hello-world.nim", source: BindingSource::Examples },
```

(`azul.nim` is written to `target/codegen/azul.nim`; the example lives at
`examples/nim/hello-world.nim`, which `BindingSource::Examples` resolves
under `examples/`.)

## 4. api.json — `installation.languages["nim"]` + `exampleFiles["nim"]`

### 4a. Add to `installation.languages` (object after `"zig"`, ~line 2784)

```json
                "nim": {
                    "displayName": "Nim",
                    "platforms": {
                        "linux": {
                            "description": "Download the native library, the generated azul.nim binding, and the hello-world, then build with nim c (libazul is dlopen'd at run time via the dynlib pragma)",
                            "steps": [
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/libazul.so" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.nim" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.nim" },
                                { "type": "command", "content": "nim c -d:release hello-world.nim" },
                                { "type": "command", "content": "LD_LIBRARY_PATH=. ./hello-world" }
                            ]
                        },
                        "macos": {
                            "description": "Download the native library, the generated azul.nim binding, and the hello-world, then build with nim c (libazul is dlopen'd at run time; its macOS frameworks come in via the dylib's own load commands)",
                            "steps": [
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/libazul.dylib" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.nim" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.nim" },
                                { "type": "command", "content": "nim c -d:release hello-world.nim" },
                                { "type": "command", "content": "DYLD_LIBRARY_PATH=. ./hello-world" }
                            ]
                        },
                        "windows": {
                            "description": "Download azul.dll, the generated azul.nim binding, and the hello-world, then build with nim c (azul.dll must sit in the working directory)",
                            "steps": [
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.dll" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/azul.nim" },
                                { "type": "command", "content": "curl -O $HOSTNAME/ui/release/$VERSION/hello-world.nim" },
                                { "type": "command", "content": "nim c -d:release hello-world.nim" },
                                { "type": "command", "content": ".\\hello-world.exe" }
                            ]
                        }
                    }
                },
```

NOTE: the `nim c` step does NOT pass `--passL:-lazul` on purpose. The
`dynlib` pragma dlopens libazul at run time, so no compile-time link flag
(and no macOS `-framework` flags) is needed — the library only has to be
discoverable, which is what the `*_LIBRARY_PATH=.` run step provides.
If you prefer a statically-resolved link instead of runtime dlopen, the
alternative is `nim c -d:release --passL:-L. --passL:-lazul hello-world.nim`
(plus `--passL:"-framework Foundation -framework AppKit -framework OpenGL
-framework CoreGraphics -framework CoreText"` on macOS), but the dynlib
route above is simpler and is what the guide + e2e recipe use.

### 4b. Add to `exampleFiles` (map near line ~2991, beside `"zig"`)

```json
                    "nim": "nim/hello-world.nim",
```

### 4c. `installation.tabOrder` (top of api.json, ~line 7) — OPTIONAL / DEFERRED

Leave Nim OUT of `tabOrder` for now: `tabOrder` mirrors the officially
shipped tab set, and Nim is ALPHA (unverified — no toolchain locally).
Promote it into `tabOrder` (e.g. after `"zig"`) only once the CI board
shows the `nim` row green.

## 5. e2e matrix — `scripts/e2e_language_matrix.sh`

### 5a. Add `nim` to `ALL_LANGS` (~line 90), keeping the print order:

```sh
ALL_LANGS=(
  ada algol68 c cobol cpp csharp fortran freebasic go haskell java kotlin
  lisp lua nim node ocaml pascal perl php powershell python ruby rust scala
  smalltalk vb6 zig
)
```

Do NOT add `nim` to `SHIPPED_LANGS` — it must stay ALPHA (never gates CI)
until verified. `tier_of nim` will return `alpha` automatically.

### 5b. Add the `lang_nim` recipe next to `lang_zig` (~after line 900).

The parallel driver dispatches by calling a shell function named
`lang_<lang>` (`declare -F lang_nim`), so the function name is load-bearing:

```sh
# Toolchain: nim (CI: jiro4989/setup-nim-action). The example uses
# {.importc, cdecl, dynlib: "libazul.so".}, so libazul is dlopen'd at run
# time from the loader path the harness already exports (target/release).
lang_nim() {
  have nim || { skip nim "nim not installed (jiro4989/setup-nim-action)"; return; }
  local f; f="$(log_path nim)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.nim" "$REPO_ROOT/examples/nim/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/nim/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/nim" || exit 1
    nim c -d:release --hints:off --warnings:off -o:hello-world-e2e hello-world.nim || exit 1
    ./hello-world-e2e
  ) >"$f" 2>&1
  finish nim "nim c build/run failed (importc dynlib azul.nim)"
}
```

`LIB_PATH` / `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` are already set by the
harness to point at `target/release`, so the runtime dlopen resolves
`libazul` without extra flags. Tier: **ALPHA (unverified)** — reports
WORKS/FAILS/SKIP but never gates `--gate-shipped`.

## 6. GitHub Actions — `.github/workflows/rust.yml`

### 6a. Add `nim` to the scripting family's `langs` list (~line 1476):

```yaml
          - key: scripting
            langs: "python lua ruby node csharp java kotlin ocaml go zig nim ada algol68 cobol fortran freebasic haskell lisp pascal perl php powershell scala smalltalk vb6"
```

### 6b. Install Nim in the `e2e_native` job. Add a dedicated step near the
other best-effort toolchains (before "Install extra language toolchains",
~line 1574). `jiro4989/setup-nim-action` is the maintained action and
covers Linux/macOS/Windows:

```yaml
      - name: Set up Nim (best-effort)
        if: matrix.family.key == 'scripting'
        continue-on-error: true
        uses: jiro4989/setup-nim-action@v2
        with:
          nim-version: '2.0.x'
          repo-token: ${{ secrets.GITHUB_TOKEN }}
```

`continue-on-error: true` + the recipe's `have nim` guard mean that if the
action is unavailable on a given runner the `nim` row simply SKIPs (never
fails the job), exactly like the other alpha toolchains.

---

## What CI validates once wired

1. **Codegen builds**: `azul-doc` compiles `lang_nim` and emits
   `target/codegen/azul.nim` during the normal codegen run (any Rust build
   error in the four new `.rs` files fails the doc/codegen job).
2. **`azul.nim` compiles**: `nim c hello-world.nim` type-checks the entire
   generated module (every `{.bycopy.} object`, `{.union.} object`,
   size-pinned enum, `{.cdecl.}` proc typedef, and `importc` proc decl) —
   a single malformed declaration fails the whole compile.
3. **The counter links + runs headless**: the built binary dlopens
   libazul, upcasts the model into an `AzRefAny`, runs `layout`, and (under
   `AZ_E2E=…hello_world_counter.json`, `AZ_BACKEND=headless`) the harness
   drives a click and asserts the counter increments 5 → 6 — proving the
   direct `{.cdecl.}` callback path works with no host-invoker.
4. **Board row**: the language-binding board prints a `nim | alpha |
   WORKS/FAILS/SKIP` row per OS. ALPHA never gates the deploy, so a red
   nim row is visible but non-blocking until you promote it.
