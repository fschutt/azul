# NEWS9 - AzNews progress (branch wt/news9, base e537ddbe2)

Brief: scripts/waves/wave9/PLAN.md "NEWS9"; planning ../azul-apps/planning/core/news-reader.md;
rules scripts/waves/house_rules.md ("Apps"). Never compile; never spawn subagents.

## DONE
- a67d3ee9e skeleton (Cargo.toml, main/lib/ids), registered: root Cargo.toml, workspace_test_members,
  rust.yml dll_tests step (runs `python3 examples/azul-news/scripts/test_feed_server.py` - MUST EXIST)
- 646bdce18 RED / 46b7ae492 GREEN xmltree.rs (lenient XML on quick-xml 0.41)
- 3d770602a RED dates.rs (GREEN written, not yet committed at the time of this note -> see git log)
- 62b3c6a6b RED feed.rs + links.rs + reader.rs (plain_text / excerpt) + tests/fixtures/*

## IN PROGRESS
- GREEN: dates.rs (commit), links.rs, reader::plain_text / excerpt, feed.rs parse.

## NEXT
- opml.rs (RED: round trip, nested folders, malformed OPML), state.rs (read / starred / later),
  library.rs (views, counts, day groups via azul_pim DateGroup - move it from AzMail), store.rs,
  fetch.rs (conditional GET via azul_storage Transport, discovery), reader view (article Xml +
  images via scan_external_resources), sample.rs, jobs.rs, ui.rs, E2E + feed_server.py + its test.

## Type-check trick (no cargo)
- Pure modules: `rustc --edition 2021 --crate-type lib --test --emit=metadata` on a harness in
  /tmp/news9_check that #[path]-includes the module, with `--extern` the prebuilt rlibs in
  /Users/fschutt/Development/azul/target/release/deps (quick_xml-b155ebc1b94015ba,
  encoding_rs-6519d963c5c6c912, chrono-111531f69b7755d2, url-*, serde*). No azul rlib exists:
  azul-dependent code is stubbed in the harness (e.g. `mod reader { pub fn plain_text }`).

## Decisions
- Feed parsing: own parser on quick-xml 0.41 (already in Cargo.lock via docx-parser / ooxml-common /
  plist) in lenient mode (allow_dangling_amp, allow_unmatched_ends, check_end_names off) +
  serde_json for JSON Feed; NOT feed-rs (not in Cargo.lock; pulls its own quick-xml, mediatype,
  regex, uuid, siphasher). roxmltree is strict (a bare `&` or a truncated feed fails the whole feed).
  Charset via encoding_rs 0.8 (in Cargo.lock via mail-parser / lopdf). url 2.5 for links.
- HTTP: azul-storage's one Transport seam (`HttpCall` / `HttpReply`, `AzulTransport` = azul's
  HttpRequestConfig from an azul Thread) - no second HTTP client; tests use a fake Transport.
- Dates: own lenient reader; chrono only adds up the parts (its rfc2822 refuses wrong weekdays).
- Titles / excerpts: plain text through azul's HTML parser (reader::plain_text) - the engine's one
  entity table; item ids without guid/link: azul_storage::sigv4::sha256_hex (stable).
- Shell: S4 PimShell (.office_shell()), like AzContacts; reading pane = azul's ReadingPane widget.
- DateGroup twins: azul-mail listing.rs DateGroup, azul-notes model, azul-tasks list -> plan:
  move AzMail's into azul-pim/dates.rs, AzMail re-exports it (minimal edit), AzNews uses it.

## Open questions
- (none yet)
