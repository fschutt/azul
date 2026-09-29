# W5B_FLORA_TABS - progress (branch `wt/w5b-flora-tabs`, from 0a326afe5)

Task: a flora look (light + dark) and a theme option for tabs (TabHeader,
TabContent), titlebar and tree_view; unpinned they follow the app theme through
`theme_blocks::follow_props` (U1 moves T3's helper there - the build waits for U1).

Shape (per widget): the widget file keeps ONE structure builder that takes a
`<Widget>Look` (one style per part + the marker class); `themes::flat` /
`themes::flora` supply the looks (appended `// ==== <widget> ====` sections);
`theme: None` merges the two looks part by part with `follow_props`, marker of
the structure theme (`UiTheme::current()`). Per widget: plumbing (theme field,
Look, flat look, flora = flat placeholder, autotests pinned to flat) -> RED ->
flora look.

## DONE
- tree_view: 068155ead plumbing, ecd7c8b92 RED, cb2fcd9c1 flora
- tabs: 79e0dc64e plumbing, e48d6d8bc RED, e53a85308 flora

## IN PROGRESS
- titlebar

## NEXT
- titlebar: plumbing (TitlebarLook; flat keeps build_container_style / build_title_style /
  build_button_container as its parts), RED, flora (+ make
  `the_macos_titlebar_has_no_fill_and_the_system_separator` read the flat block)
- report

## Open questions
- flat tree labels keep their parked half-pair (KNOWN_HALF_PAIRS); flora's pairs are whole.
