# Codegen language-binding plan

Adding new language bindings to azul-doc's v2 codegen system. Six new targets
in two waves: **S-tier** (large audience, pure host-side FFI) — C#, Ruby, Lua —
and **B/C-tier** (smaller audience, equally easy mechanically, included as a
"universal framework" showcase) — Pascal, Ada, FreeBASIC.

The point of the second wave is not the audience size; it is to demonstrate
that the v2 IR can drive *any* language with a C-ABI-capable FFI mechanism.

## What already exists

- `doc/src/codegen/v2/` — the production codegen pipeline (api.json → IR →
  per-language generator → string output). `lang_c.rs`, `lang_cpp/`,
  `lang_python.rs`, `lang_rust.rs` are wired in via the `LanguageGenerator`
  trait in `generator.rs`.
- `doc/src/codegen/experimental/<lang>_api.rs` — 26 prior-art generators that
  still target the old `crate::api::ApiData` shape. None of them have been
  ported to `CodegenIR`. They are *reference material*, not callable.
- `dll/src/lib.rs` — produces the prebuilt artifact (`azul.dll` /
  `libazul.so` / `libazul.dylib`) that every binding loads at runtime.
- `examples/{c,cpp,python,rust}/` — per-language example folders.
- `api.json` — the source of truth. Two places matter:
  - **Top-level `installation.languages`** — installation steps per language
    (downloads, compile commands, package-manager hints).
  - **Top-level `examples[]`** — each example object has a `code` map keyed by
    language slug pointing to `<lang>/<name>.<ext>`. Adding a language means
    adding a new key per example.

## Per-language deliverables

Each binding lands as a single PR-shaped change with five things:

### 1. `doc/src/codegen/v2/lang_<lang>/` subdir

A directory (not a single file) under `v2/`, modeled on `v2/lang_cpp/` and
`v2/rust/`. Minimum contents:

- `mod.rs` — `pub struct <Lang>Generator;` implementing whatever entry-point
  signature is right (most will *not* implement `LanguageGenerator` because
  that trait targets Rust/C/C++/Python output formats — emit a free function
  `pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String>`
  instead, called from `mod.rs::generate_<lang>` like `generate_python`).
