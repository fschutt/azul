# L3_POLISH progress

Branch `wt/l3-polish`, base `748f999af`. Report: `scripts/L3_POLISH_2026_09_29.md`.

## DONE

- Item 3: `1e47bfa51` (RED), `cd5c112b3` (fix) - `form_controls::graft` lowers a grafted
  `*:hover` rule into a condition.
- Item 2: `f12061f2b` (RED), `fab8bcb9a` (fix) - `apply_node_style_change` keys "changed" by
  every declaration (`changed_declaration_keys`), re-runs the author cascade (variable pass) when a
  `var()` / `env()` / `--x` is involved.
- Item 4: `03c1243cf` - dock-panel shadow back to `rgba(16, 24, 40, 0.1)`.
- Item 1a: `4bbe89050` (RED), `b81940907` (fix) - `get_component_thumbnail` takes `dark`
  (`builder::preview_in_dark_mode`). Resumed after a power loss: the uncommitted fix was
  complete and was committed as it was.

## IN PROGRESS

- Item 1b: debugger page light + dark (tokens on `:root`, ONE dark block under
  `@media (prefers-color-scheme: dark)`, header toggle rewrites its media condition), palette
  thumbnails requested with `dark`, smoke `scripts/debugger-ui/builder-mode-smoke.mjs` +
  screenshots `scripts/debugger-ui/screenshots/light-dark-*.png`, rerun every node test + smoke.

## NEXT

- Report `scripts/L3_POLISH_2026_09_29.md`.

## Open questions

(none)
