# Wave 9 resume table

Resume: SendMessage the agent id ("continue from your progress file"). Worktrees: .claude/worktrees/agent-<id>.

| Task | Agent id | Branch | State |
|---|---|---|---|
| WIDGETS9A | a18aaef546b141fde | wt/widgets9a | DONE (report scripts/WIDGETS9A_2026_10_03.md with the api.json entries; Toolbar (overflow menu, one Tab stop), TokenInput (app-owned state like DataTable), IconGrid (row-scrolled, data callback, rubber band, drag out); engine gap: :focus-within parsed but never set) |
| WIDGETS9B | acb99bc6296facd68 | wt/widgets9b | DONE (765ae4548; report scripts/WIDGETS9B_2026_10_03.md with the api.json entries - incl. CHANGED ComboBoxStateWrapper (item_details, open_on_type) + ComboBox.status; MoneyInput (minor units), Gauge (Chart vector path), DateRangePicker (DatePicker day_grid shared), ReferencePicker (on ComboBox); ComboBox two-clicks-to-close fixed) |
| MAIL9 | a4d36f4ef063912bd | wt/mail9 | PAUSED - resume from scripts/MAIL9.PROGRESS.md (report scripts/MAIL9_2026_10_03.md: direct delivery + client DKIM DONE end to end; submission via lettre started; remote-content pre-pass NOT started) |
| PDF9 | a90ecd8756e4fac61 | wt/pdf9 | DONE (19 commits; report scripts/PDF9_2026_10_03.md; ParsedPdf -> page SVG in the API (api list in report), CPU SVG renderer: rgb() colours, transform order / lists, fit honoured; AzPdf viewer + export; scripts/pdf_chrome_probe.py; NEXT: azul SVG renderer lacks <text> / <image> (PDF pages show shapes only) - plan in PDF9.PROGRESS.md) |
| READER9 | ad5e2a07b9e50eb78 | wt/reader9 | running (base e537ddbe2) |
| TERM9 | aeeb57dd4eb500ff0 | wt/term9 | DONE (report scripts/TERM9_2026_10_03.md with the api.json entries; TerminalView widget (VirtualView, xterm encoding, theme palettes) + AzTerm on alacritty_terminal 0.26 (engine + PTY; new crates listed for vet); engine: a focused node with a Paste callback now gets the paste) |
| CODE9 | aec89a04e113059d5 | wt/code9 | running (base e537ddbe2) |
| MEDIA9 | a78d3666fb280f6a4 | wt/media9 | running (base e537ddbe2) |
| CLOCK9 | a77d97682c470b269 | wt/clock9 | DONE (4dc00fe0f; report scripts/CLOCK9_2026_10_03.md; scheduled notifications in the engine (Notification.deliver_at: Apple UN trigger, Windows ScheduledToast, held + woken elsewhere) - api.json: field deliver_at LAST + with_deliver_at; AzClock (DST-correct RRULE alarms via chrono-tz, timers, stopwatch, world clock)) |
| KEYS9 | ad62e1cc06f76a821 | wt/keys9 | DONE (34 commits; report scripts/KEYS9_2026_10_03.md; AzKeys ~10k lines / ~80 tests; Argon2id + XChaCha20-Poly1305 (argon2 0.5.3 + blake2 0.10.6 need vet exemptions; argon2 API written from docs); no api.json) |
| MONITOR9 | a21e54e23b6931c52 | wt/monitor9 | DONE (report scripts/MONITOR9_2026_10_03.md; AzMonitor ~4500 lines, 78 tests; sysinfo 0.38 (ntapi 0.4 Windows needs a vet exemption); 1 Hz via VirtualViews, no layout(); RISK: keyboard events into a focused node inside a VirtualView unchecked; no api.json) |
| NEWS9 | af8b08e8d81607db5 | wt/news9 | running (base e537ddbe2) |
| ERP9 | a7949c63ed0ed81a4 | wt/erp9 | running (base e537ddbe2) |
