# SWITCH13 - why some toggle switches lag (settings pages) while AzWidgets' are smooth

Worktree branch `worktree-agent-a74fe8ae45a4fc609`, fast-forwarded to cbd0ae0a8 (the lead's HEAD).

## Measured (headless, prebuilt 04:25 binaries, dylib 04:16; scripts/switch13_probe.py)
- The user's look: ~/.azlin/config.json = flora:green dark (every non-pinned Azlin app); AzWidgets
  defaults to flat. The user toggled AzCalc's grouping / history and AzClock's 12-hour switches.
- Display-list rebuilds per toggle (frame report, 0.8 s window): AzCalc settings flora:green dark
  9-12, flat 2-3; AzWidgets flora dark 15-16, flat 3-4. Scripted-clock trajectory: identical knob
  path (11.4, 6.0, 2.8, 1.2, 0.5 px ...) in both; flora's dl_rebuilds +1 EVERY frame, flat's never.
- AZ_PROFILE=cpu per glide frame: AzCalc flora css_transition_tick 350 us + dl_regenerate_full
  610 us + raster 126 us (~1.1 ms) vs flat 24 us + raster 71 us; AzWidgets flora 2.95 ms + 7.7 ms
  + 0.82 ms (~11.5 ms) vs flat ~0.6 ms.
- Click -> glide start: AzWidgets regenerate 260-360 ms (3472 nodes), AzCalc 14 ms.
- Settings toggles also spawn a save thread whose write-back (`on_settings_saved`) returned
  RefreshDom unconditionally: a second whole-window rebuild per toggle (coalesced in headless,
  1-2 frames later on macOS, mid-glide).
- Flat settings page: the 2nd switch's track was in the paint damage of every frame of the 1st
  switch's glide -> subtree_len bug (the colour patch recoloured it).

## DONE (RED first, then fix)
- d194d337e RED a_gradient_face_fade_frame_is_patched_in_place.rs (2 tests)
- 8aedf01ae fix: DisplayList::patch_background_layers + tick_animations layered-background patch
- 6fe19d050 RED appkit options::a_save_that_changes_nothing_on_the_page_draws_nothing
- 07b040710 fix: notice_after_save -> on_settings_saved redraws only when the notice changed
- 068b5f641 RED a_rebuild_mid_glide_keeps_the_running_transition.rs (2 tests, 1 RED)
- daedfc7e8 fix: begin_reconciliation keeps a tween running toward the value the rebuild keeps
- ef904c89c RED core subtree_len_of_a_last_child... + layout a_switchs_fade_leaves_the_switches_after_it_alone.rs
- 02be09d64 fix: core NodeHierarchy::subtree_len ends at the nearest ancestor's next sibling

## NEXT
- Final report. Latent (reported, not fixed): apply_node_css_change seeds CssTransitions for a
  VirtualView child DOM, but tick_animations drives ROOT only and writes the override onto the
  root node with the same index.

## Open questions
- None blocking.
