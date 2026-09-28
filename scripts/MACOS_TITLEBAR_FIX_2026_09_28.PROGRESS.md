# macOS titlebar + bold system font fix: progress checkpoint

Branch `wt/macos-titlebar` (from `5414bfa6b`, PR #476 base `fix/input-bugs-2026-09-19`).
Rules: no cargo/rustc/LSP; a RED commit before each fix; explicit staging; commit this file
after every commit; delete it in the last commit (the final report
`scripts/MACOS_TITLEBAR_FIX_2026_09_28.md` replaces it).
Source of truth: `scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md` (sections 0, 4.1, 5.3, 8 P0).

## DONE

- `080968b40` RED fonts: css `a_bold_system_font_on_macos_is_the_system_font_first`,
  `the_macos_title_is_bold_like_the_titlebarfont`; new
  `layout/tests/a_bold_request_draws_the_bold_instance_of_a_variable_font.rs` (RedHatMono VF from
  `doc/fonts` registered as a FILE must resolve to its baked 700 instance; macOS-gated SFNS test).
- `fe402914c` FIX fonts: the macOS UiBold/TitleBold chain is `[System Font, Helvetica Neue,
  Lucida Grande]`; `TitlebarMetrics::macos().title_font_weight` is 700. getters has
  `select_variable_weight_instances` + `variable_weight_instance` (process-wide memo, tri-state
  `WeightInstance`) + `bake_weight_instance_into`, called from both resolvers and from
  `text3::cache::resolve_chain_on_miss`.
- `9e30ba3a5` RED titlebar geometry: new
  `layout/tests/the_macos_titlebar_lines_up_with_its_traffic_lights.rs` (harness `laid_out`,
  `nodes_with_class`, `border_box`, `centre_y`, `text_run_centre_x`, all `pub(crate)`).
- `00ee8a43b` FIX titlebar centring: the container is flex in both modes (title-only = column +
  justify-content:center; CSD = row + align-items:center); the title has no padding-top.
  Generated tests updated.
- `5ea0f49f8` RED separator: `the_macos_titlebar_has_no_fill_and_the_system_separator` (light
  #D0D0D0, dark #000000, width > 0, solid, no BackgroundContent).

- `6f41cf5a5` this checkpoint file.
- `15cc3df99` FIX separator/background: `TitlebarMetrics.separator_color/_inactive/_width`
  (macOS width 0.5px, presets #D0D0D0 / #000000); `Titlebar.separator_color/_inactive/_width`
  (appended), const-fn builders `set_background`, `with_background`, `with_background_inactive`,
  `set_border_bottom`, `with_border_bottom`, `with_border_bottom_inactive`,
  `without_border_bottom`; container emits box-sizing border-box + border-bottom (+ dark twin
  for the default colour, + :backdrop); `dom_controls_only` draws no line.

## IN PROGRESS

- Demo RED (NEXT 1 below).

## NEXT (in order)

1. RED demo: make `page_frame`/`PageFrame` in `layout/tests/azul_widgets_demo_follows_the_theme.rs`
   `pub(crate)`, add `label` (= `lits[label + 1]`). Add demo geometry tests to the traffic-lights
   file (rebuild `body > titlebar > [title > text, label > text]` from the demo styles, and
   assert height 28 (38 today), title-run centre x 240 (left-aligned today), and title centre
   y 14 (19 today)).
2. FIX demo `examples/azul-widgets/src/lib.rs` ~L825: `height: 28px; box-sizing: border-box;
   display:flex; align-items:center; position: relative; padding-left/right: 78px; border-bottom:
   0.5px solid system:separator;` no fill; title `font-family: system:title:bold; font-size:
   13px; flex-grow:1; flex-basis:0px; min-width:0px; text-align:center; ...`; label `position:
   absolute; top:0; right:12px; height:28px; display:flex; align-items:center; ...`. Keep the
   literal order (titlebar style, "Azul Widget Showcase", title style, "custom titlebar", label
   style, body style, scroll style) and only `system:` colours (the theme tests scan them).
   Update the comment at L62-68.
3. Write `scripts/MACOS_TITLEBAR_FIX_2026_09_28.md` (commits + expected REDs, API changes,
   least-sure-to-compile spots, open items) and delete this file in the same commit.

## Open questions / risks

- Whether allsorts `variations::instance` bakes SFNS (4 axes incl. avar/MVAR/trak, gvar 7 MB).
  If it fails, the SFNS test stays red and bold renders SF regular (no longer Helvetica Neue).
- Bake cost on first use (SFNS ~0.1-0.5 s?, once per process per weight). No disk cache.
- Title-only padding is still button_area/2 per side (39+8), so a long title can run under
  the lights (x 8..60). Left as an open item, not fixed.
- `create_csd_stylesheet` still hard-codes `border-bottom: 1px rgb(200..)`; the inline
  separator overrides it when set.
