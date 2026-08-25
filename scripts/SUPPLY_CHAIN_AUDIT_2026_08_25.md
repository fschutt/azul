# Supply-chain audit + hardening — 2026-08-25

Branch `security/supply-chain-hardening`, from `origin/master` @ cf5099e17.

## What was done

1. Four new gates, wired into CI **before anything compiles** (`scripts/supply-chain/`).
2. cargo-vet adopted, with the shared audit pool imported.
3. Every crate in azul's tree that executes code at build time was read and
   classified by a reviewer — **192 crate-versions, 13 parallel reviewers**.
4. One live credential exposure in CI found and fixed.

## Headline results

**Zero suspicious crates.** 192 crate-versions audited; none did anything
unexplained by its stated purpose. Independent per-batch verification also
re-hashed the vendored files against `.cargo-checksum.json`: **855 crates, every
file matching what crates.io published**, no additions, no modifications.

**Zero build scripts use network primitives.** No sockets, no HTTP clients, no
`curl`/`wget` shell-outs, no DNS anywhere in the 141 build scripts as configured.
Three crates carry latent, feature-gated network paths (below), all currently
inert.

**Nothing yanked, no checksum drift, nothing suspiciously new.** All 779
crates.io versions in `Cargo.lock` are ≥14 days old once first-party crates are
excluded; every lockfile checksum matches the sparse index.

## Composition

| | count |
|---|---|
| crates in `Cargo.lock` | 773 |
| crates vendored | 855 |
| **with a build script** | **141** |
| proc-macro crates | 50 |
| transitive `[build-dependencies]` closure | 247 |
| **total crates executing code at build time** | **416** |

The 141 build scripts break down as: 38 declare cfg names, 26 emit link
directives, 21 emit cfg flags, 19 generate tables into `OUT_DIR`, 17 compile
C/C++, 14 probe the rustc version, 1 probes pkg-config, 5 other. That ratio —
~16% of crates having a build script — is normal for a tree with this much
native integration; the number is not inflated. It was cross-checked against
`cargo metadata --all-features`, which reports 143 `custom-build` targets, the
extra two being azul's own path crates that `cargo vendor` does not vendor.

## Findings — things a human should decide on

### 1. Build-time execution of binaries chosen by the environment

Six crates execute a binary whose identity comes from `PATH` or an environment
variable, during `cargo build`:

