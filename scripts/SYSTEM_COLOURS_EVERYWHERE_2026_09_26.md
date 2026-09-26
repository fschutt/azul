# system: colours in every colour property (2026-09-26)

User ruling: "parse_color_or_system_token is only used by the border and
text-colour parsers -> thats a bug, should be used everywhere".

Worktree branch `wt/system-colours-everywhere` (from 4ace6fcbe, worktree
`.claude/worktrees/agent-a1da2fad1e124a185`), **UNCOMPILED**, NOT yet integrated.
The agent continues on the same branch with the demo follow-up (see below).

```
bb58d4b6b fix(layout): CSS transitions interpolate resolved system: colours
efcfb209f fix(web): the HTML export writes system: colours out resolved
d8da31e26 fix(xml): SVG fill and stroke attributes accept system: keywords
590fb66e2 fix(css): every colour parser accepts system: keywords
93e49bb59 fix(layout): every colour getter resolves system: keywords at one point
4380faba8 feat(css): one trait resolves system: colours in every colour value type
831a6f7dd test(css): a system: colour keyword resolves in every colour-valued property   <- RED
```

## Design

- `ResolveSystemColors` trait (css/src/dynamic_selector.rs), one method
  `resolve_system_colors(self, ctx)`, implemented by every colour-carrying value
  type: the `{inner: ColorU}` types (text, caret, both selection colours, the four
  border sides, column-rule), `StyleBoxShadow`, `StyleFilter`/`Vec`,
  `StyleScrollbarColor`, `StyleBackgroundContent`/`Vec` (the old
  `resolve_system_background`, moved), `ColorOrSystem`, `BoxOrStatic<T>`, and the
  containers `Option<T>`, `CssPropertyValue<T>`, `CssProperty`.
- Layout's one resolution point: `getters::system_colors_resolved(styled_dom,
  value)`, against `css_property_cache.ptr.dynamic_context`. Without a context a
  keyword takes its light built-in default (`resolve_system_color_ref(r, None)`);
  the scrollbar getter falls back to a context built from the system style.

| Property | Parser | Read sites | Resolved now |
|---|---|---|---|
| color | token parser | style props, DL live colour, caret fallback | helper |
| background(-color/-image), CSS fill | background parser | background getters | trait via helper |
| border*-color, border, stroke, column-rule shorthand | border token parser | get_border_info; fc.rs | helper (fc.rs direct resolver) |
| caret-color | NEW | get_caret_style | helper |
| -azul-selection-background-color / -color | NEW | get_selection_style | NEW |
| scrollbar-color | NEW | get_scrollbar_style | trait NEW |
| -azul-scrollbar-track/thumb/button/corner | already accepted | get_scrollbar_style | NEW (was TRANSPARENT) |
| box-shadow, -azul-box-shadow-* | NEW | box shadow getters | NEW |
| text-shadow | NEW | get_text_shadow + DL raw read | NEW |
| filter / backdrop-filter (flood, drop-shadow) | NEW | get_filter / get_backdrop_filter + DL raw reads | NEW |
| column-rule-color | NEW | no painter | CssProperty impl |
| SVG fill= / stroke= attributes | NEW (core/src/xml.rs) | background/border getters | as above |
| HTML export | - | dll/src/web/html_render.rs | NEW |
| CSS transitions | - | window.rs endpoints | NEW |

Expected RED (831a6f7dd): 17 of 27 rows fail in both themes (34/54 checks);
10 control rows pass. Plus `svg_fill_and_stroke_attributes_take_a_system_keyword`
and `the_display_list_hands_the_renderers_resolved_shadow_and_filter_colours`.
No FFI type changed (no api.json sync). New Rust-only items:
`ResolveSystemColors`, `resolve_system_color_ref`, `getters::system_colors_resolved`.

Least sure to compile: closures coerced to fn pointers in a `const ROWS`; Copy
assumptions for `&StyleFilter` / stops; `BoxOrStatic<T>` impl (`heap`,
`into_inner`); `css_of` in html_render.rs (web feature only); the DL test needs
`position: relative; z-index: 1` for PushFilter.

Not done: prop_cache debug string prints tokens as hex; SVG `stop-color`
unparsed; no painter for `-azul-scrollbar-resizer` / column rules; a filter alone
creates no stacking context (existing gap).

## Follow-up (running): the demo

User on the integrated build (dark mode): "the azwidgets demo now doesn't have the
'system:ui' and 'system' colors that it should have" and "the azwidgets is now
'dark text on dark mode' for the title". The agent is rebasing onto the PR branch
and making the demo use the system palette directly (both themes), with a RED
contrast test for the heading/titlebar title.
