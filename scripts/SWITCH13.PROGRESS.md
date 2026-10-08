# SWITCH13 - why some toggle switches lag (settings pages) while AzWidgets' are smooth

Worktree branch `worktree-agent-a74fe8ae45a4fc609`, fast-forwarded to cbd0ae0a8 (the lead's HEAD).

## Measured (headless, prebuilt 04:25 binaries, dylib 04:16; scripts/switch13_probe.py + AZ_E2E traj)
- The user's look: ~/.azlin/config.json = flora:green dark (every non-pinned Azlin app); AzWidgets
  defaults to flat. The user toggled AzCalc's grouping / history and AzClock's 12-hour switches.
- Display-list rebuilds per toggle (frame report, 0.8 s window): AzCalc settings flora:green dark
  9-11, flat 2-3; AzWidgets flora dark 15-16, flat 3-4. Scripted-clock trajectory: identical knob
  path (11.4, 6.0, 2.8, 1.2, 0.5 px ...) in both; flora's dl_rebuilds +1 EVERY frame, flat's never.
- AZ_PROFILE=cpu per glide frame: AzCalc flora css_transition_tick 350 us + dl_regenerate_full
  610 us + raster 126 us (~1.1 ms) vs flat 24 us + raster 71 us; AzWidgets flora 2.95 ms + 7.7 ms
  + 0.82 ms (~11.5 ms) vs flat ~0.6 ms.
- Click -> glide start: AzWidgets regenerate 260-360 ms (3472 nodes), AzCalc 14 ms.
- Settings toggles also spawn a save thread whose write-back (`on_settings_saved`) returns
  RefreshDom unconditionally: a second whole-window rebuild per toggle (coalesced in headless,
  1-2 frames later on macOS, mid-glide).

## DONE
- d194d337e RED test layout/tests/a_gradient_face_fade_frame_is_patched_in_place.rs (2 tests)
- 8aedf01ae fix: DisplayList::patch_background_layers + tick_animations layered-background patch

## NEXT
- appkit: on_settings_saved returns RefreshDom only when the notice changed (pure helper in
  options.rs + unit test).
- engine (if time): a rebuild mid-glide restarts a running transition whose end value it did not
  change (CSS Transitions s3 says keep it) - RED test + fix in begin_reconciliation.

## Open questions
- None blocking.
