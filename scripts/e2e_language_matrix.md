# `e2e_language_matrix.sh` — per-language toolchain + recipe reference

`scripts/e2e_language_matrix.sh` runs the AZ_E2E "hello-world click → counter"
test across every shipped language binding and prints an honest status board
(`✓ WORKS` / `✗ FAILS` / `⊘ SKIP`). This file documents what each language
needs so CI installs can be wired up.

## Prerequisites (the script does NOT build these)

```sh
cargo build --release -p azul-dll --features build-dll   # target/release/libazul.{dylib,so}
cargo run -r -p azul-doc codegen all                     # target/codegen/<lang files>
```

The script auto-detects the OS and exports `AZ_LIB`, `AZ_LIB_DIR`, and the
loader path (`DYLD_LIBRARY_PATH` on macOS, `LD_LIBRARY_PATH` on Linux) pointing
at `target/release`. It also copies `libazul` into each FFI example dir, since
every README says "libazul in the working directory" and macOS SIP strips
`DYLD_*` from hardened interpreters (so a bare-named `dlopen` can't find it via
env alone — the on-disk copy or an absolute `AZ_LIB_DIR` path is required).

## Usage

```sh
bash scripts/e2e_language_matrix.sh                 # all 26
bash scripts/e2e_language_matrix.sh "c rust lua"    # subset (space or comma list)
bash scripts/e2e_language_matrix.sh --strict        # exit 1 if any ✗ FAILS
```

Exit code is always 0 (status report) unless `--strict` and ≥1 `FAILS`. `SKIP`
(missing toolchain) never trips `--strict`. Per-language combined logs land in
`$TMPDIR/azul-e2e-matrix.XXXX/<lang>.log`.

## "Working" detection

The headless runner prints cargo-test output ending in `test result: ok. 1
passed; 0 failed; …`. The script strips ANSI escapes (libazul colorizes that
line unless `NO_COLOR` is set — the script sets it) and requires both
`test result: ok` and `0 failed` → `WORKS`. Anything else with a built/run
toolchain → `FAILS` (with a sniffed 1-line reason). Missing toolchain → `SKIP`.

## Per-language toolchain matrix

