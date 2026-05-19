# Next-session bootstrap prompt

Paste this into a fresh Claude Code session at `/Users/fschutt/Development/azul-mobile` to continue from where we left off.

---

You are picking up an in-progress mobile expansion of the **azul** Rust UI framework. The previous session shipped iOS + Android backends + the unified gesture/event surface; this session implements **Priority 1 of SUPER_PLAN_2** (rust-fontconfig mobile arms + PermissionManager + mobile file pickers) and prepares the runway for Priority 2 (AzulPaint goal app).

## Required reading before you touch anything

In this exact order:

1. `SUPER_PLAN_2.md` — the master plan for this session. §0 (crates we own), §0.5 (dll-submodule rule), §0.6 (PDF via SVG), §0.7 (goal apps), §1.5 (permission-aware DOM), §4 (priority order with goal apps) are mandatory.
2. `scripts/MOBILE_SESSION_LOG.md` — previous session's tick-by-tick log; orients you on what landed.
3. `scripts/research/05_assets_fonts_perms.md` — drives P1.1 (rust-fontconfig fix). Load-bearing finding: **mobile builds today find zero system fonts.**
4. `scripts/research/08_permission_dom_nodes.md` — drives P1.2 (PermissionManager + permission-diff pass).
5. `scripts/research/04_system_integration.md` — drives P1.3 (mobile file pickers § first, IME § referenced later).
6. `SUPER_PLAN.md` — last session's plan; mostly for the architecture seams it documents (Sprint M's `inject_native_*` pattern is the template every new platform integration follows).

After P1 lands, the *next* tier briefs to read are `research/03` (pen/touch → P2), then `research/04` § geolocation + `research/06` MVT (→ P3), etc.

## Hard rules

- **Worktree only:** all work happens in `/Users/fschutt/Development/azul-mobile` on branch `mobile-ios-android`. The original repo at `/Users/fschutt/Development/azul` belongs to a different agent — never touch that working tree. Cargo file-lock contention → wait 60 s and retry.
- **No new deps in `azul-css` / `azul-core` / `azul-layout`** (SUPER_PLAN_2 §0.5). Every new integration goes into `dll/src/desktop/extra/<feature>/`. The only exception is `PermissionManager` itself (pure cross-platform state, no platform deps) which can live in `layout/src/managers/permission.rs`.
- **Goal-app guard rail:** every change must serve the current tier's goal app. P1 has no goal app — it's pure unblock work. P2 = AzulPaint. P3 = AzulMaps. P4 = AzulVault. P5 = AzulDoc. If you're tempted to add a feature outside the goal app's punch list, write a note in `MOBILE_SESSION_LOG.md` and defer it to P6.
- **User owns `rust-fontconfig` and `printpdf`** (SUPER_PLAN_2 §0). You may patch them directly in the user's local override paths (`/Users/fschutt/Development/rust-fontconfig/` for fontconfig; check the workspace for printpdf). No need to send upstream PRs.
- **PDF render path is `printpdf::page_to_svg()` → `azul_layout::svg`** (SUPER_PLAN_2 §0.6). Do NOT pull in `pdfium-render` or `mupdf-rs`. `research/06` recommended pdfium for inline render but the user has explicitly overridden this — printpdf can parse PDF and emit SVG-as-string, and the framework already renders SVG.
- **Mobile gate:** `bash scripts/mobile-check-all.sh` must stay GREEN across all 5 targets (`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`, `aarch64-linux-android`, `x86_64-linux-android`) after every commit. Currently green; don't regress.
- **iOS link is still SDK-gated:** the user is installing Xcode via `xcodes install --latest` separately. Until `xcrun --sdk iphonesimulator --show-sdk-path` succeeds, run `cargo check` (source-only) for iOS targets, not `cargo build`. Android `cargo build` already produces `libazul.so`.

## Concrete starting tasks (Priority 1)

Tackle in this order. Each is 1–3 days. After each, `bash scripts/mobile-check-all.sh` must stay green, and an MOBILE_SESSION_LOG entry must land.

### P1.1 — Fix `rust-fontconfig` for iOS + Android (~3 days)

Reference: `scripts/research/05_assets_fonts_perms.md` §1 + §4.

