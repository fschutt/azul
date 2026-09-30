# MAILVIEW progress (engine bugs that block mail display)

Branch `wt/mailview-engine` from `d8be2016d`. Report: `scripts/MAILVIEW_2026_09_30.md`.
Never compiled (house rule): the parent compiles and runs the suites.

## DONE
- 1. E-XML-2 href: RED `2b9071aa0`, first fix `589f75168` (attribute-table entry) REPLACED by ruling: RED `76363ee28`, fix `3d158593f` (component arguments: data_model_with_attributes + apply_builtin_element_args, both loaders + builtin render fn)
- 7. E-XML-4 entities: RED `a9121e604`, fix `e81a2e7cb`
- 6. E-CSS-1 CDO/CDC: RED `e667598a2`, fix `dab0ffad1`
- 3. E-UA: RED `d879dc38e`, fix `3e400235d` (also: UA beats inherited on elements + hand-down)
- 4. E-BR: RED `3479422c8`, fix `0074a100d` (IFC height = line boxes)
- E-RUN (found): run extent misses its last glyph (underline/background/hit area): RED `6f9e20da0`, fix `370ee7177`
- 2. E10: RED `eb8c3036f`, fix `13add42a7` (inline fragments from DL text-run cursor areas)
- 5. E-OL: RED `8db89d933`, fix `ce06c2aca` (marker outside its text clip; CPU LCD tile path clips, sweep does not)

- 8. E-MODE DROPPED from the engine by ruling (RED f0b198335 removed in e062843b8). Replacement per corrected ruling: AzMail paper that follows the mail's dark rules: RED `1473e3954`, feat `93e756fb5`

- 9. R1's open table items (mixed cell, min-content cells, colspan in intrinsic sizes): RED `44d2b60be`, fix `d12ef9fa6`, closure fix `4962371a7`
- Report written: `scripts/MAILVIEW_2026_09_30.md`

## IN PROGRESS
(none - task complete, awaiting the parent's compile and test run)

## NEXT
(none)

## Open questions
(none)

## Found on the way (not fixed yet)
- first text line of a mounted doc not painted in AzWidgets headless when the window is < ~700px tall (DL has it, damage full, not the pretile path); AzMail at 760x400 fine - likely AzWidgets per-NodeId state surviving `mount`
- `list-style-position: inside` on <ol> not honoured (markers stay outside)
- CPU raster: pretile LCD path clips per pixel to the text item clip, sweep/grayscale paths do not
