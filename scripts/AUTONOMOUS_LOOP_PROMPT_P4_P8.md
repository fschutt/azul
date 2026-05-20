# Autonomous loop — SUPER_PLAN_2 P4–P8 (AzulVault → AzulDoc → expansions)

You are continuing the azul-mobile build **fully autonomously**. P1–P3 are done
(see the `### Tick — FINAL` entry in `scripts/MOBILE_SESSION_LOG.md`). Your job is
to drive P4 onward to completion, one small forward step per tick, keeping the
gate GREEN. **All the design decisions below are already made — do NOT stop to
ask. Make reasonable judgment calls and keep moving.**

Working dir: `/Users/fschutt/Development/azul-mobile`. Branch: `mobile-ios-android`.
Never touch `/Users/fschutt/Development/azul`.

---

## Step 0 — orient (1 min)
1. `tail -n 100 scripts/MOBILE_SESSION_LOG.md` — what just landed.
2. `SUPER_PLAN_2.md` §4 — the priority list. Lowest-numbered open item from **P4** wins. (P4 Vault, P5 Doc, P6 expansions are defined; if the user has added P7/P8 to §4, do those after.)
3. The matching `scripts/research/0X_*.md` for the feature you're touching (02 biometric, 07 libsql, 06 pdf, 01 camera/screencap, 03 sensors/gamepad/stylus).

## Step 1 — verify baseline GREEN
```
bash scripts/mobile-check-all.sh
```
If RED on a clean tree: `target/codegen/` may be missing (after a `cargo clean`) —
run `cargo run -q --bin azul-doc -- codegen all` to regenerate it, then re-check.
If RED because the *last commit* broke it: `git reset --hard HEAD~1`, log a
`### Tick — gate RED, reverted` entry, exit. Never pile on a red gate.

## Step 2–5 — do ONE real step, verify, commit, log
Land **one** real implementation per tick (real native call / SQL / wiring — not a
stub-with-TODOs). Cap ~10 files / ~600 added lines; split if larger. Then:
`bash scripts/mobile-check-all.sh` GREEN → commit (only the files you touched) →
append a `### Tick — P<n>.<x> — <summary>` entry to `scripts/MOBILE_SESSION_LOG.md`
(≤10 lines, mirror recent entries) → stop. **Do not HOLD when work remains** — if
one item is genuinely blocked, log why and move to the next independent item.

---

## RESOLVED DESIGN DECISIONS (do not re-litigate)

**Hard product constraints (user):** one `libazul.dylib`; demos use ONLY the
api.json public surface (never call azul-core/layout/dll crates directly).

**1. Working model:** sequential, you alone, incremental ticks. No parallel agents.

**2. ObjC async / completion blocks → objc2.** Biometric auth (`evaluatePolicy:reply:`),
the deferred iOS/macOS *permission requests* (`requestAccessForMediaType:completionHandler:`),
camera, etc. — write these iOS/macOS backends in **objc2** (block2 is first-class
there; the macOS shell already uses it). New backends start objc2-native (don't mix
objc 0.2 + block2). The existing *sync* objc 0.2 probes can stay as-is.

**3. DB (P4.3) = approach A.** Static-link **rusqlite (bundled sqlite)** into the dll
behind a `db-sqlite` Cargo feature. Expose a **SQL-string** `Db` API in api.json:
`Db::open(path) -> Db`, `db.execute(sql, params) -> rows_affected`,
`db.query(sql, params) -> DbRows` (params + rows as typed value arrays — a `DbValue`
enum: Null/Integer/Real/Text/Blob). The engine is fully hidden behind SQL strings, so
the public surface is engine-agnostic and the demo never sees rusqlite. This satisfies
"one dylib" + "api.json surface". (Pure-Rust engine / "no C libsqlite" is a *future
backend swap* behind the same API — don't attempt it now. Wire-protocol / remote libsql
is a *separate* optional `db-libsql-remote` feature for P4.3's remote-sync follow-up,
not local storage.)

**4. AzulVault (P4.4) spec:** an Apple-Password-Manager-style demo at
`examples/azul-vault` — a basic **key/value store for sensitive data** (add / list /
view entries), **biometric-gated** on launch (FaceID/TouchID), persisted to a **local
`db-sqlite` database** via the `Db` API. Compose the biometric gate as a
permission-as-DOM / callback flow; public API only.

**5. Per-feature architecture pattern** (mirror P1.2 permissions / P3.1 geolocation —
they are your templates):
- POD types in `azul-core/src/<feature>.rs` (e.g. `biometric.rs`).
- Stateful + async delivery in `azul-layout/src/managers/<feature>.rs`: a manager
  holding `last_*`, plus a process-global async channel (`push_*` / `drain_*`,
  `std::sync::Mutex<Vec<_>>`, poison-recovering) — copy `geolocation.rs`'s channel
  verbatim. Unit-test the channel (push→drain→apply→read).
- Native backends in `dll/src/desktop/extra/<feature>/{mod,ios,macos,android,linux,windows}.rs`.
  The dll layout pass (`dll/src/desktop/shell2/common/layout.rs`) drains the channel
  into the manager (see steps "7a"/"7c" there for the permission/geolocation precedent).
- Expose to users via api.json (autofix + codegen — see workflow below). The accessor
  pattern is `CallbackInfo::get_<x>()` reading the manager (template:
  `CallbackInfo::get_location_fix`). Engine/DB handles are POD `Az*` types with methods.
- Demo consumes the public `azul::` API only.

---