Files to touch (in `/Users/fschutt/Development/rust-fontconfig/`):
- `src/lib.rs:121` — add `OperatingSystem::IOS` + `OperatingSystem::Android` variants; fix `OperatingSystem::current()` so iOS / Android targets resolve to themselves, not the Linux catch-all.
- `src/lib.rs:1833..1903` — add `#[cfg(target_os = "ios")]` + `#[cfg(target_os = "android")]` arms to `FcFontCache::build()`.
- **iOS arm:** use `core-text` crate's `CTFontManagerCopyAvailableFontURLs` + per-URL `CTFontDescriptor` for family/weight/style.
- **Android arm:** walk `/system/fonts/*.ttf` + `*.otf` + parse `/system/etc/fonts.xml` (a system XML file mapping family names to file paths + weights — Android's fontconfig-equivalent).

Verification:
- Write a tiny `examples/mobile-font-probe/main.rs` that prints `FcFontCache::build().families()` count.
- iOS sim run should show ≥ 200 families; Android emulator ≥ 30.
- `bash scripts/mobile-check-all.sh` GREEN.

### P1.2 — `PermissionManager` + permission-diff pass (~2 days)

Reference: `scripts/research/08_permission_dom_nodes.md` §3 + §6.

Files to create:
- `layout/src/managers/permission.rs` — pure cross-platform state, `PermissionState` enum (with `Granted{quality}` + `EphemeralGranted{until_app_close}`), `Capability` enum, subscribe/release/recheck_all APIs.
- `dll/src/desktop/extra/permission/mod.rs` — platform-specific stubs that the manager calls. iOS / Android / macOS / Linux / Windows submodules.
- `layout/src/window.rs` — call `permission_manager.diff(&styled_dom)` at the end of every layout pass; collect emitted `PermissionDiffEvent`s.

Verification:
- A unit test that constructs a `Dom` with `GeolocationProbe`, runs one diff, asserts `Capability::Geolocation → Subscribe`. Remove the probe, second diff, asserts `→ Release`.
- `cargo test -p azul-layout permission_diff` passes.

### P1.3 — Mobile file pickers (~2 days)

Reference: `scripts/research/04_system_integration.md` §1.

Files to touch:
- `dll/src/desktop/extra/file_picker/{ios,android}.rs` — populate the no-op stubs in `layout/src/desktop/dialogs.rs::FileDialog::open_file_mobile / open_directory / save_file`. iOS: `UIDocumentPickerViewController` sheet; Android: `Intent.ACTION_OPEN_DOCUMENT`.
- Wire async via the `FilePickerHandle::poll` pattern from research/04 — blocking channels deadlock the UI thread.

Verification:
- macOS + Linux + Windows still work (no regression in existing `tfd` flow).
- iOS / Android: `cargo check` GREEN; runtime behavior verified once a device is in the loop.

## Workflow

1. Run `bash scripts/mobile-check-all.sh` first thing to confirm baseline.
2. Pick the lowest-numbered P1 sub-task that's still open. Read the relevant `research/*.md` section. Sketch the change in your head, identify the exact files.
3. Make the smallest possible diff that progresses the sub-task. Verify gate stays GREEN. Commit with `mobile: P1.X — <one-line summary>`. Append an MOBILE_SESSION_LOG entry.
4. When the sub-task is complete, mark it in TaskList and move to the next.
5. Periodically (every 3-4 commits) re-run the FULL gate suite — not just the most recent target — to catch cross-target regressions early.

## Things explicitly out of scope for this session

- Camera, screen sharing, sensors, gamepad, Wacom-pad extensions — these are P6 horizontal expansions, not part of P1.
- The IME / `UITextInput` / Wayland-text-input-v3 work — defer to P2 if AzulPaint's save dialog needs it; otherwise P3 once the map widget needs search input.
- Linux-host iOS cross-compile (SUPER_PLAN Sprint N) — defer until the user explicitly asks.
- The web (wasm32) backend.

## Success criteria for this session

- `rust-fontconfig` returns ≥ 200 families on iOS sim, ≥ 30 on Android emulator.
- `PermissionManager` subscribes / releases via DOM diff. Tested.
- iOS + Android file pickers no longer return `OptionString::None`.
- `bash scripts/mobile-check-all.sh` stays GREEN throughout.
- `MOBILE_SESSION_LOG.md` documents each P1.x landing with the gate result.
- AzulPaint scaffolding (P2.1 — populate existing PenState fields on every backend) is optionally landed if time permits, as a head-start on the next session.

Total P1 effort: ~7 days. AzulPaint goal-app comes online in the *following* session.

Good luck.
