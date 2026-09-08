# Site quality pass — handoff (2026-09-08)

State of PR **#470** (`site/quality-pass` → `master`; #469 is merged into master
and merged into this branch) and of the work that was in flight when the
session hit its limit. Read this before resuming; the subagents' transcripts
are gone, their worktrees and branches survive.

## Done and in the PR (verified locally, see the PR description)

- `/ui` texts (hero, tagline, meta description, seven tile texts), screenshot
  under the heading, favicon everywhere, Copy-button fixes (landing + docs).
- Install routes: `installation.languages.<lang>.install[]` — one entry per
  documented route, one dropdown; system routes first (apt / Homebrew /
  Chocolatey / Scoop), manual download last; dnf/pacman/apk for C/C++ with
  `container`; `scripts/run_install_steps.py` runs every route for
  `--platform` or one with `--variant`; post-release `distros` job + Scoop
  step (written, not yet exercised by a release).
- Rust registry config is one pasteable line per platform.
- Examples: no comments; the counter label is one `create_p_with_text`;
  e2e asserts `body > p`; C hello-world without JSON hooks (host moved to
  `tests/e2e/counter_host.c`); filler tokens fail every deploy.
- Bindings: C++ `namespace azul::ffi` (+ snake_case variant constructors,
  `copy_from_ptr` raw pointer); OCaml enum constants; Haskell idiomatic layer
  on the C-compiler layout oracle (23-line hello-world); Deno callback
  pointer fix (`callback.pointer`).
- Kotlin: `rs.azul:azul-kotlin` built by the maven job (kotlinc, own natives;
  it cannot share a classpath with the Java jar — same `com.azul` class
  names), published by `build_registry_mirrors.sh`, checked by
  `verify_install_commands.sh`; `examples/kotlin/pom.xml` ships as
  `pom-kotlin.xml`; routes "Maven (Linux)" / "Maven (macOS)". Proven locally
  against a file:// mirror (counter e2e passed).
- Tooling: debug deploys read templates from disk; azul-doc builds at -O1
  incrementally (45 s full, ~10 s edit); `--no-screenshots` is NOT a deploy
  flag (every deploy regenerates the 27 guide PNGs — `git checkout --
  doc/guide/en/screenshots/` afterwards).

## Parked subagent work (recovered notes)

### Fortran (worktree `agent-af41a6602068d11a0`, branch `worktree-agent-af41a6602068d11a0`)

Carried into this branch: the one-line tag-width fix in
`doc/src/codegen/v2/lang_fortran/layout.rs::mono_layout` (`TaggedUnion` uses
`tag_layout(repr.as_deref())` like `lang_c`, instead of a hard-coded 4-byte
tag; the 13 `CssPropertyValue<…>` blobs were 8 bytes where the C header says
5). Pinning unit test still to write.

Design for the usable layer (nothing implemented), settled with gfortran
16.2 probes (`brew install gcc`; CI's lane is ubuntu-22.04 gfortran 11, so
stay within F2003/F2008 core):

- gfortran finalizes function results after assignment → no `final ::` on
  wrapper types returned by factories (use-after-free); explicit `delete`
  type-bound procedure instead.
- `class(*), pointer` handle store with sourced allocation + `select type`
  works and mutations persist; `c_int == kind(0)`; a module may re-export
  `iso_c_binding` names; a `procedure(iface), pointer, nopass` table
  component called from a `bind(C)` invoker with a borrowed wrapper works.
- Old wrapper layer was unusable by construction: `Dom_create_body` etc.
  never `public`, methods took raw `type(AzString)`.
- Wrapper types `<snake>_t` (`dom_t`, `button_t`, `app_t`, `ref_any_t`;
  Fortran folds case so `type(Button) :: button` is illegal); public
  factories `dom_create_p_with_text('5')`; `String` args as
  `character(len=*)`, returns `character(len=:), allocatable`; unit enums as
  `integer` + un-prefixed constants (`ButtonType_Primary`,
  `Update_RefreshDom`); `bool` as `logical`; consuming-self builders as
  in-place subroutines (`call label%with_css('font-size: 32px;')`), other
  consumers mark `self%owned = .false.`; borrowed wrappers passed to
  consuming params are cloned; per-kind typed abstract interfaces from
  `CallbackTypedefDef`; one handle table (`class(*), pointer` object + one
  procedure-pointer component per `HOST_INVOKER_KINDS` kind); lazy
  invoker/releaser install on first handle; `ref_any_create(t_model(5))` /
  `data%get()`; `window_create_options_create(layout)`; the example becomes
  ~55 lines with no `iso_c_binding` / `c_f_pointer` / `host_invoker_init`.