| lang | toolchain probe | CI installer | codegen artifact | example entry | notes |
|------|-----------------|--------------|------------------|---------------|-------|
| c | `clang` | apt `clang` / macOS preinstalled | `azul.h` | `hello-world.c` | verified-green recipe from `rust.yml` e2e_native. |
| rust | `cargo` | `dtolnay/rust-toolchain` | `azul.rs` (via crate) | `examples/rust` crate | `cargo build -p azul-examples --example hello-world`. |
| cpp | `clang++` | apt `clang` | `azul20.hpp`,`azul.h` | `cpp20/hello-world.cpp` | C++20; same link line as C. |
| csharp | `dotnet` | `actions/setup-dotnet` | `Azul.cs` | `hello-world.cs` + `Hello.csproj` | `dotnet run -c Release`; net10.0 target. |
| java | `mvn` (+ JDK 17) | `actions/setup-java` (maven) | `java/*.java` (dir!) | `HelloWorld.java` + `pom.xml` | copy `java/*.java` → `examples/java/com/azul/`; macOS needs `-XstartOnFirstThread`. JNA 5.14.0. |
| kotlin | `kotlinc` (+ JDK) or `gradle` | `fwilhe2/setup-kotlin` or sdkman | `kotlin/Azul.kt` | `HelloWorld.kt` | script prefers `kotlinc` (needs JNA jar in `~/.m2`); falls back to `gradle run`. |
| scala | `scalac` (+ JDK + JNA) | `coursier/setup-action` | rides on Java's `com.azul.*` | `HelloWorld.scala` + `build.sh` | needs `examples/java/target/classes` built first (script builds it if absent). |
| zig | `zig` | `mlugg/setup-zig` | `azul.zig`,`azul.h` | `hello-world.zig` | `zig build-exe … -lazul`; macOS adds frameworks. |
| ocaml | `dune` + `ocaml` | `ocaml/setup-ocaml` + opam `ctypes ctypes-foreign` | `azul.ml`,`azul.mli`,`dune` | `hello_world.ml` | `dune exec ./hello_world.exe`. |
| haskell | `cabal` + `ghc` | `haskell-actions/setup` | `haskell/` (cabal pkg + cbits) | `HelloWorld.hs` | `cabal run hello-world`. README notes macOS webrender block. |
| lua | `luajit` | apt/brew `luajit` | `azul.lua` | `hello-world.lua` | vanilla lua has no ffi; needs LuaJIT. |
| node | `node` + `koffi` | `actions/setup-node` + `npm i koffi` | `node/azul.js` | `hello-world.js` | macOS: koffi `load('azul')` bare-name + SIP-stripped DYLD → FAILS; Linux honors `LD_LIBRARY_PATH`. |
| ruby | `ruby` + `ffi` gem | `ruby/setup-ruby` + `gem install ffi` | `azul.rb` | `hello-world.rb` | `azul.rb` resolves lib via `AZ_LIB_DIR`/own dir (script copies lib in). |
| php | `php` (+ ext-ffi) | `shivammathur/setup-php` | `Azul.php` | `hello-world.php` | php-ffi path is POD-only (no callbacks) → counter E2E FAILS; full path needs the `php-extension` cargo build. |
| perl | `perl` + `FFI::Platypus` | `shogo82148/actions-setup-perl` + cpanm | `Azul.pm` | `hello-world.pl` | README: invoker drops callback out_ptr → smoke-only, FAILS counter. |
| fortran | `gfortran` | apt `gfortran` / brew `gcc` | `azul.f90`,`Makefile.fortran` | `hello_world.f90` + `Makefile` | smoke-only (tagged-union codegen gap) → no counter E2E, FAILS. |
| cobol | `cobc` | apt `gnucobol` / brew `gnu-cobol` | `azul.cpy` | `hello-world.cob` | smoke-tier (needs hand-written ENTRY paragraphs) → FAILS counter. |
| pascal | `fpc` | apt `fp-compiler` / brew `fpc` | `azul.pas` | `hello-world.pas` (+ `.lpi`) | README: `AzApp_run` access-violation on macOS (libazul-side) → FAILS. |
| lisp | `sbcl` + Quicklisp | apt/brew `sbcl` + bootstrap quicklisp | `azul.lisp`,`azul.asd` | `hello-world.lisp` | README: macOS NSApp threading block. SKIP if quicklisp absent. |
| ada | `gprbuild`/`gnatmake` | GNAT-FSF via `alire` / apt `gnat` (Linux) | `azul.ads`,`azul.adb`,`azul.gpr` | `hello_world.adb` + `.gpr` | README: not brew-installable on macOS-aarch64 → SKIP there. |
| algol68 | `a68g` | build from source (niche) | `azul.a68` | `hello-world.a68` | README: a68g rejects codegen FFI syntax (dialect) → FAILS even if installed. |
| freebasic | `fbc` | x86_64 Linux/Win only (no macOS-aarch64) | `azul.bi` | `hello-world.bas` | SKIP on macOS-aarch64. |
| smalltalk | `gst` | apt/brew `gnu-smalltalk` | `Azul.st` | `HelloWorld.st` | gst runs smoke only; Pharo Tonel blocker → FAILS counter. |
| powershell | `pwsh` | preinstalled on windows runners | `Azul.cs`/`.psd1`/`.psm1` | `hello-world.ps1` | Windows-only; macOS pwsh CFRunLoop blocks NSApp.run → SKIP on non-Windows. |
| vb6 | (VB6 IDE) | none | `.cls`/`.vbp` | `HelloWorld.vbp` | 32-bit Windows legacy; always SKIP on Linux/macOS, no CI toolchain. |
| python | (python-extension) | n/a | n/a | (GUI examples only) | NOT a counter-E2E binding: no AZ_E2E example; `python-extension` is a separate cargo build → always SKIP. |

