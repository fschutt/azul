# Dark mode + system colours (2026-09-26)

Worktree branch `wt/system-colours` (worktree
`.claude/worktrees/agent-a2050d36f3988c5d7`, based on 97f211f96), 16 commits,
**UNCOMPILED**; RED values reasoned, not observed. The agent used helper
agents (forks) for the widget groups.

| Commit | Expected RED / what it fixes |
|---|---|
| 1c5954108 test(css): a system colour keyword resolves against the theme | `a_system_background_...`: None vs Some(255,255,255,255); `a_system_text_colour_...`: (0,0,0,255) vs (0,0,0,221); `a_system_border_colour_...`: None vs Some(0,122,255,255) |
| 0d00729e3 fix(css) | keywords resolve against the palette of the theme the cascade uses |
| e95c3cc1e test(css): every system colour slot has a keyword | "15 of 24 slot keywords do not parse"; field background (0,0,0,0) vs (255,255,255,255) |
| d84afaa81 feat(css) | keywords for all 24 slots + 3 new field slots |
| 1d57a938b fix(examples) | `margin: 0` on the demo body (integrator request) |
| f67893b43 feat(shell) | macOS / Windows / KDE probes fill the new slots |
| 29ea975b2 test(css): a nested @-rule keeps the declarations around it | `plain.len() == 1` sees 0 |
| 80d4e3952 fix(css) | the parser gives a nested @-rule its own nesting level |
| 3c1f892e2 test(examples): every colour the demo paints follows the theme | "N demo style(s) paint a light-theme colour with no dark counterpart", N = 22-24 |
| 89bd20d4b fix(examples) | every demo colour gets a `system:` twin under `@media (prefers-color-scheme: dark)` |
| 330c7095f test(widgets): every widget's text stays legible in its theme | group tests fail at `assert!(bad.is_empty(), ...)`: status N=18, containers N~9 (1.23:1 body text), inputs N~57 (39 date picker), chrome N~14; flat fields: text_input dark surface Some(52,58,64,255) vs Some(30,30,30,255) |
| f157080db fix(widgets): Info button writes black | drops the 2 "button info" findings (white on cyan is 1.96:1 in light too - the one light value moved on purpose) |
| 00df362cb fix(widgets): inputs | new `themes::system_palette`, flat theme + input widgets |
| 3a71c3268 fix(widgets): alerts, toasts, default chip | status group green |
| a5baf573f fix(widgets): cards, dialogs, panels, tabs | containers group green |
| 801aadfc2 fix(widgets): breadcrumbs, pagers, steppers, title bars | chrome group green |

## Public types changed (run the api.json autofix)

- `SystemColorRef`: 15 new variants appended after the original 9; new `ALL`, `get`, `resolve_for_theme`, `fallback`, `from_css_name`, `to_color_token`, `from_color_token`.
- `SystemColors`: 3 new fields (`control_background`, `placeholder_text`, `text_selection_background`), now derives `Hash`.
- `SystemStyle::colors_for_theme`.
- `DynamicSelectorContext`: new field `system_colors`, method `system_color`.
- free fns `dynamic_selector::resolve_system_color_token`, `color::parse_color_or_system_token`.
- layout: public module `widgets::themes::system_palette` (consts + fns), `flat::FIELD_PLACEHOLDER_DARK`. No widget struct changed.
- Outside the allowed paths: `core/src/resources.rs` (mock `apply_to` resets `system_colors` with the theme).

## Least sure to compile

`const fn` chain on `SystemColorRef` (`fallback`, `to_color_token`) + palette
consts built from it; widget `static` style arrays embedding
`system_palette::DARK_*`; the macOS probe's `responds_to` / `q_if_known!`,
Windows `GetSysColor` indices, KDE reads; the theme-contrast harness
(`StyledDom::create_from_dom_with_context`, getters); updated per-widget tests
allowing dark twins (label, radio_group, modal, titlebar, pagination,
stepper). The widget click handlers' theme check (AZ_THEME pin, then the
window theme) is copied 4x (segmented, date_picker, pagination, stepper) -
fold into one helper.

## Demo in dark mode, before -> after (macOS dark preset / generic fallback)

| Surface | Before | After |
|---|---|---|
| Page | #f2f4f7 | `system:background` (28,28,30) |
| Cards, panel, titlebar row, tab pane | #ffffff | `system:window-background` (44,44,46) |
| Drop / menu zones | #f9fafb | `system:control-background` (30,30,30) |
| Headings | #1d2939 / #101828 | `system:text` rgba(255,255,255,221) |
| Labels | #667085 / #475467 | `system:secondary-text` rgba(255,255,255,140) |
| Quiet hints | #98a2b3 | `system:tertiary-text` rgba(255,255,255,64) |
| Rules | #d0d5dd / #e4e7ec | `system:separator` rgba(255,255,255,26) |
| Idle tab | #e4e7ec | `system:under-page-background` (40,40,40) |
| Grip bar | #eaecf0 | `system:selection-background-inactive` (70,70,70) |
| Drop zone idle / hover | #eef4ff / #2970ff | `system:text-selection-background` (63,99,139) / `system:accent` (10,132,255) |
| Text fields | #343a40 / #f8f9fa | `system:control-background` (30,30,30) / `system:text` |

Notes: platform values come only from the desktop probes (presets untouched;
missing slot -> platform-neutral default); metrics keywords dropped (no
desktop reports them). Still hard-coded: alert/toast Bootstrap dark tints (no
desktop slot for semantic tints), the titlebar's no-desktop dark default.
Left as is: tooltip, spinner track, progress bar (outside the harness), the
switch track (recoloured by code at runtime, no dark twin).