- `types.rs` — struct/enum emission.
- `functions.rs` — function bindings (the FFI declarations).
- `wrappers.rs` — idiomatic wrapper types with destructors (see §"Wrapper
  rules" below).

Wire it in:

- `doc/src/codegen/v2/mod.rs`: add `pub mod lang_<lang>;` and a public
  `generate_<lang>(api_data) -> Result<String>` entry point that mirrors
  `generate_python`.
- `doc/src/main.rs` (or wherever the CLI dispatch lives): add a CLI subcommand
  that writes the output to `target/codegen/v2/azul.<ext>` plus any
  package-manifest artifacts (`azul.csproj`, `azul.gemspec`, etc.).

### 2. Idiomatic wrappers with destructors

This is the hard requirement. The C ABI exposes raw structs and `_delete`
functions. Bindings MUST wrap these in language-native types whose destructor
calls the corresponding `_delete`. Reference: `lang_python.rs` does this with
PyO3 `#[pyclass]` + `__del__`. Per-language conventions:

| Language    | Wrapper construct             | Destructor hook                               |
|-------------|-------------------------------|-----------------------------------------------|
| C#          | `class : IDisposable`         | `Dispose()` + finalizer + `SafeHandle`        |
| Ruby        | `class`                       | `ObjectSpace.define_finalizer` per instance   |
| Lua         | `setmetatable(t, M)`          | `__gc` metamethod                             |
| Pascal      | `class` w/ `destructor Destroy; override;` | called by `Free`                  |
| Ada         | `Ada.Finalization.Controlled` | `overriding procedure Finalize`               |
| FreeBASIC   | `Type` w/ `Destructor`        | called when scope ends                        |

For tagged-union types (the `enum class` pattern with payload) emit either a
language-native sum type if the language has one (Pascal variant records, Ada
variant records, FreeBASIC `Union`), or a class hierarchy (C#, Ruby), or a
plain dispatch on a `tag` field (Lua).

### 3. Idiomatic naming

The generator must rename `AzAppCreate` → idiomatic call sites:

- C#: `Az.App.Create(...)` (namespaced static methods on partial classes)
- Ruby: `Azul::App.new(...)` (module + classes)
- Lua: `azul.App.new(...)` (table-as-module)
- Pascal: `unit Azul; type TApp = class … class function Create(): TApp;`
- Ada: `package Azul.Apps;` with `function Create return App;`
- FreeBASIC: `Type App … Declare Constructor` inside `Namespace Azul`

### 4. `examples/<lang>/hello-world.<ext>`

Port `examples/c/hello-world.c` to the target language using the new wrappers.
Same data model (counter), same callback semantics, same visual output.
Other priority ports if time permits: `widgets.c`, `opengl.c`. Hello-world is
the only one *required* for the first PR.

### 5. `api.json` registration

Three edits per language:

- `installation.tabOrder` — append the language slug.
- `installation.languages.<slug>` — installation steps per platform
  (`linux`/`macos`/`windows`), modeled on `installation.languages.c`.
- Each entry in `examples[].code` — add `"<slug>": "<lang>/<name>.<ext>"`.

**Coordination warning**: `api.json` is a single shared file. Agents working
in parallel must NOT all edit it independently — the merge will conflict.
Plan: each language agent emits a sidecar file
`scripts/api-json-additions/<lang>.json` describing the three patches it
wants, and the orchestrator (me) merges them sequentially after agents
complete.

## Out of scope for the first PR per language

- CI: per-language CI runs come later.
- Package publishing (NuGet, RubyGems, LuaRocks, etc.): the *artifacts* are
  generated; *publishing* them is a separate step.
- Comprehensive widget coverage: hello-world only.
- Native build infra (NAPI, NIFs, R `.Call`): explicitly excluded — those
  languages weren't picked precisely because they need it.

## Subdir scaffolding template

```
doc/src/codegen/v2/lang_<lang>/
├── mod.rs           # entry point: pub fn generate(ir, config) -> Result<String>
├── types.rs         # struct/enum emission
├── functions.rs     # FFI declarations (DllImport / attach_function / cdef / etc.)
├── wrappers.rs      # idiomatic wrappers + destructors
└── (lang-specific manifest emission, e.g. csproj.rs / gemspec.rs)

examples/<lang>/
├── hello-world.<ext>
└── (build manifest if needed: <lang>.csproj / Gemfile / .lua-version / etc.)

scripts/api-json-additions/
└── <lang>.json   # patch description merged into api.json by orchestrator
```

## Wave 1 — S-tier (parallel)

| Lang  | Slug      | FFI mechanism        | Manifest        | Ext   |
|-------|-----------|----------------------|-----------------|-------|
| C#    | `csharp`  | P/Invoke `DllImport` | `azul.csproj`   | `.cs` |
| Ruby  | `ruby`    | `ffi` gem            | `azul.gemspec`  | `.rb` |
| Lua   | `lua`     | LuaJIT `ffi.cdef`    | `azul-1.rockspec` | `.lua` |

Reference experimental file: `doc/src/codegen/experimental/<slug>_api.rs`.

## Wave 2 — B/C-tier (parallel after Wave 1 lands)

| Lang       | Slug        | FFI mechanism                  | Manifest      | Ext    |
|------------|-------------|--------------------------------|---------------|--------|
| Pascal     | `pascal`    | `cdecl; external 'azul';`      | none / `.lpi` | `.pas` |
| Ada        | `ada`       | `Interfaces.C` + `pragma Import` | `azul.gpr`  | `.adb`/`.ads` |
| FreeBASIC  | `freebasic` | `Extern "C"`                   | none          | `.bas` |

Reference experimental file: `doc/src/codegen/experimental/<slug>_api.rs`.

## Acceptance criteria per binding

1. `cargo run -p azul-doc -- codegen <lang>` (or equivalent CLI invocation)
   writes `target/codegen/v2/azul.<ext>` without error.
2. The generated file includes wrapper types with destructors for every
   non-trivial C-API type.
3. `examples/<lang>/hello-world.<ext>` exists and uses the wrappers (no raw
   `Az*` names in user-facing code).
4. `scripts/api-json-additions/<lang>.json` exists with the three patches.
5. The example builds with the language's standard toolchain
   (smoke-test only — no need to wire CI).