## CI wiring suggestion

The existing `e2e_native` job in `.github/workflows/rust.yml` already installs
Rust + clang and runs C/Rust/Lua/Node/Ruby/C#/Java/Kotlin (each
`continue-on-error: true`). To drive this matrix in CI instead, install the
toolchains for the languages you want green (table above), then run:

```sh
bash scripts/e2e_language_matrix.sh    # appends the board to $GITHUB_STEP_SUMMARY
```

Languages whose toolchain you don't install show `⊘ SKIP` (clean, not a red X).
Promote a language to a hard gate by adding `--strict` once it is reliably
`✓ WORKS` on the target OS — but note several bindings are documented
smoke-only / libazul-side-blocked and will legitimately report `✗ FAILS` for
the counter scenario (cobol, fortran, perl, php, pascal, lisp, smalltalk,
algol68), and `node` is macOS-only-FAILS / Linux-WORKS.

## Observed baseline (macOS-aarch64 dev host, 2026-05-27)

Toolchains installed: clang/cargo/dotnet/zig/gfortran/fpc/cobc/sbcl/ghc/cabal/
ocaml/dune/php/perl/pwsh/kotlinc/scalac/gst/a68g/luajit/node/ruby/python3/
mvn + a Homebrew JDK (no `gprbuild`/`fbc`/`gradle`/`pharo`).

| lang | observed | cause |
|------|----------|-------|
| c, rust, csharp, lua, ruby | ✓ WORKS | genuine pass |
| ocaml | ✓ WORKS after recipe fix | (kept example's own `dune`; copy only `azul.ml/.mli`) |
| java, scala, kotlin | host-dependent | needed JDK discovery (script now auto-finds it) + Maven build-helper source root (no `com/azul` copy → avoids "duplicate class"). |
| node | ✗ FAILS (macOS only) | `azul.js` calls `koffi.load('azul')` bare-name; macOS SIP strips DYLD_* from hardened `node` → lib not found. WORKS on Linux. |
| zig | ✗ FAILS | **real binding drift**: `hello-world.zig` callback type is `?*const fn(AzRefAny, AzCallbackInfo) callconv(.c) c_uint` but the generated header now expects `AzCallback` (struct). Example/codegen mismatch, not a script issue. |
| pascal | ✗ FAILS (crash) | README-documented libazul-side `AzApp_run` access violation on macOS. |
| fortran, cobol | ✗ FAILS | smoke-only examples (no counter); fortran also needs `azul.mod` build to succeed. |
| haskell | ✗ FAILS | generated `azul` library package + hardcoded `extra-lib-dirs` in `cabal.project` (points at a foreign checkout); recipe now stages the pkg in the sibling `examples/azul-haskell/` and overrides `--extra-lib-dirs`, but full GUI is README-blocked on libazul macOS webrender. |
| perl, php, lisp, smalltalk, algol68 | ✗ FAILS | smoke-only / dialect-incompat / NSApp-blocked per each README. algol68 also OOMs `a68g` on the large generated binding. |
| ada, freebasic, powershell, vb6, python | ⊘ SKIP | toolchain absent (ada/freebasic), Windows-only (powershell/vb6), or no counter example (python). |

The takeaways for "which bindings actually work": **C, Rust, C#, Lua, Ruby,
OCaml** drive the counter E2E to `test result: ok` on macOS. **Java/Kotlin/
Scala** work once a JDK is on PATH (CI's setup-java handles this). **Node**
works on Linux but not macOS (SIP). **Zig** has a real callback-type drift in
its example/header that needs a codegen or example fix. The rest are
documented smoke-only / platform-blocked.
