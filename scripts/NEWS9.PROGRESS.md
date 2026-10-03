# NEWS9 - AzNews progress (branch wt/news9, base e537ddbe2)

Brief: scripts/waves/wave9/PLAN.md "NEWS9"; planning ../azul-apps/planning/core/news-reader.md;
rules scripts/waves/house_rules.md ("Apps"). Never compile; never spawn subagents.
On resume: read this file, `git -C <worktree> status`, `git log --oneline -12`, continue at NEXT.

## DONE (examples/azul-news)
- a67d3ee9e skeleton (Cargo.toml, .cargo/config.toml, main/lib/ids), registered: root Cargo.toml,
  scripts/workspace_test_members.txt, rust.yml dll_tests step. That step runs
  `python3 examples/azul-news/scripts/test_feed_server.py` - THIS FILE MUST STILL BE WRITTEN.
- 646bdce18 RED / 46b7ae492 GREEN src/xmltree.rs (lenient XML tree on quick-xml 0.41, decode()).
- 3d770602a RED / 17f745cd0 GREEN src/dates.rs (parse_date, lenient RFC 822 / 3339 / US).
- 62b3c6a6b RED / 627b4e9a2 GREEN src/feed.rs (RSS / Atom / JSON Feed -> Feed / Item),
  src/links.rs (resolve, strip_tracking, site_name, is_web), src/reader.rs (collapse, plain_text,
  excerpt, cut_at_word, text_to_html via Xml::create_from_html / Xml::encode_text),
  tests/fixtures/* (13 fixture feeds incl. malformed ones).
- ed4fe2eec rustfmt + progress.
- 34adfa6e8 RED / f02080a96 GREEN src/opml.rs (Subscription, parse, write; azId) and
  src/state.rs (ReadState: read / starred / later, to_json / from_json, prune).
- feed.rs + links.rs + xmltree.rs + dates.rs + state.rs TYPE-CHECK CLEAN (see "Type-check trick").
  opml.rs / reader.rs use azul (Xml::encode_attribute / encode_text / create_from_html): unchecked.

- db112258c DateGroup + date_group moved azul-mail listing.rs -> azul-pim dates.rs (tests moved
  too; mail re-exports; mail's own tests kept, `use chrono::Weekday` added to them).

- 1b59ae13f RED / d34839e47 GREEN src/library.rs (as designed below; type-check clean with
  /tmp/news9_check/harness4.rs = harness2 + library + an opml::Subscription stub, and an
  azul_pim rmeta built from the worktree: `rustc --crate-name azul_pim --emit=metadata
  examples/azul-pim/src/lib.rs` -> /tmp/news9_check/libazul_pim.rmeta).

- 09eab67d6 RED / 1770e5c49 GREEN src/store.rs (keys, load_jobs, load -> Loaded {library,
  problems, minted}, subscriptions_job / meta_job / items_job / state_job / feed_jobs /
  delete_jobs, items_to_json / items_from_json). Type-check clean (harness5 = harness2 + library
  + store + an opml stub {Subscription, parse, write}; azul_appkit rmeta built from the worktree
  like azul_pim, without its `azul` feature).

- 2ad860375 RED / bf200bd5b GREEN src/fetch.rs (USER_AGENT, ACCEPT, Fetched {NotModified, Feed,
  Page, Failed}, FeedLink, Candidate, request, interpret, fetch, normalize_input, find_feeds) +
  reader::feed_links. Type-check clean (harness6 = harness2 with a feed_links stub + fetch).

- 82c74431d RED / 6db53f1d1 GREEN reader view: reader::{Article, article, reader_css,
  reading_minutes, images_of, Cleaner, element, text_node} (azul-dependent: NOT type-checked;
  re-read against target/codegen/dll_api_external.rs when resuming if in doubt).

- 426d4f8eb RED / 0f148f2de GREEN src/sample.rs (sample_library(now): 42 feeds, 891 articles,
  214 unread, 16 starred, 6 saved, BROKEN_404 = 20, BROKEN_INVALID = 33, NO_PICTURES = 2,
  PICTURES = 30). Type-check clean (harness7 = harness5 + sample).

- eccf4acd0 src/jobs.rs (A below: spawn_refresh / spawn_pictures / spawn_find, RefreshJob,
  RefreshEvent, PictureEvent, FindEvent, take::<T>(&mut msg)). "Open original": azul has no
  open-URL API -> plan: append `open_external(target)` to azul-appkit/src/files.rs (std Command:
  open / xdg-open / cmd start; twin of azul-review lib.rs:573) and list an engine API as left.

- a3ce8bfca RED / cb9fa11d8 GREEN azul-appkit files.rs external_target / open_external (shared
  helper for "Open original"; azul has no open-URL API).
- c5df3dccf ui.rs part 1 (facts, Settings / Pictures, age / long_date, AddFeed / OpmlImport /
  import_rows / Reading / NewsApp, start(), small pieces, tests). NOT yet `pub mod ui` in lib.rs.

- 08acca885 ui.rs part 2 (navigation_views / navigation, list_pane / article_row, reading_pane
  with article_view / add_feed_view / import_view / feed_page, toolbar, status_bar,
  settings_sections, layout). It names callbacks that part 3 must define (exact names):
  on_add_open on_nav on_row on_search on_filter on_show_more on_mark_all_yes on_mark_all_no
  on_prev on_next on_open_original on_star on_later on_toggle_read on_reading_event
  on_add_input on_add_find on_add_pick on_add_folder on_add_subscribe on_leave on_import_path
  on_import_choose on_import_read on_import_toggle on_import_run on_feed_title on_feed_folder
  on_feed_refresh on_feed_articles on_unsubscribe_confirmed on_unsubscribe on_refresh
  on_import_open on_export on_mark_all on_open_settings on_set_font on_set_measure on_set_paper
  on_set_pictures on_set_strip on_set_refresh_start on_set_refresh_every on_set_keep on_key
  on_window_created (+ on_files_done, on_refresh_event, on_picture_event, on_find_event, the
  refresh timer). Signatures as in azul-contacts (button: (RefAny, CallbackInfo) -> Update;
  text input: (.., TextInputState) -> OnTextInputReturn; segmented: (.., SegmentedState);
  switch: (.., SwitchState); checkbox: (.., CheckBoxState); nav: (.., ShellNavigationPaneEvent);
  reading pane: (.., ReadingPaneEvent); write-back: (RefAny app, RefAny msg, CallbackInfo)).

- 6fd772605 RED / 606831e88 GREEN links::is_documentation_host (sample addresses not refreshed).
- c47a09b57 ui.rs part 3 (all callbacks; no undefined / unused on_* names) + lib.rs
  `pub mod ui; start() -> ui::start()`. 02b0c10cd rustfmt. THE APP IS COMPLETE (uncompiled).

## NEXT
D1 (next step). examples/azul-news/scripts/feed_server.py: a local HTTP server (python3
   http.server, ThreadingHTTPServer, port arg) serving /feed.xml (RSS 2.0 with ETag + Last-Modified
   and 304 on If-None-Match / If-Modified-Since; a POST /bump adds an item and changes the ETag),
   /atom.xml, /feed.json, /broken.xml (malformed: bare &, unclosed tags), /missing (404),
   /page.html (an HTML page with <link rel=alternate> to the three feeds), /img/red.png (a small
   PNG), /subs.opml (an OPML listing the three feeds + a duplicate); logs each request with its
   conditional headers to stdout so the E2E can assert a 304 was asked.
D2. examples/azul-news/scripts/test_feed_server.py (plain python tests of the server: 200 with
   ETag, 304 on If-None-Match, 404, bump changes the ETag) - CI runs it (rust.yml dll_tests).
D3. scripts/aznews_e2e.py on scripts/azlin_e2e.py (like scripts/azcontacts_e2e.py): start the
   server, run AzNews --data-dir <tmp> --size 1200x760 with the OPML as file argument ->
   import preview (AZNEWS_IMPORT_PREVIEW 3) -> Import -> refresh -> AZNEWS_REFRESHED lines; files
   news/subscriptions.opml + news/feeds/*/items.json exist; click an article row (AZNEWS_SELECTED),
   reader shows text (get_node_layout on #__aznews_reader), Load pictures -> AZNEWS_PICTURE;
   star (state.json contains it); refresh again -> server log shows If-None-Match and the app
   prints `304`; Add feed with the page URL -> AZNEWS_FOUND 3; export -> AZNEWS_EXPORTED;
   settings Flora / Dark; screenshots. Second run with --sample on another folder: AZNEWS_LOADED 42
   891, AZNEWS_SAMPLE_WRITTEN 127 (1 + 42 * 3).
D4. Report scripts/NEWS9_2026_10_03.md (or the finishing date) + final progress.

## (done) B: ui.rs part 3. Old plan of part 2: navigation (pre-order views, TreeViewNode with_badge), the list
   (rows, day headers via DateGroup, Show more, mark-all confirm), the reading pane (Article via
   ReadingPane + InfoBar, AddFeed, Import, Feed page), toolbar, status bar, settings sections,
   layout(); part 3: callbacks (files done, refresh / picture / find events, nav, row, search,
   filter, star / later / unread / prev / next / open, add feed, import, export, feed page,
   settings, keys, timer). Then `pub mod ui;` + start() -> ui::start() in lib.rs. Then C, D.
A (done). src/jobs.rs - three azul Threads (pattern: azul-appkit/src/ui.rs file_thread + azul-mail
   lib.rs run_sync/post for several WriteBack messages and TerminateThread):
   - refresh: init {feeds: Vec<(id, url, etag, last_modified)>}; in the thread
     `AzulTransport::new(fetch::USER_AGENT).with_timeout(30)`, per feed `fetch::fetch` -> send
     WriteBack RefreshMsg {id, fetched, now}; a final Done message. UI: Library::merge (Feed) /
     meta.checked + status (NotModified) / meta.error + status (Failed, Page -> "this is a web
     page, not a feed"); then store::feed_jobs via kit::spawn_file_jobs.
   - pictures: init {urls}; per url GET through the transport, RawImage::decode_image_bytes_any
     (U8VecRef::from(bytes.as_slice())) -> ImageMsg {url, Option<RawImage>}; UI:
     ImageRef::create_rawimage(raw).into_option() -> info.add_image_to_cache(url, image).
   - find (Add feed): fetch::find_feeds -> FindMsg {Result<Vec<Candidate>, String>}.
B. src/ui.rs: SPEC / ABOUT (app_folder "news") / SHORTCUTS (j / k next / previous, s star,
   m read / unread, r refresh, Mod+N add feed, Mod+O import OPML, Mod+E export, Mod+F search,
   F6) / APP_CATEGORIES ["Reading", "Refresh"]; NewsApp {kit, data_root, sample, library,
   loaded, view, query, unread_only, selected: Option<ArticleRef>, reading: Reading {Article,
   AddFeed(..), Import(..), FeedPage(index)}, nav_open, notice, refreshing, pictures:
   BTreeSet<String> (in the image cache), images_for: Option<(feed id, item id)> (load pictures
   clicked), settings (font_px 20, measure 680, sepia false, images OnClick, refresh_on_start
   true, refresh_minutes 60, strip_tracking true, keep_days 30), list_limit 300}.
   layout(): like contacts - settings page or PimShell::create(nav, list, reading)
   .with_list_label("Articles").office_shell().with_title_row(kit::title_row("AzNews"))
   .with_ribbon(toolbar).with_status_bar(status) in ShellThemeScope::create(root).body() with
   the VirtualKeyDown callback. Reading pane: azul's ReadingPane (title, feed name, date,
   fields Author / Reading time, InfoBar "N pictures not loaded" + on_load_images, body =
   Dom::create_from_parsed_xml(reader::article(..).xml)) under a row of Buttons (Open original,
   Star, Read later, Mark unread, Previous, Next). Navigation: ShellNavigationPane with
   TreeViewNode groups (check how event.index counts nested nodes in
   layout/src/widgets/shells/navigation_pane.rs before mapping). On start: kit::spawn_file_jobs
   (store::load_jobs) -> store::load (new ids: azul_storage::ids::new_uuid) -> --sample on an
   empty library writes sample_library(now) -> refresh on start (not for the sample).
   Export OPML: FileJob::Put news/exports/subscriptions-<unix>.opml (exports go INTO the data
   tree, house rule). Import OPML: FileDialog (contacts' on_import_choose / spawn_outside_read).
   stdout lines for the E2E: AZNEWS_LOADED <feeds> <articles>, AZNEWS_VIEW <n>,
   AZNEWS_SELECTED <feed id> <item id>, AZNEWS_REFRESHED <id> <new|304|error>,
   AZNEWS_SUBSCRIBED <id> <url>, AZNEWS_IMPORT_PREVIEW <n>, AZNEWS_IMPORTED <n>,
   AZNEWS_EXPORTED <key>, AZNEWS_PICTURE <url>, AZNEWS_SAVED <key>.
C. main.rs already calls aznews::start(); lib.rs `pub fn start() { ui::start() }`.
D. E2E + feed server + its test, then the report (see 9 / 10 below).

## (older plan, kept for reference)
8. (done) src/sample.rs: `sample_library(now) -> Library` - deterministic (LCG like
   azul-contacts sample.rs), ~42 feeds in 6 folders (Tech, Science, Local, Cooking, Culture,
   Podcasts), example.org / example.net addresses, ~900 items over 60 days, 2 broken feeds
   (meta.error "HTTP 404" / "the feed could not be read: ..."), 1 feed without pictures, some read
   / starred / later marks; the articles' HTML has headings, quotes, code, a table, an image
   placeholder. Tests: counts, determinism, unread / starred counts, every id unique per feed.
   Then src/jobs.rs + src/ui.rs (see 8 below), E2E (9), report (10).
(done) 7. reader view, in src/reader.rs: `ImagePolicy { Always, OnClick, Never }`,
   `Article { xml: Xml (html > head > style READER_CSS, body > div.__aznews_article > cleaned
   tree), images: Vec<String> (Xml::scan_external_resources of the cleaned tree, kind Image,
   web only), blocked: usize, words: usize }`, `article(html, base, policy, strip_tracking) ->
   Article`; policy: keep p h1-h6 (h1 -> h2) blockquote pre code ul ol li dl dt dd figure ->
   div, figcaption -> div.__aznews_caption, img (src resolved, alt; tracking pixels <= 2px
   dropped; not loaded -> span.__aznews_image-placeholder "[image: alt]"), a (href resolved,
   tracking stripped), em strong b i u s sub sup mark small q cite abbr kbd, table thead tbody
   tr td th caption (colspan / rowspan), br hr; drop script style iframe object embed form input
   button select textarea svg math noscript template nav; unknown -> children kept; no other
   attributes. `reading_minutes(words)` (230 wpm, at least 1). READER_CSS: typography only,
   colours inherited from the theme (works in both modes), font-size / max-width from the
   settings (`reader_css(font_px, measure_px, sepia)`). Tests: scripts gone, relative links
   resolved, pictures listed / blocked, tracking pixel dropped, the stylesheet present.
(done) 6. src/fetch.rs: see item 6 below.
(done) 4. src/library.rs (pure, type-checkable with harness2 + `--extern azul_pim=<rlib>`):
   `FeedMeta` (serde, feed.json: format "aznews.feed" v1, url, title, site, icon, kind, etag,
   last_modified, checked, updated, status, error), `FeedData { sub: Subscription, meta, items:
   Vec<Item>, state: ReadState }`, `Library { feeds: Vec<FeedData> }` (subscription order),
   `View { All, Unread, Starred, Later, Folder(String), Feed(id), Broken }`,
   `ArticleRef { feed, item }`; fns: feed_index, folders, unread(feed) / unread_total /
   unread_in_folder, starred_count, later_count, broken, article, list(view, query) (newest first,
   search via azul_pim::search::Query over title + excerpt + author + feed title),
   sections(refs, today, offset_secs) -> Vec<(DateGroup, Vec<ArticleRef>)>, is_read / set_read /
   toggle_star / toggle_later, mark_all_read(refs) -> changed feeds, merge(feed, parsed: Feed,
   now, keep_days) -> new count (keeps `seen`, keeps starred / later and items inside keep_days,
   caps 500, prunes state, refreshes meta), subscribe / unsubscribe / subscriptions, next(list,
   current, forward). Tests: read state across merge, unread counts, starred kept when dropped.
5. src/store.rs: keys news/subscriptions.opml, news/feeds/<id>/{feed.json,items.json,state.json};
   load from FileOutcome::GotAll; FileJob builders.
6. src/fetch.rs: conditional GET (If-None-Match / If-Modified-Since) through
   azul_storage::{Transport, HttpCall, HttpReply, Method}; 304 -> NotModified; discovery of
   <link rel=alternate> feeds in an HTML page (Xml::create_from_html). Tests with a fake Transport.
7. reader view: article(html, base, images policy) -> Xml (reader stylesheet in <style>, cleaned
   tree, images resolved) + image URLs via Xml::scan_external_resources; tests.
8. src/sample.rs (--sample: ~42 feeds / 6 folders, generated items), src/jobs.rs (refresh Thread
   with AzulTransport, image Thread: fetch + RawImage::decode_image_bytes_any, writeback ->
   info.add_image_to_cache(url, ImageRef)), src/ui.rs (PimShell like AzContacts ui.rs, ReadingPane,
   settings via kit::settings_page, add-feed + OPML import in the reading pane, CloseGuard n/a).
9. scripts/aznews_e2e.py (model on scripts/azcontacts_e2e.py + scripts/azlin_e2e.py),
   examples/azul-news/scripts/feed_server.py (ETag / 304, a broken feed, an HTML page with links,
   an image) + examples/azul-news/scripts/test_feed_server.py.
10. Report scripts/NEWS9_2026_10_03.md (or the finishing date): api.json list (none planned so far),
    new crates (quick-xml 0.41, encoding_rs 0.8, url 2.5: all already in Cargo.lock), the DateGroup
    move, least-sure spots, test commands, what is left.

## Type-check trick (no cargo; LENIENT did the same)
- /tmp/news9_check/check.sh <harness.rs>: `rustc --emit=metadata --test` against the prebuilt
  rlibs in /Users/fschutt/Development/azul/target/release/deps. Harness = #[path] includes of the
  pure modules + stub `mod reader { collapse, plain_text, excerpt, text_to_html }` (harness2.rs).
  No azul rlib exists: azul-dependent code (reader.rs, ui.rs, jobs.rs) cannot be checked - read
  the generated API in /Users/fschutt/Development/azul/target/codegen/dll_api_external.rs and
  reexports.rs (python3 /tmp/news9_whereis.py Name -> azul::<module>::Name). /tmp may be gone on
  resume: recreate from this description.

## Decisions
- HOUSE RULE (coordinator, 2026-10-03): never send the user's email / personal data to a service
  (User-Agent, URL, payload). AzNews' User-Agent: `AzNews/0.1 (+https://github.com/fschutt/azul)`.
- Feed parsing: own parser on quick-xml 0.41 (already in Cargo.lock via docx-parser / ooxml-common /
  plist) in lenient mode (allow_dangling_amp, allow_unmatched_ends, check_end_names off) +
  serde_json for JSON Feed; NOT feed-rs (not in Cargo.lock; pulls its own quick-xml, mediatype,
  regex, uuid, siphasher). roxmltree is strict (a bare `&` or a truncated feed fails the whole feed).
  Charset via encoding_rs 0.8 (in Cargo.lock via mail-parser / lopdf). url 2.5 for links.
- HTTP: azul-storage's one Transport seam (`HttpCall` / `HttpReply`, `AzulTransport` = azul's
  HttpRequestConfig from an azul Thread) - no second HTTP client; tests use a fake Transport.
- Dates: own lenient reader; chrono only adds up the parts (its rfc2822 refuses wrong weekdays).
- Titles / excerpts: plain text through azul's HTML parser (reader::plain_text) - the engine's one
  entity table; text -> HTML through Xml::encode_text (the one encoder); item ids without
  guid/link: azul_storage::sigv4::sha256_hex (stable).
- xmltree::push_escaped is NOT a twin of Xml::encode_text: it writes the lenient tree back and
  keeps the HTML references (`&nbsp;`) the XML layer left undecoded. Say so in the report.
- Shell: S4 PimShell (.office_shell()), like AzContacts; reading pane = azul's ReadingPane widget.
- Data: news/subscriptions.opml (the list, user-facing), news/feeds/<uuid>/feed.json (url, title,
  etag, last_modified, checked, error), items.json, state.json (read / starred / later ids).
- DateGroup twins: azul-mail listing.rs DateGroup, azul-notes model, azul-tasks list.

## Open questions
- (none)
