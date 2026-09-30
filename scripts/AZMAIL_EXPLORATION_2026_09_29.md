# AZMAIL_EXPLORATION: final report

Branch `wt/x-mail-explore`, base `34a8fe46f`. Exploration only. The deliverable is
`scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md`. Nothing was compiled.

## What was done

- **Q1 mail rendering:** 8 representative mail HTML samples, each run 3 ways (raw, HTML5-
  normalized, legacy-rewritten), through the release dylib, headless.
  - Harness: `AzWidgets` + `AZ_E2E` + the `mount` op; nothing built.
  - Parse verdicts: 3 of 8 raw samples fail to parse. All 8 parse once normalized.
  - Rendering: the legacy rewrite fixes replies. Table layouts still break, and the receipt
    **crashes the CPU renderer**.
  - Two findings: form elements render as live widgets, and there are gaps in the UA defaults
    and in CSS parsing.
- **Q2 crates:** facts from the crates.io API (version, date, license, deps) and checks against
  `deny.toml` and the 14-day cooldown. Recommended: mail-parser, mail-builder, ammonia and
  rusty-s3, plus lettre only for SMTP submission.
- **Q3 mailbox:** the Email Worker, the S3 layout (ULID keys, sidecar JSON, spam folder, flags
  file with `If-Match`) and auth: device key in the keyring, then prefix-scoped R2 temporary
  credentials, then SigV4 in the app. Access links are a DB row that turns into scoped
  credentials. Worker endpoint list and Python mock plan included.
- **Q4 sending:** micromail, built without `tls`, sends plaintext after STARTTLS.
  - Proven with a Python replay against a real TLS server, and pinned with 4 RED tests
    (STARTTLS, MIME-Version, dot-stuffing, bare LF).
  - Measured: MX hosts listen only on 25.
  - Verdict: crash reports go over HTTPS to the `crash` Worker; AzMail sends through a Worker
    with the Email Service binding, or through SMTP submission for external accounts.
- **Q5 editor:** a flat block model with `quote_depth` (the AzWriter IR pattern), because the
  engine's Enter and Backspace act on the host's direct child. Serializers live in the app. The
  engine gaps are listed with file:line.

## Commits

| Commit | What |
|---|---|
| `100923856` | progress checkpoint |
| `a40976f60` | RED `layout/tests/a_full_width_rule_in_a_spanning_table_cell_renders.rs` + `all.rs` |
| `42d4a0885` | progress |
| `c818942d2` | RED `layout/src/telemetry/crash_mail.rs` `smtp_sink_tests` (4 tests) |
| `a001cde2a` | RED `layout/tests/a_linear_gradient_puts_its_colours_where_css_says.rs` + `all.rs` |
| `e3e57aeeb` | progress |
| `d7ebffbd1` | `scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md` |
| (this commit) | this report + progress |

## api.json changes

None.

## Least sure to compile

- `crash_mail.rs` `smtp_sink_tests`: std-only (`TcpListener`, `BufReader::read_line`,
  `Read::read` on the `BufReader`). `drop(result)` follows the file's own style. The tests are
  gated by `feature = "crash-mail"` through the module.
- `a_full_width_rule_in_a_spanning_table_cell_renders.rs`:
  - Uses `azul_layout::xml::parse_xml` + `dom_from_parsed_xml` (feature `xml`, which the
    default `cpurender` implies).
  - Destructures `DisplayListItem::Text { glyphs, clip_rect, .. }` (`clip_rect.0` is a pub
    `LogicalRect`).
  - The panic was reproduced on the dylib through the core XML loader plus the headless
    compositor. The test uses the same loader plus `cpurender::render_with_font_manager`. If
    that path does not panic, the first test (finite geometry) may still fail or may pass;
    report back and I will narrow it.
- `a_linear_gradient_puts_its_colours_where_css_says.rs`: pixel reads at row 10 of a 100x20 box
  at the origin (body `margin: 0`), in the same shape as `a_box_shadow_paints_once.rs`.

## Test commands for the parent

```
cargo test --release -p azul-layout --test all a_full_width_rule_in_a_spanning_table_cell_renders
cargo test --release -p azul-layout --test all a_linear_gradient_puts_its_colours_where_css_says
cargo test --release -p azul-layout --features crash-mail --lib telemetry::crash_mail::smtp_sink_tests
```

All three are expected RED. The `crash_mail` tests use `127.0.0.1` ephemeral ports.

## Other agents' files touched

`layout/tests/all.rs`: 2 `#[path]` + `mod` pairs appended at the end. Nothing else outside the
task's own files.

## What is left

The phased M-task plan is in the ideas doc, section 7:
- Phase 0: fix what this exploration proved (crash over HTTPS, micromail 0.1.1, the table crash,
  gradients, XML/UA items, inline hit areas, `color-scheme`).
- Phase 1: azmail-core (parse, plain, sanitize, RichTextView).
- Phase 2: the mail/identity/share Workers and app sync.
- Phase 3: compose.
- Phase 4: external accounts.

Open questions for the user are in section 9 of the ideas doc.

Observations not pinned by a RED test (noted in the ideas doc):
- Headless screenshots paint text in the system mode's colour on a fixed white canvas.
- A scenario that shrinks the window below the app's size draws its first text line displaced;
  not root-caused.

Probe scripts, samples and screenshots are in the session scratchpad (`xmail/`), not committed:
- `mail_html_probe.py`, `run_e2e.py`, `try_markup.py`
- `smtp_probe.py`, `crates_probe.py`, `chrome_ref.py` (Chrome could not start in the sandbox)
