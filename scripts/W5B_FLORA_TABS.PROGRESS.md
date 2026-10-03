# W5B_FLORA_TABS - progress (branch `wt/w5b-flora-tabs`, from 0a326afe5)

Task: a flora look (light + dark) and a theme option for tabs (TabHeader,
TabContent), titlebar and tree_view; unpinned they follow the app theme through
`theme_blocks::follow_props` (U1 moves T3's helper there - the build waits for U1).

## DONE
- tree_view: 068155ead plumbing, ecd7c8b92 RED, cb2fcd9c1 flora
- tabs: 79e0dc64e plumbing, e48d6d8bc RED, e53a85308 flora
- titlebar: 8f23902c4 plumbing, 715aa42ad RED, da220f323 flora
- report: scripts/W5B_FLORA_TABS_2026_09_29.md

## IN PROGRESS
- nothing

## NEXT (parent)
- build once U1's `theme_blocks::follow_props` is in; run the suites in the report, section 5
- autofix api.json (report section 4); decide TabContent's field order
- merge with W5a (single-look guard, appended theme sections)

## Open questions
- TabContent: `theme` LAST after `has_padding` leaves 3 bytes of interior padding.
- The flat title's `:backdrop` colour is pushed before its resting colour (never applies).