## CRITICAL WORKFLOWS & GOTCHAS (these caused multi-tick stalls before)

**api.json + codegen (the prescribed workflow — now de-risked):**
- Edit the Rust method/type, then `cargo run -q -p azul-doc -- autofix add Type.method`
  (targeted — adds just that method; NOT bare `autofix`, which dumps all drift).
- `cargo run -q --bin azul-doc -- autofix apply target/autofix/patches`.
- **New `Option<T>` return types** need a 2-pass: after applying the method, re-run bare
  `cargo run -q -p azul-doc -- autofix` — it now reports `OptionT` as a needed *addition*;
  **curate** `target/autofix/patches/` to keep ONLY the `add_OptionT` patch (delete the
  pre-existing-drift patches: `*_remove_MapTileId*`, `*_move_Detected*`, `*_move_Gesture*`),
  then apply. The repr bug is fixed (data-carrying enums now get `repr(C, u8)`), so this
  is a clean single-pass now.
- `cargo run -q --bin azul-doc -- codegen all` regenerates `target/codegen/*` (overwrites;
  not a disk balloon). Then gate. **Revert-on-RED**: `git checkout api.json` + `codegen all`.
- For a new POD type, add `impl_option!(T, OptionT, [Debug,Clone,Copy,PartialEq])` in core
  if you need `Option<T>` across the FFI (it emits `#[repr(C,u8)]`).

**NEVER `include!` a `target/codegen/*` artifact from azul-core/css/layout** — `azul-doc`
builds those crates to *generate* the artifact (build cycle; it broke on `cargo clean`).
Such includes live ONLY in `azul-dll` (downstream of codegen). See
`dll/src/desktop/material_icons.rs` for the pattern.

**`build-dll` is the complete-dylib umbrella.** When you add a new feature-gated
subsystem (`db-sqlite`, `biometric`, `camera`, …), add it to `build-dll` in
`dll/Cargo.toml` AND, if it's active under the gate's feature set, ensure its deps are
pulled (use `dep:foo` for optional deps; converting one ref to `dep:` suppresses the
implicit feature so convert ALL bare refs to that dep).

**Dependency isolation (§0.5):** no new deps in azul-css/core/layout (managers are pure
Rust). Platform/engine deps (objc2, jni, rusqlite, printpdf, …) go in `azul-dll` behind a
Cargo feature, under `dll/src/desktop/extra/<feature>/`.

**Disk hygiene:** if `df -h /Users/fschutt` shows >~92% used, purge ALL incremental dirs
(`rm -rf target/debug/incremental target/*/debug/incremental`) before building. Use
`cargo check` not `cargo test` for the whole dll (test binaries balloon target/). Targeted
`cargo test -p azul-layout --lib managers::<x>::` is fine for manager unit tests.

**Gate / verification:**
- Mobile: `bash scripts/mobile-check-all.sh` (5 targets — iOS is cargo-check-only, no sim).
- macOS backend (not in the mobile gate): `cargo check -p azul-dll --no-default-features --features "std,logging,link-static,a11y"` (host).
- Examples: `cargo check -p azul-vault` (etc.) — leaf, low-disk; mobile gate stays warm.
- "Done" = compiles + correct per platform docs / unit-tested where pure-Rust. iOS/Android
  aren't runtime-tested here; that's expected.

---

## P4–P8 backlog (work in order; SUPER_PLAN_2 §4 is the source of truth)

**P4 — AzulVault (auth):**
- P4.1 biometric: core `BiometricKind{NotAvailable,Fingerprint,Face,Iris}` +
  `BiometricResult{Authenticated,Failed,Cancelled,FellBackToPasscode,Unavailable,Error}`
  → `BiometricManager` + result channel → backends (objc2 LAContext availability +
  evaluatePolicy block; Android `BiometricManager.canAuthenticate` + `BiometricPrompt`) →
  api.json `request_biometric_auth(config)` + `CallbackInfo::get_biometric_result()` +
  sync availability accessor.
- P4.2 keyring: `dll/extra/keyring/` (iOS/macOS Keychain, Android KeyStore, Linux
  libsecret, Windows CredentialLocker) — biometry-bound secret storage.
- P4.3 DB: `db-sqlite` feature + `Db` SQL-string API (approach A above). Remote sync =
  `db-libsql-remote` follow-up.
- P4.4 `examples/azul-vault` (spec above).

**P5 — AzulDoc (documents):** P5.1 PDF export via `printpdf` (`App::export_pdf`); P5.2 PDF
render via `printpdf::page_to_svg()` → `azul_layout::svg` (NO pdfium); P5.3 watch the
printpdf↔azul-layout dep cycle (path override); P5.4 `examples/azul-doc`.

**P6 — horizontal expansions (no single goal app):** camera, screen-share, sensors
(accel/gyro/mag), gamepad, wacom pad. Each: `dll/extra/<feature>/` + the per-feature pattern.

**P7/P8:** only if the user has added them to SUPER_PLAN_2 §4.

---

## When to stop the loop yourself
- If two consecutive ticks fail Step 1 (baseline RED) and you reverted both → something
  architectural broke; log it and stop.
- If a **hard rule** (§0.5 dep isolation, one-dylib, api.json-only) would have to be broken
  to proceed → log and stop.
- If the genuinely-next item needs a *new* product decision not covered above → log it,
  **skip to the next independent item**, and keep going. Don't HOLD repeatedly.
- When all of P4–P6 (and any P7/P8) are complete + demos build → write a final summary tick.
