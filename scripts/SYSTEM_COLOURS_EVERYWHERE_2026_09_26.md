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

## Follow-up done: branch rebased onto fix/input-bugs-2026-09-19 (39a96b9bc), UNCOMPILED

```
dcc171ca4 fix(examples): the widgets demo paints from the system palette directly
e15e43bea test(examples): the widgets demo paints from the system palette directly   <- RED
c957af93c fix(css): a child inherits the declaration that won on its parent
9e797cb1a test(css): the text inside paints the colour that won on its parent         <- RED
dc5d05081 .. 843dfa788  (the 7 system-colour commits above, rebased without conflicts)
```

**"Dark text on dark mode for the title" - root cause (a regression EXPOSED by
4d3ba32fe):** in `CssPropertyCache::restyle` (core/src/prop_cache.rs, inheritance
step 2) a child inherits its parent's stylesheet declarations BEFORE they are
deduplicated; the parent's list holds every matching declaration in order
(`color: #101828`, then the dark twin `system:text`) and the child's insert keeps
the FIRST per property -> the text node inherits #101828. The div itself is right
(its own list is last-wins), the compact tier is right (inherits the parent's
compact value) - which is why `get_style_properties` harnesses stayed green - but
`compute_inherited_values` lets the wrong inherited value win and the display
list's live text colour re-resolve paints it. Before 4d3ba32fe the dark block came
first, so first-wins picked the twin by accident. Fix c957af93c: dedupe the
parent's list last-wins per property before the child sees it (as the inline step
already does). RED 9e797cb1a: `inline_media_follows_source_order::the_text_inside_
paints_the_declaration_that_won` (dark: #ff0000 instead of #0000ff; block-then-plain:
#0000ff instead of #00ff00) and `azul_widgets_demo_follows_the_theme::the_page_and_
titlebar_titles_are_legible_in_both_themes` (dark: #101828 on (44,44,46)/(28,28,30)
~1.3:1).

**Demo on the system palette:** "system:ui" is azul's `font-family: system:ui`
(no CSS `system-ui` generic support found). RED e15e43bea
(`the_demo_paints_from_the_system_palette_directly`: ~40 fixed colours + 27 twins
today). dcc171ca4: one `system:` value per colour, no twins - page
`system:background`; cards/titlebar/dock panel/active tab/pane
`system:window-background`; drop zones + context box `system:control-background`;
text `system:text` / `secondary-text` / `tertiary-text`; rules/borders
`system:separator`; file-hover `system:accent` on `system:text-selection-background`;
body `font-family: system:ui; color: system:text`; code spans `system:monospace`.
Only non-system colour: the dock panel shadow `rgba(0,0,0,0.1)` - the old
`rgba(16, 24, 40, 0.1)` never worked (the shadow parser splits at spaces and
dropped the declaration: a separate parser bug to fix).

Least sure to compile: the new demo-theme test's literal-index lookup (assumes
the demo's current style order), `NodeId::new(<literal>)`. Not checked: whether
any core test depended on first-wins inheritance (none found by grep).
