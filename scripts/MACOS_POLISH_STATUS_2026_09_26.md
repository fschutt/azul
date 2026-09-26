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
| ef11e00b7 / d94748244 | Switch + Slider tracks `align-self: start` (were `center`: centred horizontally in a column - the Linux ledger's L1). RED: 180/100 vs 0. |

Batteries run after the overflow change: azul-layout `--test all` 1135 green;
`--lib` 7767 green + the 3 updated tests green (one unrelated failure was the
other session's in-flight page_breaks test).

## Worktree agents (no compilation; integrate + compile ONCE at the end)

| branch | worktree | status | report |
|---|---|---|---|
| `wt/viewport-scrollbar` | agent-ad7cf34be0a4bfeec | done, 2 commits (rebased onto 028ecfcfd) | scripts/VIEWPORT_SCROLLBAR_2026_09_26.md |
| `wt/viewport-scroll-frame` | (new agent, from 358f07ef1) | running: root scroll frame (content moves with the viewport) + classic-bar overshoot | - |
| `wt/animation-pacing` | agent-ab0ba1d8799a9fee8 | **INTEGRATED** as dac2aa31a..0527d9cae: compiled first try; 5 layout + 5 dll tests GREEN at the tip; with the 6 fixes reverted exactly the 6 predicted REDs (values as predicted), the constraint test stays green | scripts/TOGGLE_ANIMATION_PACING_2026_09_26.md |
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

## Ledger - back of the queue (user, 2026-09-26)

- **Focus does not leave the main window when it moves into the ColorInput's
  picker sub-window**: the ring stays on the main window's colour preview ->
  desync. "There are lots of bugs like this" - architecture first. Analysis
  agent running -> scripts/FOCUS_SUBWINDOW_AND_ARROW_KEYS_ANALYSIS_2026_09_26.md.
- **Arrow keys on the picker's gradient do nothing** (ring on the gradient;
  arrows should move the colour by 1%, Ctrl+arrow by 10%). Same analysis:
  widget arrow semantics vs arrow-key spatial focus navigation.
- E2E `get_selection_state` reports `range.end.cluster_id.start_byte_in_run`
  without the affinity: Cmd+A over "hello world" reads `end: 10` (it is
  cluster 10 Trailing = all 11 chars). Tooling gap, not a selection bug.
- Headless E2E pitfall: every `key_down` needs its `key_up`, or the next
  press of the same key is not a new KeyDown (looked like "Tab is stuck").
- Verified headless on AzWidgets: Tab order TextInput -> NumberInput ->
  TextArea -> ColorInput -> Slider -> Switch -> CheckBox, Shift+Tab back, Esc
  blurs the TextArea.
