# ../azul-apps planning vs what azul has built (research, 2026-10-03)

User: "research in ../azul-apps, we planned way more than just 20 apps - we just did the 'base' for the most important
scaffold, whats next?" Source: /Users/fschutt/Development/azul-apps/planning/ (README.md, build-ledger.md,
cloud-*.md, core/, advanced/, education/, engines/, foundation/, mobile/, other/), compared with azul/examples and
azul/layout/src/widgets. Read-only research by a subagent; numbers as it counted them.

## Totals
77 planned apps (planning/README.md): 13 BUILT, 7 PARTIAL, 57 NOT STARTED. The build ledger's step 4, the "cloud
spine" (C5-C10), is not started, nor packaging (F3-F5, F8). So every app after step 4 exists as a UI, but nothing
syncs between devices, ships, or sends mail yet.

## Status by category
- core (24): BUILT word (AzWriter), excel (AzSheets), powerpoint (AzShow), photoshop (AzPhoto), video-editor
  (AzVideoCut), mail (AzMail - receive only), contacts / calendar / notes / todo / calculator, meet (AzMeet + the
  meet Worker), file-manager (AzDrive - browse local + S3; no transfers / sharing / FTP). PARTIAL system-monitor
  (AzDashboard pieces, no process data), chat (only AzMeet's in-call chat), video-player (VideoWidget, no player).
  NOT STARTED illustrator, clock, password-manager (+PGP), news-reader, pdf, music, code-editor, terminal.
- advanced (8): PARTIAL infinite-canvas (AzPaint raster only). NOT STARTED clipboard history, screen recorder, font
  manager, archive manager, QR/camera, mobile sync, WhatsApp (engine pieces exist for recorder / QR / archive).
- mobile (16): PARTIAL gis (AzMaps tile demo), filezilla (AzDrive S3), canvas-blackboard (if whiteboard = AzPaint).
  NOT STARTED camera, photo manager, document scanner, SMS, offline encyclopedia, sticky notes, chess, MIDI editor,
  voice memo, 2FA, weather, e-reader, health.
- other (26): all NOT STARTED - five ERP modules (accounting, invoicing, asset management, time tracking, POS),
  banking, automation, archiving, language learning, math, recipes, personal finance, disk analyzer, translation,
  solitaire, remote desktop, USB writer, printer manager, system logs, emoji picker, colour chooser, hex editor,
  man-page reader, BitTorrent, network analyzer, PGP key manager.
- education (3): all NOT STARTED - missal, plotting/CAS, screen reader. ecosystems/README.md is research notes.
- azul examples not in the plan: AzBuilder, AzDashboard, AzSetup, AzShells, AzWidgets, AzMaps, AzPaint, AzReview.

## Ledger / engines / cloud
- build-ledger.md sequence: Calculator -> Notes -> Calendar -> cloud spine (C5-C10) -> Contacts/Tasks -> Files ->
  Writer -> Sheets -> Slides -> Mail -> Meet. BUILT: all 11 app UIs; foundations F1, F2, F6, F7 (= azul-appkit +
  the shells); C3 + U4 (the meet Worker, iroh in AzMeet). NOT BUILT: F3 packaging/signing, F4 update feed, F5 crash
  reports, F8 CI; C1 releases + C2 crash Workers; C5-C10 identity / sync / blobs / sharing / installer; U1 IronCalc
  upstream patches, U2 micromail fixes, U3 mail deliverability rig; U5 URL-scheme registration (the meet Worker
  hands out azlin://meet/<room> links that need it).
- Engines: IronCalc used (AzSheets); Graphite unused (AzPhoto has its own tiles, AzVideoCut azul's H.264); the iroh
  cull-then-route design is in azul-meet/src/routes.rs; mail sending (engines/mail-transport.md) open.
- Cloud / S3: azul-storage has LocalDrive + S3Drive (own SigV4), the `.azlin/cache` manifest + diff(), ScopedDrive.
  NOT BUILT: the sync loop (upload / download), conditional writes (If-Match - no hit), the access-link table + the
  Worker handing out short-lived R2 credentials (scripts/ideas/STORAGE_BACKENDS_REPORT_2026_09_30.md s5.2), identity
  (cloud-platform-notes.md decisions 14/15: a per-device key in the OS keychain, bootstrapped by a magic link).

## Missing pieces ranked by apps unblocked (core + other; foundation/05-widget-backlog.md, foundation/03 gap table)
IconGrid (thumbnail grid) 22; generic Toolbar 19; TokenInput 14 (spec in scripts/PIMDRIVE7_2026_10_03.md s8);
ReferencePicker (type-to-filter ComboBox; check) 14; media set (MediaControls 9, LevelMeter 8, Waveform 7, SeekBar 6,
plus an audio decoder - the sink is PCM-only) ~12; Gauge 10; DateRangePicker 10; CanvasViewport 9 / InkLayer 7 /
PageView 6 (exist app-local in AzPaint / AzReview / AzWriter - promote); MoneyInput + the ERP view-JSON interpreter 8;
CodeView 7; TerminalView + process spawning / PTY 4 (no PTY code); printing 6 (no API); file-system watch 5; URL
schemes 5; colour emoji 5; PDF page rendering 3 (azul only writes PDF via printpdf); NodeGraph edges 4
(draw_connection returns a null image). The HTML5 parser is wave-8 XML8; OS notifications + global hotkeys exist.

## Recommended next wave (dependency order)
1. Update the planning docs (cloud-workers.md for the storage-split ruling; refresh foundation/01 + /05).
2. Shipping basics: F3 packaging / signing, C1 releases + C2 crash Workers, U5 URL-scheme registration.
3. S3 sync in azul-storage (C6/C8): conditional writes, upload / download from diff(), a conflict policy - makes all
   13 built apps multi-device.
4. Identity + access-link Worker (C5/C9): device keys + R2 temporary credentials - Drive sharing, Meet recordings.
5. Mail sending (U2, U3, A10): micromail fixes, DKIM in the client, relay fallback.
6. Widget batch in azul: IconGrid, TokenInput, Gauge, DateRangePicker / MultiSelect, MoneyInput; move KanbanBoard /
   TimeGrid out of AzTasks / AzCalendar into azul.
7. Media engine: audio decoding into the sink + MediaControls / SeekBar / Waveform / LevelMeter -> AzMusic, AzPlayer,
   voice memo, transcription.
8. Document engine: a PDF rasteriser (hayro) + PageView / InkLayer / CanvasViewport into azul -> AzPDF, e-reader,
   document scanner.
9. Developer engine: process spawning + PTY, a terminal state machine, TerminalView, CodeView (needs f64 scroll
   offsets) -> AzTerm, AzCode, a real AzMonitor.
10. Cheap apps on what exists: AzClock (alarms = engine backlog 14), AzKeys (keyring + biometrics exist), AzMonitor on
   AzDashboard, AzNews after XML8, ERP asset management ("the best first one", other/erp/README.md).

## Where the planning docs and azul disagree
- Code location: planning/README.md puts apps in azul-apps/apps + shared/azlin-kit; every app lives in azul/examples.
  azul-apps/apps/notes and shared/azlin-kit are dead drafts; only cf-workers/meet is live.
- Cloud: cloud-workers.md keeps note / contact / event bodies in a DB `record` table; the user's 2026-09-30 ruling
  (durable data = files in the user's S3 bucket; DB only minting, transient state, access links) supersedes it - the
  ruling lives in Claude's memory (azlin_cloud_storage_split.md), not in either repo. The meet Worker uses /rooms +
  libSQL, not the ledger's /m/<uuid> or the Durable Objects the platform notes recommend.
- Engines: AzPhoto / AzVideoCut were planned on Graphite; file-manager.md says OpenDAL (azul-storage has its own SigV4).
- Formats: AzWriter saves Markdown (the plan treats .docx fidelity as the core job); AzShow builds slides from plain
  azul DOM (settles decision 9).
- Stale: engines/iroh-routes.md + ledger U4 say AzMeet has no networking (it runs on iroh); foundation/01 + /05 count
  51 widget modules (azul has ~80 incl. the 11 shells, DataTable, CellGrid, Chart, Timeline, RichTextEditor).
- Scope: the ledger says desktop only, no browser clients; core/chat.md plans desktop + phone + web.
- Names: AzFiles = AzDrive, AzSlides = AzShow, AzCut = AzVideoCut.
