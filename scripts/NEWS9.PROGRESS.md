# NEWS9 - AzNews progress (branch wt/news9, base e537ddbe2)

Brief: scripts/waves/wave9/PLAN.md "NEWS9"; planning ../azul-apps/planning/core/news-reader.md;
rules scripts/waves/house_rules.md ("Apps"). Never compile; never spawn subagents.

## DONE
- (start) branch wt/news9 created at e537ddbe2; read house rules, PLAN, news-reader.md.

## IN PROGRESS
- reading the APIs the app needs (appkit, PimShell, ReadingPane, azul-storage Transport, quick-xml 0.41).

## NEXT
- Cargo.toml + skeleton (lib.rs / main.rs / ids.rs), register in workspace / test members / CI.
- RED tests: feed parsing (RSS 2.0, RSS 1.0, Atom, JSON Feed, malformed real feeds), OPML round trip,
  read state; then GREEN.

## Decisions
- Feed parsing: own parser on quick-xml 0.41 (already in Cargo.lock via docx-parser / ooxml-common /
  plist) in lenient mode (allow_dangling_amp, allow_unmatched_ends, check_end_names off) +
  serde_json for JSON Feed; NOT feed-rs (not in Cargo.lock; pulls its own quick-xml, mediatype,
  regex, uuid, siphasher). roxmltree is strict (a bare `&` or a truncated feed fails the whole feed).
  Charset via encoding_rs 0.8 (in Cargo.lock via mail-parser / lopdf).
- HTTP: azul-storage's one Transport seam (`HttpCall` / `HttpReply`, `AzulTransport` = azul's
  HttpRequestConfig from an azul Thread) - no second HTTP client; tests use a fake Transport.
- Dates: chrono (in tree) rfc2822 / rfc3339 plus lenient fallbacks.
- Shell: S4 PimShell (.office_shell()), like AzContacts; reading pane = azul's ReadingPane widget.

## Open questions
- (none yet)
