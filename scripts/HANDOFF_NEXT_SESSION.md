# Next-session handoff — azul 0.2.0 release

## Goal
Release azul **0.2.0** (a vertical slice; OK if a bit rough — azul isn't widely announced yet).
Path: finish the dep-release chain → drop the `printpdf` `[patch]` in azul-mobile →
push `mobile-ios-android` → drive CI green → deploy to the website.

**Standing constraints:** NEVER push to master; never force-push; branch `mobile-ios-android`;
always `--release` (disk-limited); commit footer
`Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`;
append `### Tick —` / `### CI-GREEN tick` entries to `scripts/MOBILE_SESSION_LOG.md`.
Do NOT modify `/Users/fschutt/Development/azul` (other agent's repo) — but merging FROM it is now done.

## State (2026-05-23)
- `mobile-ios-android` tip = **2c65d789f**, **131 commits unpushed** (intentional — see THE BLOCKER).
- **Published to crates.io this session:**
  - `allsorts-azul 0.16.4` — 7 direct deps dropped (23→16, Option-B backport of PR #134; bitflags/FeatureMask + pathfinder kept → public API unchanged), TrueType hinting retained, GitHub repo renamed `fschutt/allsorts` → **`fschutt/allsorts-azul`**, README fork banner. Tags `v0.16.3` (dep-drop) / `v0.16.4` (banner).
  - `allsorts_no_std 0.5.3` — metadata `repository`/`homepage` repointed at fschutt + fork README (wezm/YesLogic ask, PR #1; was confusing people onto the upstream tracker, yeslogic#49).
  - `rust-fontconfig 4.3.0`.
- **web/remill backend MERGED** (task #17): azul/`layout-debug-clean`'s **125 commits** rebased linearly onto the mobile tip + 1 merge-fixup commit. Tags **`web-backend-start`** (d47182e69) .. **`web-backend-end`** (2c65d789f) bracket the 126. One real conflict (`layout/src/window.rs`) resolved: kept mobile's deduplicated constructor refactor (`from_font_manager`) + folded in the agent's `skip_gpu_sync`. Re-stripped the profiling `Instant::now()` the M12.x debug commits dragged back into styled_dom.rs. `cargo check -p azul-core` + `-p azul-layout` PASS with allsorts-azul 0.16.4. `third_party/remill` submodule pointer (uninitialized) came in.

## THE BLOCKER (why CI is red, why we don't push)
Root `Cargo.toml` `[patch.crates-io]` line 47: `printpdf = { path = "/Users/fschutt/Development/printpdf" }`.
On CI runners that path is absent → `cargo metadata` fails → EVERY cargo job red
(last run `26333448552` = failure). Nothing goes green until printpdf is **published** to
crates.io and this `[patch]` line is dropped. (libudev-sys = forks/ in-repo and
azul-css/core/layout = css/core/layout in-repo are fine — those paths exist in the repo.)

## Remaining chain (in order) — tasks #18–#22
- **#18 — CI dual-build + remill version-lock.** Pin the remill fork in CI
  (`third_party/remill` → https://github.com/fschutt/remill.git @ `212d3e46`). Update azul-doc
  deploy to build TWO libazul prebuilts: **no-remill (~25 MB, default)** + **with-remill
  (~130 MB, Docker/web app deploys)**. Ensure the `build-dll` feature has everything enabled.
  (Reversible CI-config work — can be done autonomously.)
- **#19 — Release azul-layout to crates.io** (+ azul-css / azul-core at compatible versions).
  printpdf depends on a published azul-layout. **IRREVERSIBLE.** Validate the full dll build first.
- **#20–22 — printpdf** (branch `azul-mobile-parsedfont-compat`): switch its `allsorts` dep to
  `allsorts-azul 0.16.4` + bump azul-layout to the new published version; promote the branch to
  the new printpdf `master` (it's ahead); **full source review with subagents + merge the
  community PR/branch backlog** (NEEDS USER INPUT on which to take); major-version release after
  reviewing all the bug fixes. **IRREVERSIBLE publish.**
- **Final — drop the `[patch]` + push.** Remove the printpdf `[patch]` line, bump dll's
  `printpdf` dep to the published version. Run the preflight: `cargo run -r -p azul-doc autofix`
  (0 drift) → `codegen all` → `cargo check` css/core/layout/dll (CI feature sets) → the exact
  no-Instant gate (0 ungated). Only then push `mobile-ios-android`; let the cron drive the
  remaining failures green.

## Preflight / known follow-ups (surface before the final push)
- **probe.rs:55** `Instant::now()` is gated by `#[cfg(all(feature="probe", not(target_family="wasm")))]`
  on the enclosing `mod imp` (line 23) — ~32 lines above, just outside the gate script's 30-line
  window. Re-verify against the EXACT no-Instant gate; add an explicit cfg if it false-positives.
- **dll feature-matrix NOT yet validated** post-merge (core + layout are). Check
  a11y / no-a11y / minimal / link-dynamic / all_img / svg-xml / layout.
- **link-dynamic codegen bug** (still open): `target/codegen/dll_api_external.rs` calls
  `Az*_*WithCtx` fns (AzButton_setOnClickWithCtx, AzDom_createVirtualViewWithCtx,
  AzMapWidget_domWithFetchWithCtx, AzThread_createWithCtx, AzTreeView_setOnNodeClickWithCtx, …)
  not declared in the link-dynamic `extern` block. The WithCtx wrappers are emitted
  (doc/src/codegen lang_rust.rs:~2990 / lang_c.rs:~938) but the link-dynamic import block omits
  them. Fix where extern decls are generated.
- **Example AzCallback API drift** (async/hello-world/widgets) — `build_binaries` `cargo check --examples`.
- **patch_format.rs** VariantDef missing `ref_kind`.

## Stub-audit backlog (NOT CI-blocking — for the 0.2.0 known-issues list)
iOS/macOS permission-request no-op (permission/ios.rs:28, macos.rs:31); 6 Android features lack
Java glue (AzulBiometric/Keyring/Sensors/Geolocation/Gamepad/Permissions.java missing — only
AzulFilePicker.java exists; AzulActivity.java lacks onRequestPermissionsResult); PDF export
rect-only (pdf/mod.rs:155); libsql remote DB absent (sqlite/mod.rs); Windows geolocation stub
(geolocation/windows.rs:22); video_codec stub all platforms. The desktop sensors/keyring/biometric
backends (tasks #8–#10) are REAL/verified.

## The CI-green cron
A 30-min cron fires the CI-loop prompt (check `gh run list -b mobile-ios-android`, work the
PENDING list, preflight, push, diagnose). It is effectively **paused on the printpdf release
chain** above — pushing can't make CI green until the `[patch]` is dropped. Once printpdf is
published + the patch removed + the preflight is clean, the push + cron-watch phase resumes.
