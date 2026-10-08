# TABS13 - flora tabs: the gold like the website (flora.css)

Worktree: .claude/worktrees/agent-a44bac5bf68dda4ac, branch worktree-agent-a44bac5bf68dda4ac
(fast-forwarded to cbd0ae0a8). Nothing compiled (house rule); the lead compiles.

## Findings (site vs app)
- Rule: site = `.navbar::after`, 2px `--fl-rule-metal-bg` (90deg: dim brass at both ends, cream
  highlights at 32% / 68%); app = a flat #C6B279 inset shadow (`flora::tab_rule`). THEME.
- Selected tab metal: site = `background: gem-sunken padding-box, --fl-rolled-tab border-box`
  through a transparent 2px border (cream head rolling to #C6B279 at the cove's tangent); app =
  flat #C6B279 top border + flat #C6B279 S strokes. ENGINE (per-layer background-clip) + THEME.
- Run-outs (`.fl-tab-runout-l/-r`, 34px easing the rule into the turn colour): app has none. THEME.
- Transitions: effective site rule is `.nav-links a { transition: background/border-color/color/
  box-shadow var(--fl-dur) var(--fl-ease), filter var(--fl-dur-slow) }` + `:active` fast; app
  used the overridden `0.2s ease`. THEME (declare; interpolation is BUTTONS13's).
- Unselected hover: site adds a 1px `--fl-bd` border (top/left/right), pressed `--fl-bd3` +
  `inset 0 1px 3px`; app had none. THEME.
- ENGINE: `background: A, B` paints B over A (CSS: A on top) - layer order of CSS text reversed.
- ENGINE: `background-clip` is one value for all layers; the `background` shorthand rejects a
  `<visual-box>` per layer (`... padding-box, ... border-box` does not parse).

## DONE
- c462141d6 RED: the_first_background_layer_is_painted_on_top, each_background_layer_is_clipped_to_its_own_box
- f93b2bd8e fix(css): background lists in paint order (CSS text was upside down) + layer_value
- 351af4270 feat(css): background-clip per layer (StyleBackgroundClipVec), shorthand `<visual-box>`

- 6e7b1eaef progress
- 65a1625bf RED: flora tab rows (tabs.rs theme_tests + australis_tests, ribbon flora_tests,
  theme_checks flora_css_* transcriptions, background_clips, fades)
- a85e77131 fix(flora): rule = strip's transparent 2px foot over RULE_METAL; selected tab /
  app button = stone padding-box over ROLLED_TAB border-box (transparent head, margin-bottom
  -2px); S = filled band of ROLLED_TAB (tabs::curve_band); run-outs; hover hairline; fades
- 548e9d653 test: painted flora tab row (layout/tests/a_flora_tab_row_is_cut_from_the_websites_metal.rs)

## IN PROGRESS
- final review, report.

## NEXT
- (lead) compile + run: css, layout --lib, layout --test all; autofix api.json; regenerate
  lower_types; bless codegen goldens (families case background-clip).

## Answered
- `.fl-tab-foot`: leave the blur out where the tab opens onto paper (coordinator); the tab must
  be open at its foot - no line - and the rule runs only under the unselected tabs to both edges.
