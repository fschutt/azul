# Autonomous loop tick — SUPER_PLAN_2

The cron loop fires this prompt every ~30 minutes. Each tick should land **one small forward step** on SUPER_PLAN_2 and stop. Do not try to ship a whole sub-feature in one tick — the loop will continue.

## Step 0 — orient (one minute)

Working directory: `/Users/fschutt/Development/azul-mobile`. Branch: `mobile-ios-android`. Worktree-only; never touch `/Users/fschutt/Development/azul`.

Read in this order:
1. `scripts/MOBILE_SESSION_LOG.md` — tail it (last ~80 lines) to see what just landed.
2. `SUPER_PLAN_2.md` §4 — priority list. Find the lowest-numbered P-tier task that's not yet checked in.
3. The relevant `scripts/research/XX_*.md` section for the task you're about to touch.

## Step 1 — verify baseline GREEN

```
bash scripts/mobile-check-all.sh
```

If RED: revert the last commit (`git reset --hard HEAD~1` after confirming nothing else is pending) **and exit** — leave a `### Tick — gate RED, reverted` entry in MOBILE_SESSION_LOG.md explaining what broke. Do NOT pile new work on a red gate.

If GREEN: proceed.

## Step 2 — pick a step

Lowest-numbered open task wins. Current open list (update this as you close items):

- **P1.2 — PermissionManager core** (`layout/src/managers/permission.rs`). Pure-Rust state machine. No platform deps. Test: `cargo test -p azul-layout permission_diff`.
- **P1.2 — Platform stubs** at `dll/src/desktop/extra/permission/{ios,android,macos,linux,windows}.rs`. Initial: each returns `PermissionState::NotDetermined` for every capability.
- **P1.2 — permission-diff pass** in `layout/src/window.rs` end of every layout pass. Emits `PermissionDiffEvent::{Subscribe,Release,Reconfigure}`.
- **P1.3 — iOS file picker** (`dll/src/desktop/extra/file_picker/ios.rs`) via `UIDocumentPickerViewController`. Async via `FilePickerHandle::poll`.
- **P1.3 — Android file picker** (`dll/src/desktop/extra/file_picker/android.rs`) via `ACTION_OPEN_DOCUMENT` + JNI bridge. Same `FilePickerHandle::poll` shape.
- **P1.1 — `examples/mobile-font-probe`** (optional — only useful once iOS sim or Android emulator is in the loop; defer if you don't have the sim).
- **P2.1 — `PenState` mobile fields**: populate `is_eraser`, `barrel_button_pressed` on iOS (touch-began payload from `UITouch.type == .stylus` + `auxiliaryButtonState`) + Android (`MotionEvent.getToolType(...) == TOOL_TYPE_ERASER`).

After P1 closes, switch to P2 (AzulPaint goal app). After P2, P3 (AzulMaps). Etc.

## Step 3 — make the smallest forward diff

Hard rules carried from `NEXT_SESSION_PROMPT.md`:

- **No new deps in `azul-css`, `azul-core`, `azul-layout`** (SUPER_PLAN_2 §0.5). Every platform integration lives in `dll/src/desktop/extra/<feature>/`. Exception: `PermissionManager` is pure-rust state + lives in `layout/src/managers/permission.rs`.
- **Goal-app guard rail:** P2 = AzulPaint, P3 = AzulMaps, P4 = AzulVault, P5 = AzulDoc. If a sub-feature doesn't unblock the current tier's goal app, defer it.
- **User-owned crates** `rust-fontconfig` (`/Users/fschutt/Development/rust-fontconfig/`) and `printpdf` may be patched directly. No upstream PRs.
- **PDF render** = `printpdf::page_to_svg()` → `azul_layout::svg`. No `pdfium-render` / `mupdf-rs`.
- **iOS** is `cargo check`-only until Xcode SDK installs (`xcrun --sdk iphonesimulator --show-sdk-path` must succeed). Android `cargo build` already produces `libazul.so`.

Prefer extending existing files over adding new modules. Comments only where the *why* is non-obvious. No emoji.

## Step 4 — verify GREEN

```
bash scripts/mobile-check-all.sh
```

If it goes RED: revert your edit with `git checkout -- <file>` (or `git restore`), think about what you missed, try a smaller diff. Do NOT commit a red gate.

If GREEN: continue.

## Step 5 — commit + log

```
git add <only the files you touched>
git commit -m "$(cat <<'EOF'
mobile: P<N>.<x> — <one-line summary>

<2-4 lines of context: what you did + the gate result>
EOF
)"
```

Then append a `### Tick — <one-line summary>` entry to `scripts/MOBILE_SESSION_LOG.md`. Mirror the prose style of recent entries (the file shows the pattern). Keep each tick entry under ~10 lines.

## Step 6 — exit

Stop. The cron will fire again in ~30 minutes. Do not start a second step in the same tick.

## When to stop the loop yourself

Tell the user (via a normal message — the user is awake to see them):

- If two consecutive ticks have failed at Step 1 (baseline RED) and you reverted both times → something architectural is wrong; flag it and stop.
- If `git log --oneline ${branch_start}..` shows you've committed 10+ ticks but `mobile-check-all.sh` only ever runs `cargo check` and the user hasn't installed Xcode yet → schedule a final tick that summarizes outstanding follow-ups, then `CronDelete` yourself.
- If a hard rule from §3 would have to be broken to make progress (e.g. you need a new dep in `azul-core`, or a PDF rasteriser that isn't printpdf), STOP and ask the user.

When in doubt about scope or risk: stop, write a short note in MOBILE_SESSION_LOG.md, and let the next tick or the user decide.
