# LAYOUTPERF8 progress (wave 8, branch wt/layoutperf8, base e290321da)

Task: a one-node change (the AzWidgets switch knob's margin-left tween) re-lays out almost the whole page
(118-290 ms per frame, 2853 text re-flows for a 16 px move; ANIM8 report section 4). Root-cause why the
cached layout is not reused for the unchanged parts; RED test counting re-laid-out nodes / text re-flows for a
one-node change in a large tree; fix. Never compile; never touch page_breaks.rs.

## DONE
- (this file)

## IN PROGRESS
- reading: house rules, PLAN, ANIM8 report + probes (done); next: read solver3 cache / dirty marking.

## NEXT
- reproduce on prebuilt AzWidgets (capped, headless), read solver3/cache.rs + mod.rs ~866.

## Decisions / open questions
