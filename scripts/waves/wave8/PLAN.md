# Wave 8 - the user's topics of 2026-10-03 afternoon (base 0836ecb96: wave 7 integrated, every app compiles)

User: "we definitely still need to churn the mail reftests -> plus research the shm copy for wayland thingy, whether
we have that implemented?"; "animations don't work at all anymore, i.e. the AzWidgets toggle? or the hover animation
over buttons, it immediately transitions"; "start subagents for all these topics" (the topics: Chrome table parity /
the mail corpus, the WPT reftest sweep, the ABI guard, widget threads that never stop, animations that never settle,
the lenient XML parser, the video leftovers). Details of the backlog items: the ledger's "Engine backlog" (copied
into each brief below).

Rules: scripts/waves/house_rules.md (wave-7 rules apply; your brief is the section below; report
scripts/<TASK>_<YYYY_MM_DD>.md, progress scripts/<TASK>.PROGRESS.md). The prebuilt binaries in target/release and
target/azul-lib are THIS base, compiled - run them (capped, one at a time) to see today's behaviour.

| Task | Owns |
|---|---|
| ANIM8 | the animation / transition engine: core/src/gpu.rs, core animation + transition code, layout/src/window.rs and layout managers' animation parts, the dll's animation driver; widget transition CSS only if the bug is there |
| MAILREF8 | layout/src/solver3 table / sizing / formatting-context code (NOT page_breaks.rs, NOT display_list.rs), text3 only for a mail-corpus root cause, scripts/refci/* |
| WPT8 | layout/src/solver3/display_list.rs painting, css parsing of the REFCI items, doc/src/reftest/* (the budget), tests/wpt expectations |
| WAYLAND8 | dll/src/desktop/shell2/linux/wayland/* (and the shared CPU present path if needed) |
| ABI8 | doc/src/codegen/* (abi_guard.rs), the dll's exported ABI hash, the generated bindings' load check |
| THREADS8 | thread ownership of widgets: layout thread management (run_all_threads / thread maps), the dll's thread reaping, the video / camera / screencap workers' lifetime |
| XML8 | the XML / XHTML parser (find it: core/src/xml*, the xmlparser crate we own), the paste parser's use of it |
| VIDEO8 | dll/src/desktop/extra/video_codec/*, stream.rs, the NV12 / colour paths |

## ANIM8 - animations stopped animating (REGRESSION, top priority)
The user (2026-10-03, on the current builds): "animations don't work at all anymore, i.e. the AzWidgets toggle? or the
hover animation over buttons, it immediately transitions". CSS transitions (hover colour fades, the switch toggle's
slide) jump to the end value. Find the root cause and fix it, RED test first (a headless test that changes a hovered
/ toggled state and asserts an intermediate value one frame later). Suspects (wave 6 / 7 touched the animation path -
read each diff): 53f1b748b "the animation channel releases every value no animation drives" (PAINT7) and its RED
60e1f3a0d; fea2f1dd1 "publish each slide relative to the sliding frame" (PAINT7); a3f14f4fb (hit-test transform
lookup); HEADLESS6's scripted animation clock (`scripted_animation_clock`, `debug_server::scripted_run_owns_the_clock`
- only under AZ_E2E, but check the real-time driver is still armed otherwise) and its "E" fix (window context set
before the style comparison - a transition needs the OLD value); WIDGETS7's prop_cache.rs change (placeholder
cascade); LAYOUT7's prop_cache / zoom changes. Use the prebuilt AzWidgets headless (AZ_DEBUG, hover a button, toggle
a switch, sample screenshots / node styles over several frames) to SEE it, then `git log -L` / diffs to find the
commit. Also, from the backlog (same area): "Idle leftovers: FLIP springs never settle (476 active in AzWidgets at
rest -> never FrameDamage::None); AzReview's per-frame RenderImageCallback ImageRefs keep it non-idle" - an idle app
must stop redrawing.

## MAILREF8 - churn the mail reftests (Chrome table parity)
The mail corpus measure `scripts/refci/mail_boxes.py` compares azul's boxes with Chrome's for real HTML mails; the
last measure was 625 mismatched boxes (it started at 874). MAILENG6 / TABLES / LAYOUT7 fixed much since. 1) Re-measure
on the prebuilt binaries (read the script for how it runs; Chrome is installed). 2) Group the mismatches by root
cause, largest first; fix each in the engine with a RED test (a minimal HTML snippet with Chrome's numbers - LAYOUT7
used a probe that lays the same snippet out in Chrome and in a prebuilt app; reuse it). 3) Re-measure after each
group is written down (the numbers only move after the parent's build - record the expected effect). Report the
before number, the groups, what was fixed. Tables first (CSS 2.1 s17 / CSS Tables 3), then whatever is biggest.

## WPT8 - the WPT reftest sweep (REFCI: 66 of 210 passing)
From the backlog: "body background not propagated to the canvas; inline boxes paint no margin/border/background and
inline-block breaks its line; counters / `inline list-item`; single-stop gradients; background-clip; border-width
keywords; box-shadow. Reftest budget: per-page fuzzy budget in doc/src/reftest/pipeline.rs (0.5% is too loose for
100x100 WPT subjects); baseline gains only from a CI run." Work through them in that order, RED test first per item
(layout/tests with the WPT subject's expected boxes / pixels). Coordinate with MAILREF8: it owns layout sizing /
tables; you own painting and the listed CSS features. Do not run the full WPT suite (it needs a build); list the
reftests each fix should flip.

## WAYLAND8 - research: does our Wayland backend do the "faster wl_shm" KDE did?
User: https://www.phoronix.com/news/KDE-Plasma-Faster-WL-SHM - "research the shm copy for wayland thingy, whether we
have that implemented?". 1) Read the article (and the KWin merge request / blog it links) and write down exactly what
KDE changed (which copy was removed, which protocol / buffer path). 2) Read our Wayland backend
(dll/src/desktop/shell2/linux/wayland, the CPU / software present path, wl_shm pool + buffer handling, damage,
buffer reuse / double buffering, any memcpy of the frame). 3) Report: what we do today, per frame, in bytes copied and
syscalls; whether the KDE improvement applies to a CLIENT like us (the article may be compositor-side - then say
what the client-side equivalent is: e.g. render straight into the shm buffer, damage-only uploads, keep a buffer pool
instead of re-creating, wl_surface.damage_buffer); and a plan. 4) If there is a clear client-side win, implement it
with a unit test of the buffer / damage logic (no Wayland on this Mac: keep the change testable without a
compositor and list what needs a Linux run). Network may be flaky - if the page does not load, try again later and
work on step 2 meanwhile.

## ABI8 - the ABI guard for stale apps
From the backlog: "the dylib exports an ABI hash (of api.json) and the generated bindings check it at load - stale
apps locked up / misread structs on 2026-09-30 (likely the 17.7 GB AzWidgets)." doc/src/codegen/v2/abi_guard.rs
exists - read it first: what is done, what is missing (the export in the dll, the check in each language binding's
loader, the error message). Finish it for the Rust bindings (link-dynamic: the apps) first, then C / C++ / Python;
a test that a mismatched hash is refused with a clear message.

## THREADS8 - widget threads that never stop
From the backlog: "Threads of widgets REMOVED from the DOM are never terminated (video/camera/screencap workers;
run_all_threads reaps only finished ones) -> terminate on unmount." Find how a widget owns a Thread (VirtualView
widgets, video / camera / screen capture), what happens when its node leaves the DOM (DOM diff / remap_node_ids -
HEADLESS6 cleared other managers' state there in wave 6), and make an unmounted widget's threads get a stop message
and be joined / reaped. RED test: a DOM that drops a widget owning a thread -> the thread is told to stop and gone
after the next frame.

## XML8 - the lenient XML parser
From the backlog: "a very lenient XHTML mode in OUR xmlparser crate (user: 'we own all the crates'): unquoted/boolean
attributes, uppercase tags, <BR>, '--' in comments, '<' in attribute values, control chars, unclosed-at-end, implied
end tags for p/li/td/tr/option, HTML5 void elements. Alternative (rejected for core): html5->xhtml in AzMail." Also
"Editing leftovers: the paste parser is the strict XML loader". Find where the parser lives (core/src/xml*, a
vendored or path dependency `xmlparser`), add the lenient mode with one RED test per rule, and make the HTML paste
path and the mail renderer use it. Coordinate: HYGIENE (wave 5) made one encoder / decoder - do not add another.
USER (2026-10-03): "the xml parser has to be very lenient, almost html5-like + also accept mail. Probably needs a
refactor so all the special-cases don't grow out of control." So: FIRST read what exists - core/src/xml_html.rs is
already "HTML as a browser reads it (the lenient loader), and the ONE tree construction every XML loader shares", plus
core/src/xml.rs, xml_entities.rs, xml_attributes.rs, and AzMail's own mail-HTML handling (MAIL6 report). Then
REFACTOR toward the HTML5 parsing model instead of piling on cases: a tokenizer with explicit states (data, tag open,
attribute name / value unquoted / quoted, comment incl. bogus comments, doctype, raw text for script/style, RCDATA for
title/textarea) and a tree builder whose element rules are DATA, not code - one table per rule family: void elements,
implied end tags (p closes on block starts, li / dt / dd / tr / td / th / option / optgroup siblings), formatting
elements with a simplified adoption agency (mis-nested <b><i></b></i>), table foster parenting (text / stray elements
inside <table>), auto-inserted html / head / body / tbody, case folding, attribute dedup (first wins), legacy
presentational attributes kept. Every special case is a table row with a comment citing the HTML Living Standard
section. Strict XML documents (the app's own .azul / XHTML) keep parsing exactly as today (a mode flag, or the
tokenizer's XML mode). Tests: one RED test per rule, plus real mails: take the mail corpus files scripts/refci uses
(and the AzMail sample mails) and assert they parse into the same tree Chrome builds (element nesting / counts - a
python probe with Chrome's DOM is fine to produce the expected trees). Keep the public API (`Xml::create_from_html`
& co.) stable; list any api.json change.

## VIDEO8 - the video leftovers
From the backlog (VIDEO_REVIEW report): "stream.rs never applies VideoConfig::output_format (double swizzle per <video>
frame); colour-matrix detection by pointer equality (CFEqual); encoder hard-wired '420v' + Rec.709 tags; decoder output
switch only on IDR; cut_frame mem::take realloc; encode/decode still on the UI thread; zero-copy IOSurface (design in
VIDEO_PATH report); JPEG fallback off Apple; Opus, echo cancellation, bitrate adaptation." Read scripts/VIDEO_REVIEW*
and VIDEO_PATH* first. Order: the correctness items (output_format, CFEqual, the hard-wired tags, the IDR switch),
then encode/decode off the UI thread, then cut_frame, then the larger items as far as you get. RED test per fix where
the code is testable without a camera.
