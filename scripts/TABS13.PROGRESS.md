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
(none yet)

## IN PROGRESS
- RED tests for the two engine gaps.

## NEXT
- fix E2 (layer order), fix E1 (per-layer clip), then the theme (rule node, rolled metal on the
  S band and the tab, run-outs, transitions), then the tests in tabs.rs / ribbon.rs.

## Open questions
- `.fl-tab-foot` (blurred accent bleeding into the band): the app's tabs open onto a leaf, not
  the blue band - left out unless the lead wants it.