- Also to update: `examples/fortran/hello_world.f90`, the fortran guide and
  README, `tests/memtest/mem_test.f90` (old names; not in CI). No api.json
  route change needed (`make` + `./hello_world`).

### JavaScript, Deno / Bun — DONE (merged into this branch)

The binding runs under Node, Bun and Deno; Bun and Deno are documented
routes, verified verbatim with `scripts/run_install_steps.py` against a
local release mirror. Three defects, one per layer: the Deno adapter
collapsed every non-primitive to `'pointer'` (now a by-value type layer
with real struct descriptors); struct-returning callbacks wrote their
result with `koffi.encode` behind a node-only gate (now the adapter's
`encodeInto`, implemented by all three runtimes); and the example asked
for `require('azul')` first, which made Bun auto-install an unrelated npm
package instead of falling back to `./azul.js`.

Original analysis, kept for context:
- **Bun**: `require('azul')` in `examples/node/hello-world.js` made Bun
  auto-install an unrelated `azul` package from npm (no `node_modules`
  present), so the `./azul.js` fallback never ran — hence
  `WindowCreateOptions` undefined. Fix: the example requires `./azul.js`
  directly (or prefers it); koffi 3.2.1 works under Bun through Node-API,
  a simpler path than `bun:ffi`.
- **Deno**: needs `package.json` with `"type": "commonjs"` next to a `.js`
  that uses `require`; after the pointer fix, the SIGBUS is the adapter's
  by-value marshalling (`toDenoType` collapses every non-primitive to
  `'pointer'`). A probe showed nested structs, sret returns and subarray
  pointers all work under `Deno.dlopen` with proper struct descriptors, so
  the plan is a full by-value type layer in the Deno adapter.
- Routes to add once each runtime passes the counter e2e: Bun (`bun add`
  the tarball or curl azul.js + library), Deno (curl azul.js + library +
  package.json, `deno run --allow-ffi --allow-read --allow-env`). Node is
  broken on this Mac (Homebrew node missing libllhttp) — cannot be tested
  here.

### Haskell pure `view` (worktree `agent-a59d00f652ed463cf`) — stopped before any work; the scope is in the PR conversation: derive a pure description surface from api.json (constructors/builders/setters with describable args), `render` replays it once, `<K>Handler` instances accept `model -> Html`, no hand-coded lists.

## Still open (not started)

- Screenshots: `examples/assets/screenshots/*.mac.png` are the January
  stand-ins; regenerate with `scripts/screenshot_single.sh <example>`
  (DLL + `target/codegen/azul.h` in the worktree; `AZ_SCREENSHOT_PROFILE=release`).
- Release notes: `api.json → "0.2.0".notes` still says "not functional yet".
- Hello-world guides for C, C++, Python install from GitHub releases with an
  unsubstituted `${VERSION}`; the other 14 use the azul.rs mirrors.
- Host-invoker ergonomics for Java/Kotlin/Scala/C#/Node: keep the table
  under the hood, emit typed callback interfaces (`Update onClick(M model,
  CallbackInfo info)`) so no `Pointer`/`outPtr`/`refanyGet` appears in user
  code; mechanism cannot vanish for JNA (no struct-by-value callbacks).
- Fortran usable layer (above). Its `wrappers.rs` half is committed on
  branch `worktree-agent-a88922446c0dfa144` as a WIP commit that does NOT
  compile: it calls helpers (`ISO_C_REEXPORTS`, `managed::{reserved_names,
  iface_name, register_name, REF_ANY_GET}`, `should_emit_function` as
  `pub(crate)`) that the managed.rs half was to provide.

## Resuming

```sh
cd /Users/fschutt/Development/azul/.claude/worktrees/site-quality-pass   # branch site/quality-pass
cargo build --release -p azul-doc && ./target/release/azul-doc codegen all
cargo build --release -p azul-dll --features build-dll,debug-server         # for e2e / screenshots
./target/release/azul-doc deploy debug && git checkout -- doc/guide/en/screenshots/
python3 -m http.server -d doc/target/deploy 8000
```
