# Wave 9 - the next apps, the widgets they need, PDF -> SVG, authenticated mail sending (2026-10-03 evening)

User (2026-10-03): shared widgets "make an agent to create all of these, with the apis so we can build the next
apps"; "AzPdf: we need to expose the PDF -> SVG capability so we can later go SVG -> DOM and display previews of the
page. Doesn't have to be perfect, we'll reference test them with Chrome again"; "AzClock, AzKeys, AzMonitor, AzNews,
ERP asset management ... media (AzMusic, AzPlayer), documents (AzPDF, e-reader), developer tools (AzTerm, AzCode) ->
start subagents to build all of them, in the same way as before: there are vt terminal handling engines already and we
can use the VirtualView to virtualize the scrollback. Same for the code (for massive files), it's actually easier than
AzWriter since we don't have to do pagination and we already have the file tree"; mail sending: "leave micromail alone
if possible: are there crates that handle this?"; remote images: "do a pre-pass ... get images and fonts list
mentioned so we can fetch them separately (after the xml parse but before layout)".
Planning source: ../azul-apps/planning (core/*.md per app, foundation/05-widget-backlog.md), summary in
scripts/ideas/AZUL_APPS_PLAN_STATUS_2026_10_03.md. Rules: scripts/waves/house_rules.md (its "Apps" section is how an app
is built and registered). Base: the commit your prompt names (wave 8 integrated; every app compiles; target/release +
target/azul-lib are that build, freshly linked).

## Notes for every wave-9 agent
- Running prebuilt apps: put env vars BEFORE the runner (`AZ_BACKEND=headless AZ_DEBUG=<port> scripts/waves/tools/
  run_capped.sh --cap-mb 1500 --seconds 120 --log <f> -- target/release/<App>`) - macOS SIP strips DYLD_* through
  /usr/bin/env. `wait_settled` before every screenshot (buttons now fade 120 ms).
- Building blocks since wave 6 (in api.json, use them): DataTable (virtualized, sort / filter / edit), Chart (line /
  bar / scatter / pie), DateRepeatPicker, RichTextEditor + RichTextDoc, CellGrid, Timeline, the 11 shells
  (OfficeShell, RecordsShell, DocumentShell, browser shell ...) + ShellThemeScope::body(), CloseGuard
  (with_dirty_check), ListSelection, TextInput::set_text_in, the standard dialogs, azul-appkit (args, data root,
  settings, shortcuts, about, find, files::read_outside), azul-storage (Drive, LocalDrive, S3Drive, ids::new_uuid),
  azul-pim, VirtualView, the HTML5-like parser (Xml::create_from_html), Xml::scan_external_resources (every img /
  background / font / stylesheet URL after the parse), TextRasterStyle / RawImage::from_text, VideoEncoder /
  VideoDecoder, AudioSink, AudioEncoder / AudioDecoder (Opus, Apple), EchoCanceller, OS notifications, global hotkeys,
  the keyring.
- NEW WIDGETS being built in parallel (WIDGETS9A: IconGrid, Toolbar, TokenInput; WIDGETS9B: Gauge, DateRangePicker,
  MoneyInput, ReferencePicker) are NOT in your base: do not wait - use existing widgets now, leave a
  `TODO(WIDGETS9x): <widget>` where the new one should replace it, list it in your report.
- New crates: prefer what is already in Cargo.lock; a new crate goes in YOUR crate's Cargo.toml only (not azul core
  unless your brief says engine), with version + why in your report (the parent handles cargo-vet exemptions).
  Never add a `[patch]`.
- Apps live in examples/azul-<name> (house rules "Apps"): prefix `__az<name>_` const AzString ids, data in the data
  tree through the Drive, appkit, both themes x modes, CloseGuard where there is a document, an E2E script.

| Task | Builds |
|---|---|
| WIDGETS9A | IconGrid, Toolbar, TokenInput (layout/src/widgets/<new>.rs + theme APPENDS + tests + manifest + api list) |
| WIDGETS9B | Gauge, DateRangePicker, MoneyInput, ReferencePicker (same) |
| MAIL9 | AzMail: authenticated SMTP submission through a crate (micromail untouched), remote images / fonts via the scan pre-pass |
| PDF9 | PDF -> SVG in azul's API (printpdf), AzPdf (viewer: page previews, thumbnails, search, zoom) |
| READER9 | AzReader (e-reader: EPUB through the HTML5 parser, paged, library, bookmarks) |
| TERM9 | AzTerm (PTY + a vt engine crate + TerminalView with VirtualView scrollback) |
| CODE9 | AzCode (CodeView for huge files via VirtualView, syntax highlighting, file tree, tabs, search) |
| MEDIA9 | audio decoding into AudioSink + MediaControls / SeekBar / Waveform / LevelMeter, AzMusic, AzPlayer |
| CLOCK9 | AzClock (alarms, timers, stopwatch, world clock; scheduled alarms) |
| KEYS9 | AzKeys (password manager: encrypted vault, keyring, generator, biometrics) |
| MONITOR9 | AzMonitor (processes, CPU / memory / disk / network on RecordsShell + DataTable + Chart) |
| NEWS9 | AzNews (RSS / Atom / JSON feeds, reader view via the HTML5 parser, images via the scan pre-pass) |
| ERP9 | ERP asset management (../azul-apps/planning/other/erp/README.md "the best first one") |

## WIDGETS9A - IconGrid, Toolbar, TokenInput
Spec: ../azul-apps/planning/foundation/05-widget-backlog.md (IconGrid 22 apps, Toolbar 19, TokenInput 14) and the
TokenInput spec in scripts/PIMDRIVE7_2026_10_03.md s8. IconGrid: a virtualized grid of thumbnails / icons with labels
(file manager, photo manager, e-reader library, app launcher): selection via ListSelection (click / Ctrl / Shift /
rubber band), keyboard navigation, drag out (DragStart data), double-click / Enter activation, context menu event,
async thumbnails (an image per item the app sets later), huge counts via VirtualView or the DataTable / CellGrid
scroll-window pattern. Toolbar: a generic row of tool buttons / toggles / separators / drop-down buttons with overflow
into a "more" menu when narrow (not the ribbon). TokenInput: chips with remove buttons + a text field, suggestions
dropdown, Backspace deletes the last chip, paste of comma lists, validation per token. Each: both themes x modes,
a11y, keyboard, tests, manifest, exact api.json list (create*, callback triples like CloseGuard's).

## WIDGETS9B - Gauge, DateRangePicker, MoneyInput, ReferencePicker
Same spec source. Gauge: radial / linear value display with ranges (ok / warn / bad bands), min / max, label, a11y as
meter. DateRangePicker: two-month view, range selection with hover preview, presets (today, last 7 days, this month
...), week start (DatePickerWeekStart), keyboard. MoneyInput: amount + currency, locale-aware parsing / formatting
(decimal / grouping separators), integer minor units (no floats for money), negative numbers, validation.
ReferencePicker: type-to-filter selection of a record from a large list (a combo box with async results from the app's
callback, keyboard, "create new ..." entry) - check what ComboBox / DropDown already do and extend rather than twin.

## MAIL9 - authenticated sending through a crate; remote images and fonts
Today (examples/azul-mail/src/send.rs, sending.rs): sending exists through micromail (MIME, DKIM, direct MX delivery by
default; the SMTP-server route has "no sign-in yet"). From a home connection direct port-25 delivery mostly fails or
lands in spam. Add AUTHENTICATED SUBMISSION through a maintained crate (evaluate `lettre` - SMTP with STARTTLS /
implicit TLS, AUTH PLAIN / LOGIN / XOAUTH2, its own message builder and DKIM - vs Stalwart's `mail-send` +
`mail-builder` + `mail-auth`; pick one, say why; micromail stays as it is for azul's crash mail and the direct route):
the account's own credentials (the IMAP app password / the OAuth token, from the keyring), provider presets
(smtp.gmail.com 465/587, smtp.office365.com 587, ...), submission becomes the default route for accounts with a preset,
the outbox / retry / Sent filing stay as they are. RED tests for the route choice and the auth mechanism selection; an
E2E against a local test SMTP server (python `aiosmtpd`, as the IMAP test server is python). REMOTE CONTENT: use
`Xml::scan_external_resources` after the parse and before layout to list a mail's images / fonts / stylesheets; behind
the reading pane's "download pictures" info bar (blocked by default - privacy, tracking pixels), fetch them on a Thread
(size and count caps, http(s) only, no cookies), register images as image resources and fonts as font resources,
then lay out; inline `cid:` images from the mail's own parts always shown. Also write in your report exactly how the
user can try a real account (wizard steps, app password links) - the user wants to try it.

## PDF9 - PDF -> SVG in the API, and AzPdf
printpdf (the user's crate, in Cargo.lock at azul-codegen-api #84dce8c) parses PDFs; find its page -> SVG rendering
(or the closest: its parse into page ops) and expose "PDF bytes -> page count, page sizes, page N as SVG text" through
azul's API (api.json entries, a Rust-only helper where needed; the decode on a Thread). Then render the SVG: azul has
SVG parsing / tessellation (SvgMultiPolygon, the svg module) - show a page as an SVG image node first (simplest); SVG ->
DOM is a later step (note what it would need). AzPdf app on the DocumentShell: open a PDF (file dialog, recent files
from the data tree), page thumbnails in a side rail, the page view with zoom (fit width / fit page / %), page
navigation, text search if printpdf exposes the text, print later. A reftest harness against Chrome's PDF rendering is
the user's next step - write the probe that renders page 1 of a few PDFs in Chrome headless and in AzPdf and compares
(like scripts/refci), even if the numbers are bad today.

## READER9 - AzReader, an e-reader
EPUB (zip + OPF + XHTML + CSS) through the HTML5-like parser (Xml::create_from_html), laid out with azul's paged layout
(pages, columns; the pdfocr work showed fragmentation and multi-column work) - reflowable, font size / margins / theme
(light / dark / sepia), a library (grid of covers - TODO(WIDGETS9A) IconGrid), a table of contents, bookmarks,
progress saved in the data tree. Plain text and HTML files too. planning: ../azul-apps/planning/mobile/e-reader.md (or
wherever e-reader is planned).

## TERM9 - AzTerm
A PTY (spawn the user's shell: posix_openpt / forkpty on Unix, ConPTY on Windows - a crate like `portable-pty` is fine)
and a VT engine crate (evaluate `alacritty_terminal`, `vte`, `vt100`, `termwiz`; pick, say why), a TerminalView widget
in azul (a cell grid of styled glyphs, cursor, selection, scrollback virtualized through VirtualView or the
scroll-window pattern so 100k lines stay smooth), keyboard (escape sequences, Ctrl / Alt / Meta, bracketed paste),
mouse reporting, resize -> TIOCSWINSZ, colours (16 / 256 / truecolor) from the theme, tabs, copy / paste.
planning: ../azul-apps/planning/core/terminal.md.

## CODE9 - AzCode
A CodeView widget for massive files (a million lines): lines virtualized (only visible lines are DOM), a rope or piece
table for the text, syntax highlighting (evaluate tree-sitter vs syntect; incremental, off the UI thread for big
files), line numbers gutter, current line, selection / multi-cursor basics, search / replace (appkit `find`), go to line,
the file tree (existing tree_view / AzDrive pieces), tabs, open / save through the Drive or real files
(files::read_outside), dirty state + CloseGuard. planning: ../azul-apps/planning/core/code-editor.md. Note: CodeView
"needs f64 scroll offsets" per the foundation docs - check whether that is still true.

## MEDIA9 - the media engine, AzMusic, AzPlayer
Audio decoding into AudioSink (evaluate `symphonia` - pure Rust MP3 / AAC / FLAC / Vorbis / WAV, and Opus via our
AudioDecoder on Apple), seeking, gapless; widgets MediaControls, SeekBar, Waveform, LevelMeter (move AzMeet's meter into
azul - no twin). AzMusic: library from a folder (tags via the decoder's metadata), albums / artists / songs on a
RecordsShell + DataTable, playlists in the data tree, now-playing with the OS media keys / NowPlayingInfo (exist).
AzPlayer: video files - container demux (MP4 / MOV via symphonia's isomp4 or the `mp4` crate; H.264 decode exists via
VideoDecoder), A/V sync, controls, fullscreen. planning: core/music.md, core/video-player.md.

## CLOCK9 - AzClock
Alarms (scheduled - they must ring when the app is in the background; the OS notification scheduling paths - engine
backlog item 14: macOS UNNotification scheduled triggers, Windows toast scheduling, Linux: the app stays resident),
timers, stopwatch with laps, world clock (time zones - a tz crate), repeat rules via DateRepeatPicker. Data in the data
tree. planning: core/clock.md.

## KEYS9 - AzKeys
A password manager: an encrypted vault file in the data tree (evaluate `age` or argon2id + XChaCha20-Poly1305 via
RustCrypto - say why), master password + OS keyring / biometrics unlock (exist), entries with fields / TOTP (RFC 6238)
/ notes / tags, a generator, search, copy with clipboard auto-clear, import (CSV from browsers / Bitwarden JSON). PGP is
a later step (note it). planning: core/password-manager.md.

## MONITOR9 - AzMonitor
Processes (the `sysinfo` crate), CPU / memory / disk / network over time on RecordsShell + DataTable (sortable process
list, end process with a confirm dialog) + Chart (history), per-core usage (TODO(WIDGETS9B) Gauge). Updates on a Thread
at 1 Hz without relaying out the page (use the animation / patch paths; a good test of ANIMFRAME8's work).
planning: core/system-monitor.md.

## NEWS9 - AzNews
Feeds (RSS 2.0 / Atom / JSON Feed - evaluate `feed-rs` vs parsing with azul's XML parser), subscriptions + OPML import /
export, refresh on a Thread with ETag / Last-Modified, a three-pane reader (feeds, items, article) on the OfficeShell /
browser shell, the article via the HTML5-like parser with a reader-mode stylesheet, images via
Xml::scan_external_resources (fetched on a Thread), read / starred state in the data tree. planning: core/news-reader.md.

## ERP9 - ERP asset management
../azul-apps/planning/other/erp/README.md calls asset management "the best first one" - read the ERP README (its
view-JSON interpreter idea) and the asset-management plan; build it on RecordsShell + DataTable (+ TODO(WIDGETS9B)
MoneyInput / ReferencePicker / DateRangePicker): assets, categories, locations, depreciation schedules (straight-line,
declining balance), maintenance log, check-out / check-in, CSV import / export, reports (Chart). Records as files in the
data tree (one JSON per record, per the S3 split), not a database.
