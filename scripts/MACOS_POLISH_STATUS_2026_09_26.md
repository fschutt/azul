# macOS polish + cross-platform engine bugs on PR #476 - status (2026-09-26)

Branch `fix/input-bugs-2026-09-19` (PR #476), started at 97f211f96. Machine:
macOS (M2, 8 GB). Everything release-only; `target/debug` was deleted
(rust-analyzer's background `cargo check --workspace` re-creates it).

**Another session is editing this checkout concurrently** (issue #478,
page breaks): `layout/src/solver3/page_breaks.rs`, `layout/tests/all.rs`
(one line), `layout/tests/a_padded_table_cell_stays_in_its_row.rs`. Never
stage those; `all.rs` is committed by writing HEAD + own lines into the index
(`git hash-object -w` + `git update-index --cacheinfo`).

## Committed here (each fix after its RED test, all seen RED then GREEN)

| commit | what |
|---|---|
| 6c1b36e74 / 9d5031778 | the "body container scroll": `get_overflow_x/y` return the COMPUTED value (CSS Overflow 3 §3.1). `overflow-y: auto` went to taffy as horizontally VISIBLE, so a 733px Pagination inside AzWidgets' page scroller gave the VIEWPORT 177px of sideways travel. RED: 260 vs 0. 3 lib tests that pinned the specified behaviour now state the computed one. |
| f701cf0c8 / f9c1ed75c | the canvas background covers the whole window, not the safe-area-inset viewport (the dark rects in the lower corners). `LayoutContext::canvas_rect`, `LayoutWindow::canvas_rect_for`, new `layout_document` parameter. RED: (0,0,1280,722) vs 1280x800. |
| 09828380c / 069dc8806 | macOS: a `NoTitle`/`NoTitleAutoInject` window (FullSizeContentView) is not inset by its own 28pt titlebar band (`layout_safe_area_insets`); the notch in fullscreen still insets. RED: (28,0,0,0) vs 0. |
| b93ce00f7 / 3afe7358f | Pagination buttons: `box-sizing: border-box` so `min-width: 36px` is the whole button (was 61px, row 733px). |

Batteries run after the overflow change: azul-layout `--test all` 1135 green;
`--lib` 7767 green + the 3 updated tests green (one unrelated failure was the
other session's in-flight page_breaks test).

## Worktree agents (no compilation; integrate + compile ONCE at the end)

| branch | worktree | status | report |
|---|---|---|---|
| `wt/viewport-scrollbar` | agent-ad7cf34be0a4bfeec | done, 2 commits | scripts/VIEWPORT_SCROLLBAR_2026_09_26.md |
| `wt/animation-pacing` | agent-ab0ba1d8799a9fee8 | done, 8 commits | scripts/TOGGLE_ANIMATION_PACING_2026_09_26.md |
| `wt/x11-on-macos` | (agent a102b231d7eccb479) | running | XQuartz at /opt/X11, no xkbcommon |
| `wt/system-colours` | (agent a2050d36f3988c5d7) | running | dark mode / system:* colours / SystemStyle fields; also `margin: 0` on the demo body |
| `wt/selection-bugs` | (agent a0d97ff9a56c803a7) | running | live bugs #1,#2,#3,#4,#6,#8 of scripts/SELECTION_ARCHITECTURE_REVIEW_2026_09_26.md |
| (queued) selection newtypes | - | after `wt/selection-bugs` | TextBlock / EditHost / TextTarget choke point (review §4) |

Integration recipe: for each branch, `git cherry-pick` its commits in order
onto `fix/input-bugs-2026-09-19` (or check out the RED commit, run the named
test, see the predicted failure, then the fix, see GREEN). Then
`target/release/azul-doc codegen all` if api.json changed (run the autofix
first for new public fields), `cargo build --release -j 6 -p azul-dll
--features build-dll`, cp+mv the dylib into target/azul-lib, rebuild
AzWidgets with `AZ_LINK_PATH=$PWD/target/azul-lib`, run the batteries
(`cargo test --release -j 6 -p azul-layout --lib`, `--test all`,
`-p azul-dll --lib --features build-dll`).

## macOS tooling

- `scripts/cgevent_input.py` - real input via CGEventPost in the
  ydotool/xdotool vocabulary (+ `activate PID`, `windows PID`, `shot FILE PID`,
  `dblclick`, `pixwheel`). The terminal has Post-Event + Screen-Recording access.
- AzWidgets run: `DYLD_LIBRARY_PATH=$PWD/target/azul-lib ./target/release/AzWidgets`;
  AZ_E2E works in the shipped dylib (`e2e-scripting` is in build-dll).
- A NoTitle window's CGWindowList bounds = its content area (full-size content view).

## Open, not yet assigned

- Root content does not move when the viewport scrolls (no scroll frame for
  the root) - see VIEWPORT_SCROLLBAR report gap 1.
- Classic vertical bars overshoot into the bottom button (gap 2).
- Toast sits at the window's bottom-right over the page (abspos against the
  ICB) - CSS-correct for the widget as written; decide if the demo wants it
  inside its card.
- Live macOS self-test of typing, selection drag, clipboard, context menu,
  dropdowns, Esc/Tab focus not done yet this session.