| crate | executes | chosen by |
|---|---|---|
| `pyo3` / `pyo3-ffi` / `pyo3-build-config` | a Python interpreter, script piped to stdin | `PYO3_PYTHON`, `VIRTUAL_ENV`, `CONDA_PREFIX`, `PATH` |
| `mozjpeg-sys` (via `nasm-rs`) | **every `PATH` entry joined with `nasm`**, each run with `-v` until one is new enough | `PATH`, incl. relative entries |
| `find-msvc-tools` | the first `cl.exe` on `PATH`; `vswhere.exe` under `%ProgramFiles(x86)%` | `PATH`, `ProgramFiles(x86)` |
| `clang-sys` (via `bindgen`, in 8 crates' build scripts) | `dlopen`s a `libclang` found on the host | `LIBCLANG_PATH`, `LD_LIBRARY_PATH`, system globs |
| `turso_sqlite3_parser` | compiles vendored `lemon.c` (5,881 lines) to a host binary, then runs it | — (sources in-crate) |
| `tikv-jemalloc-sys` | `sh <bundled autoconf configure>` then `make` | — (script in-crate, Autoconf 2.69, checksummed) |

The last two are self-contained and were verified. The first four are
environment-steerable. **Recommended**: pin `PYO3_PYTHON` and `NASM` to absolute
paths in CI, and set `LIBCLANG_PATH` explicitly.

`nasm-rs` deserves the specific note: it builds its candidate list as `nasm`
chained with each `PATH` entry joined with `nasm`, then *executes* each candidate.
A relative `PATH` entry therefore yields a CWD-relative binary that gets run.

### 2. Latent network fetches (all currently inert)

| crate | what | gate |
|---|---|---|
| `ext-php-rs` | GETs `php-devel-pack-*.zip` from `windows.php.net`/`downloads.php.net`, unzips into `OUT_DIR`, links `php8*.lib` from it — **no checksum, no signature, no version pin**; URL derived from the local `php -i` | `cfg(windows)` + `php-extension` feature |
| `oboe-sys` | downloads a prebuilt tarball from GitHub releases | `fetch-prebuilt` feature (not enabled; `fetch_unroll` not vendored) |
| `libgit2-sys` | `git submodule update --init libgit2` when `libgit2/src` is absent | inert — sources are vendored |

The `ext-php-rs` one is the real decision: it is a build-time download of an
unverified third-party archive that gets linked into the output. It is off on
macOS/Linux and behind an optional feature, but if PHP bindings are ever built
on Windows CI, that path runs. Also note `ureq`'s `Config::default()` calls
`Proxy::try_from_env()`, so `HTTPS_PROXY` in the build environment silently
redirects that download.

### 3. `built` bakes this repository's identity into a shipped binary

`turso_core` uses `built` 0.7.7 in its build script. `built` calls
`git2::Repository::discover(CARGO_MANIFEST_DIR)`, which walks **upward** — for a
vendored crate that finds the *enclosing* azul checkout. Five facts about it
become `pub static`s inside the compiled `turso_core`:

- `GIT_HEAD_REF` — the full branch ref, e.g. `refs/heads/transient-window`
- `GIT_COMMIT_HASH`, `GIT_COMMIT_HASH_SHORT`, `GIT_VERSION`, `GIT_DIRTY`

It also stamps a wall-clock timestamp, so this crate alone makes azul's builds
non-reproducible, and it calls `env::vars_os()` over the whole environment
(verified: only a fixed allowlist is emitted — no arbitrary value reaches the
generated file, but the whole environment is in hand should that ever change).

**Recommended**: drop `built`'s `git2` and `chrono` features on the `turso_core`
dependency unless the git stamp is wanted in shipped binaries.

### 4. Whole-environment enumeration

Four crates call `env::vars()`/`vars_os()` in build-time code. All were read:

- `openssl-sys` — name-only, filtered to `DEP_AWS_LC_*`, values discarded; dead
  unless the `aws-lc` features are on. **But** its `env_inner` helper *echoes the
  name and value* of every variable it queries to build-script stdout, which cargo
  persists to `target/<profile>/build/openssl-sys-*/output`. It also does
  `env::set_var("PKG_CONFIG_ALLOW_CROSS", "1")`, mutating the environment of
  every sibling build script.
- `built` — see above.
- `libm` — names only, filtered to `CARGO_FEATURE_*`.
- `wasi` — generated bindings.

`tikv-jemalloc-sys` reads `CI` (to decide whether to dump `config.log` on
failure) — the only CI-provider detection in the tree, and benign.

### 5. Writes outside `OUT_DIR`

All feature-gated and inert as configured, but worth a lint if the policy is
"OUT_DIR only": `io-uring` (`bindgen`+`overwrite`), `ring`
(`RING_PREGENERATE_ASM`), `mvt-reader` (`protoc-generated`), `pulldown-cmark`
(`gen-tests`), `vk-mem` (`generate_bindings`).

### 6. `ICU4X_DATA_DIR` redirects a compile-time `include!`

Eleven `icu_*_data` crates set `cfg(icu4x_custom_data)` from the presence of
`ICU4X_DATA_DIR`; their `src/lib.rs` then does
`include!(concat!(env!("ICU4X_DATA_DIR"), "/mod.rs"))`. Anyone who can set that
variable in the build environment compiles arbitrary Rust into azul. It should
be pinned unset in CI.

### 7. Unreviewable prebuilt binaries

Shipped in-crate and linked without rebuild. All verified structurally (import
tables, no oversized code members, clean `strings` sweeps) and all checksum-clean,
but none is auditable by reading:

- `winapi-x86_64-pc-windows-gnu` — 1,416 `.a` files, 53 MB
- `winapi-i686-pc-windows-gnu` — 1,387 `.a` files, 51 MB
- `windows_{aarch64,x86_64,i686}_{msvc,gnu,gnullvm}` — 3–13 MB import libs each
- `aegis` — `wasm-libs/libaegis*.a` (~320 KB), linked on wasm32 **without a
  source rebuild** (native targets compile from source)
- `wit-bindgen` — 858-byte wasm archive, matches its shipped C
- `hyphenation` — 82 `.bincode` dictionaries (data, not code)
- `libloading` — two ~3 KB PE test fixtures, `cfg(test)` only

### 8. Why `openssl-sys` is in the tree

It is not reachable from `cargo tree` on macOS, but it is in the lockfile via two
paths:

```
openssl-sys <- native-tls <- ureq 3.3.0     <- azul-layout, azul-doc
openssl-sys <- native-tls <- ext-php-rs     <- azul-dll
```

`ureq` pulls `native-tls` by default, which is at odds with `deny.toml`'s stated
reason for using `rustls-rustcrypto` — a pure-Rust provider so `libazul`
cross-compiles to targets where `ring`/`aws-lc-rs` need platform asm. **Worth
checking whether `ureq`'s default features should be off.** It also means the one
crate in the tree that enumerates the entire environment compiles on Linux CI.

## Gap found in the existing justification gate

`dependency-justifications.toml` has 548 justified crates. `Cargo.lock` has 773.
The `dep_tree` job runs `cargo tree` for **azul-css / azul-core / azul-layout /
azul-dll `--features build-dll`** across three OSes. That is *one feature
combination*, and it does not include `azul-doc` at all.

Reproducing it on macOS: the gate sees 415 crates and **all 415 are justified** —
it is doing real work and is genuinely green. But 235 lockfile crates are outside
its view, including `openssl-sys`, `ring`, `native-tls`, `pyo3`, `ext-php-rs`,
`jni`, `tikv-jemalloc-sys`, `mimalloc`, `v4l2-sys-mit`, `zstd-sys`, `alsa`,
`comrak`, `clap` — all of which CI compiles under other feature sets
(`build_pyext`, `php-extension`, `allocator_jemalloc`, azul-doc).

**Recommended follow-up**: widen `dep_tree` to `--workspace --all-features`, or to
the union of the feature sets CI actually builds. That will surface ~235 crates
needing a justification line — real work, but it is the honest scope, and the new
build-script policy already covers the dangerous subset of them.

## Fixed here

**`build_mobile_apps` handed the Apple signing certificate to every build script
in the tree.** The job hoisted `APPLE_CERT_P12` (base64 cert) and
`APPLE_CERT_PASSWORD` to **job-level `env:`**, so the optional cert-import step
could gate on `env.` — `secrets.` is not allowed in a step `if:`. Job env is the
environment of every step, and the build step runs `build-ios.sh` ten times, each
invoking `cargo build`. So a code-signing identity and its password sat in the
environment of ~700 crates' build scripts and proc macros for the whole build.

Reading an environment variable is the single most common payload in a
compromised crate. The `if:` needs a boolean, not a certificate: it now hoists
`HAVE_APPLE_CERT: ${{ secrets.APPLE_CERT_P12 != '' }}` and the material is scoped
to the one step that imports it into the keychain. (The secret is currently empty
in this repo, so nothing has leaked — the exposure was structural.)

## Open follow-ups

1. Widen `dep_tree` to the feature sets CI actually builds (~235 crates to justify).
2. Pin `PYO3_PYTHON`, `NASM`, `LIBCLANG_PATH` to absolute paths in CI; assert
   `ICU4X_DATA_DIR` is unset.
3. Decide on `ext-php-rs`'s Windows devel-pack download (pre-stage, or accept).
4. Check whether `ureq` should be `default-features = false` — it is what pulls
   `native-tls` → `openssl-sys`.
5. Drop `built`'s `git2`/`chrono` features on `turso_core` if branch names and a
   wall-clock timestamp are not wanted in shipped binaries.
6. Enable **Trusted Publishing** on crates.io for azul-css/core/layout — it
   replaces the long-lived `CARGO_REGISTRY_TOKEN` with short-lived OIDC and kills
   the stolen-token vector outright. Same for PyPI.
7. `cargo vet`: 693 exemptions remain. `cargo vet suggest` shows which are worth
   auditing next.
