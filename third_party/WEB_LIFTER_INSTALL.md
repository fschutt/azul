# Installing the web lifter toolchain (local)

The azul web backend (`AZ_BACKEND=web://...`) does not ship a hand-written
WASM build of azul. Instead it *lifts* azul's native machine code to
WebAssembly at server startup, using an embedded
[remill](https://github.com/lifting-bits/remill)-based pipeline:

```
  raw .text bytes
    ── remill-lift-17 ──────────────────►  LLVM IR (%struct.State form)
    ── llc -mtriple=wasm32 -filetype=obj ─►  WASM object
    ── wasm-ld --no-entry --export=<sym> ─►  final WASM module
```

This is only compiled in when azul-dll is built with the `web-transpiler`
(subprocess) or `web-transpiler-static` (in-process) feature. The lifter
shells out to five external binaries that you install **locally** — they are
**not** committed to the repo, and there are **no machine-local absolute
paths in the source**. Without them, `RemillTranspiler::is_available()`
returns `false` and the web backend degrades to server-side dispatch.

This guide is for the **subprocess** path (the default and the one validated
end-to-end on aarch64/macOS and x86_64/Windows). The in-process static-link
path additionally needs LLVM/LLD development libraries linked at build time
via `dll/build.rs` — see `dll/Cargo.toml` near the `web-transpiler-static`
feature.

---

## Binaries the lifter needs

| Tool             | Purpose                                  | Required? |
|------------------|------------------------------------------|-----------|
| `remill-lift-17` | machine code → LLVM IR                    | yes       |
| `llc`            | LLVM IR → wasm32 object                    | yes       |
| `opt`            | per-function `-O2`                        | yes       |
| `llvm-link`      | link lifted IR modules                    | yes       |
| `wasm-ld`        | link wasm objects → module                | yes       |
| `wasm-opt`       | post-link `-Oz` size pass (binaryen)      | optional  |

`llc`/`opt`/`llvm-link`/`wasm-ld` must come from a **matching LLVM/LLD**
toolchain. `remill-lift-17` is LLVM-17-based; `llc`/`opt`/`wasm-ld` may be a
newer LLVM (21 is what the macOS path uses) — the IR remill emits is
forward-compatible.

---

## How the build finds them (discovery order)

Each tool is resolved by `discover_*` in
`dll/src/web/transpiler_remill.rs`, in this order:

1. **Environment variable** (highest priority, always wins):
   `REMILL_LIFT_BIN`, `LLC`, `LLVM_OPT`, `LLVM_LINK`, `WASM_LD`, `WASM_OPT`.
2. **Workspace-relative `third_party/` install** (resolved from
   `CARGO_MANIFEST_DIR`, so cwd-independent):
   - `third_party/remill-install/bin/remill-lift-17[.exe]`
   - `third_party/remill-install/build/remill/bin/lift/remill-lift-17[.exe]`
   - `third_party/remill/dependencies/install/bin/{llc,opt,llvm-link,wasm-ld}[.exe]`
     (Windows: the LLVM 17 the superbuild builds from source)
3. **System defaults** (Homebrew `llvm@21`/`lld@21`, `/usr/local`,
   `/usr/bin`, `/opt/homebrew/bin`).

> `third_party/remill-install/` and `third_party/cxx-common/` are
> `.gitignore`d (they are local build output). The `third_party/remill`
> submodule itself **is** tracked (pinned branch `m12-q-reg-x8-sret`).

The env vars are the most robust option and the one CI / Docker use — set
them and the workspace layout below is unnecessary.

---

## macOS / Linux

### 1. LLVM + LLD (`llc`/`opt`/`llvm-link`/`wasm-ld`)

```sh
# macOS
brew install llvm@21 lld@21 binaryen     # binaryen = optional wasm-opt
# Debian/Ubuntu (apt.llvm.org)
curl -fsSL https://apt.llvm.org/llvm.sh | sudo bash -s -- 21 all
```

These land where the system-default discovery probes them, so no env vars
are needed if you use the versions above. Otherwise point `LLC`, `LLVM_OPT`,
`LLVM_LINK`, `WASM_LD` at your install.

### 2. remill-lift-17

The fork builds against Trail-of-Bits'
[cxx-common](https://github.com/lifting-bits/cxx-common) prebuilt LLVM-17
bundle (macOS arm64 / Ubuntu bundles exist; Windows does **not** — see
below). After building remill, install it into the workspace-relative
location so discovery finds it without an env var:

```sh
cmake --install <remill-build> --prefix third_party/remill-install
# → third_party/remill-install/bin/remill-lift-17
```

or just set `REMILL_LIFT_BIN=/path/to/remill-lift-17`.

---

## Windows (x86_64)

**cxx-common publishes no Windows bundle.** Build the dependencies (including
LLVM 17 + LLD + clang + XED) from the fork's `dependencies/` superbuild, then
build remill with that toolchain. Run everything from a **VS 2022 x64 native
tools prompt** (`vcvars64`); the superbuild needs clang (`__int128`).

A portable toolchain (cmake, ninja) and `vcvars64` wrappers can live outside
the repo; the example wrappers below assume `cmake`/`ninja`/`clang` on PATH.

```bat
:: from third_party/remill, in a vcvars64 shell
::
:: 1. dependencies superbuild (LLVM 17 etc.) — SLOW (~45 min, ~10 GB).
::    Build OUT OF TREE in a SHORT path (in-tree hits Windows MAX_PATH /
::    rc.exe RC1109). Install → third_party/remill/dependencies/install.
cmake -G Ninja -B C:\rb\deps dependencies ^
  -DCMAKE_INSTALL_PREFIX=%CD%\dependencies\install -DCMAKE_BUILD_TYPE=Release
cmake --build C:\rb\deps

:: 2. remill itself, against the superbuild's LLVM 17.
set "INSTALL=%CD%\dependencies\install"
cmake -G Ninja -B C:\rb\remill ^
  -DCMAKE_PREFIX_PATH=%INSTALL% ^
  -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ ^
  -DCMAKE_BUILD_TYPE=Release ^
  -DCMAKE_INSTALL_PREFIX=%CD%\..\remill-install ^
  -DREMILL_ENABLE_TESTING=OFF -DREMILL_BUILD_SPARC32=OFF
cmake --build C:\rb\remill --target remill-lift-17

:: 3. amd64 semantics bitcode. NOTE: --target remill-lift-17 does NOT build
::    the amd64 .bc — build the runtime targets explicitly:
ninja -C C:\rb\remill %CD%\lib\Arch\X86\Runtime\amd64.bc ^
                      %CD%\lib\Arch\X86\Runtime\amd64_avx.bc ^
                      %CD%\lib\Arch\X86\Runtime\x86.bc

cmake --install C:\rb\remill   :: → third_party/remill-install/bin/remill-lift-17.exe
```

remill finds the semantics via the compiled-in build-tree path
(`REMILL_BUILD_SEMANTICS_DIR_X86`), so the build dir must persist; the
install step is only for the `remill-lift-17.exe` binary + headers.

The superbuild's `llc.exe`/`opt.exe`/`llvm-link.exe`/`wasm-ld.exe` end up in
`third_party/remill/dependencies/install/bin/`, which discovery probes
directly on Windows — no env vars required if you keep that layout.

---

## Verify the install

```sh
# Lifts `mov eax, 0x1ba; ret` → IR on stdout. Non-empty = working.
remill-lift-17 --arch amd64 --ir_out - --bytes c704ba01000000
llc --version && wasm-ld --version
```

Then run an azul app under the web backend with the tools discoverable:

```sh
REMILL_LIFT_BIN=.../remill-lift-17 \
  AZ_BACKEND="web://127.0.0.1:8800" ./your-app
```

The startup log prints which lifter binaries were found (or warns that the
transpiler is unavailable).

---

## Lift cache (optional, recommended for repeated runs)

The subprocess path can persist its work to disk:

| Env var             | Effect                                                      |
|---------------------|-------------------------------------------------------------|
| `AZ_LIFT_CACHE=1`   | opt **into** the on-disk object cache (off by default)      |
| `AZ_LIFT_CACHE_DIR` | absolute path for both caches (default `$TMPDIR/az-lift-cache`) |
| `AZ_NO_LIFT_CACHE=1`| disable the raw-IR cache entirely                           |
| `AZ_LIFT_CACHE_CLEAR=1` | clear the cache on startup                              |

The cache is content-addressed by each function's (post-rewrite) machine
bytes, so it survives server restarts and dll relinks that don't change a
function's code. `AZ_LIFT_CACHE_DIR` is what lets the Docker base image
(`docker/Dockerfile`, see `docker/README.md`) bake a *warm* cache at a fixed
location.
