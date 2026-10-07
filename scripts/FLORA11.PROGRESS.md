# FLORA11 progress (flora polish, spins, Garamond, linen, shared theme config)

Worktree: .claude/worktrees/agent-a6a847892daffedc9 (fast-forwarded to fix/input-bugs-2026-09-19 bf61a1c70).

## DONE
- 1877a7dc9 RED spin test; bb350f6bc spins (themes/spin.rs + window.rs hook); c07832f0f design-system seasonal ramps
- c7fde4c7b EB Garamond 400/700 bundled (text3/ui_fonts.rs, fallback tier)
- 00b1372b3 flora buttons (kinds, gem, metal edge, double ring, caps, ButtonType::Illuminated)
- design reference: ~/Downloads/Azlin OS design system.html (widget specimen extracted to /tmp/azlin_ds/widgets_section.html); flora.css wins on conflict

- 3d6da796a fields/check box/switch/slider/progress; 7b431c034 field font 14px
- 0b888fd0a FONT_CAPS rename (CHROME11's name)
- 914807832 appkit shared ~/.azlin/config.json + Theme spins + pins (Writer/Sheets/Show)
- 179538b88 linen ground + Garamond shells; 8f138551e dialog band, popover skin, oak tooltip

- 3955cb11d flora menus + dropdown ticks its choice; c9c0bef02 scrollbar sheet; 22c7b1ebf frame caps / accordion serif / radio stone
- d6828dbe4 --shot runs ignore the shared look

## IN PROGRESS
- (none) - report to the lead

## NEXT (for the lead / later)
- build + run the suites listed in the report; api.json autofix for ButtonType::Illuminated
- merge with CHROME11: keep one FONT_CAPS in flora.rs
4. dropdown popup, menu, tooltip, dialog band, scrollbar
5. linen ground
6. EB Garamond bundling
7. ~/.azlin/config.json shared theme (appkit), pins: Writer flora, Sheets flora:green, Show flora:red

## Open questions
