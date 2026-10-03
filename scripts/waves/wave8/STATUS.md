# Wave 8 resume table (base 0836ecb96, started 2026-10-03 ~15:50)

Resume after an outage: scripts/waves/README.md. A stopped agent: SendMessage its id ("continue from your
progress file"). Worktrees: /Users/fschutt/Development/azul/.claude/worktrees/agent-<id>.

| Task | Agent id | Branch | State |
|---|---|---|---|
| ANIM8 | a69fbac481ec54de5 | wt/anim8 | DONE (22 commits; report scripts/ANIM8_2026_10_03.md; toggle: node hash was order-dependent -> reconcile swapped twin switches (core/src/dom.rs); hover fades never existed -> seed_state_change_transitions + 120 ms button fades; rebuild compares against restored states; no api.json; OPEN: a knob slide relayouts the whole page, 118-290 ms/frame -> LAYOUTPERF8) |
| MAILREF8 | aac256efd68cb4d1a | wt/mailref8 | DONE (report scripts/MAILREF8_2026_10_03.md; 531 -> ~75 expected (only Postmark left, XML8); + sans-serif -> Helvetica on macOS (layout/src/font.rs, every font cache), <hr> 2px inset (core ua_css.rs); no api.json) |
| WPT8 | ab3291d23e4f4deb8 | wt/wpt8 | DONE (93d621c04; report scripts/WPT8_2026_10_03.md; 7/8 items: canvas background propagation, inline box decoration/margins/padding/baselines, single-stop gradients, NEW background-clip property, border defaults (3px medium, currentColor), box-shadow (inline-block, inset), wpt budget 2500px; counters partial; api.json: StyleBackgroundClip(+Value), CssProperty/CssPropertyType::BackgroundClip + 3 fns; bless with AZ_WPT_BLESS=1) |
| WAYLAND8 | a5a0fcdd7da40495e | wt/wayland8 | DONE (38 commits; report scripts/WAYLAND8_2026_10_03.md; udmabuf-ready shm (sealed memfd, 256-byte pitch, page-aligned slots), idle spare slot released, one shm allocator, X11 MIT-SHM with XPutImage fallback; no api.json; needs a Linux run) |
| ABI8 | a570ada0acafb9cbe | wt/abi8 | DONE (report scripts/ABI8_2026_10_03.md; Rust first-call kinds + load-time check, MSVC C branch, azul.h macro shadowing fixed; scripts/abi_guard_e2e.py; no api.json) |
| THREADS8 | a4a2effff66331779 | wt/threads8 | DONE (report scripts/THREADS8_2026_10_03.md; task already done in 7c1c12389 - fixed 3 gaps: video workers honour stop, stop_all on window close (Drop for LayoutWindow), mount-started timers stop on unmount; no api.json) |
| XML8 | acfe4639b5484119d | wt/xml8 | DONE (23 commits; report scripts/XML8_2026_10_03.md; spec tokenizer + rule-table tree builder (xml_html_rules.rs / _tokenizer.rs / _tree.rs); Python port = Chrome on all 20 corpus mails; probe + port kept in ~/Development/azul-work/evidence/xml8; mail corpus now 20 mails (2 AzMail samples); no api.json) |
| VIDEO8 | a6e731cce5b11a63a | wt/video8 | DONE (report scripts/VIDEO8_2026_10_03.md; listed correctness items were already fixed (MEET2); built: bitrate adaptation, Opus via AudioToolbox, Vulkan H.264 encode (untested), echo canceller; zero-copy designed only; api.json REQUIRED: VideoEncoder.set_bitrate, AudioEncoder, AudioDecoder, OptionAudioFrame, EchoCanceller) |
| RULINGS8 | addd801aa5b76e026 | wt/rulings8 | DONE (report scripts/RULINGS8_2026_10_03.md; focus walks get_event_path across the VirtualView host; inline-block lines: strut, strut font, vertical-align middle, flex/grid baselines, empty inline = 0 height - 5 causes; widget CSS unchanged (none needed); 4 tests need MAILREF8 merged too; no api.json) |
| LAYOUTPERF8 | a8bde8e3457166c14 | wt/layoutperf8 | MERGED, DONE (report scripts/LAYOUTPERF8_2026_10_03.md; clone_node_from_old dropped every node\'s cached flex measurements -> a one-box slide re-laid out 12181 flex items + 2853 text runs; kept now (dropped only under a restyled node / id change; vw/vh clear all); anonymous text wrappers keep their layout; expected 157-309 -> ~20-35 ms/frame) |

2026-10-03 integration: 9 of 10 merged (THREADS8, ANIM8, XML8, ABI8, WAYLAND8, VIDEO8, MAILREF8, RULINGS8, WPT8 - only
append conflicts); api.json converged 113ea54f1 (XML8's `Content` enum renamed ContentModel - autofix had proposed
replacing the CSS Content type with it; ANIM8 closure lifetime fixed). NOT BUILT - waiting for LAYOUTPERF8.
2026-10-03: ALL TEN MERGED; autofix 0 / 0; dylib + 24 crates building (-j 4, on power).
2026-10-03 evening: WAVE 8 COMPILES (dylib + 24 crates, 21 apps) after EchoCanceller -> audio (94c741e71) and the AzMeet Send newtype (5a4ce677b). MAIL CORPUS: 20 mails, 944 boxes, 0 MISMATCHED (wave-8 base: 531 of 923; first measure 874) - target/refci/mail-wave8/.
2026-10-03: LAYOUTPERF8B merged (wt/layoutperf8b): the reconcile now finds inline content after a block (it was rebuilt fresh every layout - 607 of 618 misses), VirtualView passes keep the host's font chains (2.2 ms/tick). Rebuilding.
2026-10-03 19:10: wave 8 + LAYOUTPERF8B BUILT (all apps compile) and pushed. Re-measured the knob tick: UNCHANGED (288 re-flows, 618 misses, 19 ms root pass) - the 8B fix does not hit AzWidgets' real cause; LAYOUTPERF8 resumed on wt/layoutperf8c against the real build.
