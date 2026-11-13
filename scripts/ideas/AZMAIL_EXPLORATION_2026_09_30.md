# AzMail: what it needs (exploration, 2026-09-30)

Research and evidence only, nothing is built. Branch `wt/x-mail-explore`, cut from `34a8fe46f`.
Paths are relative to the azul repo. (The private planning notes it cites by name are not part of this repository.)

This builds on three earlier notes, which it corrects where the code has moved:
the mail UI plan, the mail-transport note (micromail, 2026-09-15) and the editing and HTML note
(2026-09-15), all private planning notes. The storage model follows the 2026-09-30 ruling: durable data is
files in S3 per user, "spam" is a folder, and the DB holds only minting, invites, transient state
and access links.

How the evidence was gathered:
- **Code reading:** file:line throughout.
- **Headless renders:** the release dylib of 2026-09-30 (`target/azul-lib/libazul.dylib`) was
  driven through `AzWidgets` with `AZ_BACKEND=headless AZ_E2E=<scenario> AZ_E2E_SHOT_DIR=...` and
  the E2E `mount` op. No window opened and nothing was compiled.
- **Python probes:** a STARTTLS SMTP server, a port-reachability check and crates.io queries.
- **Cloudflare docs:** read on 2026-09-30.
- **RED tests:** committed on this branch (section 8).

---

## 0. Executive summary

1. **azul cannot take mail HTML as it arrives.**
   - Raw mail HTML: 3 of 8 representative messages (the Outlook reply, a receipt, a legacy
     newsletter) **fail to parse at all**. The cause is one unquoted or boolean attribute, which
     fails the whole document (`xmlparser` 0.13.6 `lib.rs:981-985`).
   - Normalized first: once an HTML5 parser has normalized the markup, all 8 parse.
   - Normalized plus a legacy rewrite: when the sanitizer also turns `bgcolor`, `align`, `width`,
     `<font>` and `<center>` into CSS, replies and simple mail render well.
   - Newsletters and receipts built from tables still render badly: body text is clipped,
     columns collapse, and one receipt pattern **crashes the CPU renderer** (RED `a40976f60`).
   - **Pipeline:** `mail-parser` gives the parts; `ammonia` (html5ever) plus a mail-specific
     rewrite gives XHTML; azul's XML loader then builds the DOM. The work is all app-side, plus
     a list of engine fixes (section 6).
2. **Safety comes mostly for free, with one trap.** azul runs no scripts and never fetches
   `<img>`, `url()` or `<link>`, so tracking pixels are inert by construction. But a mail's
   `<form>`, `<input>` and `<button>` become **live, typeable widgets**
   (`styled_xml_document` -> `resolve_form_controls`, `layout/src/xml/mod.rs:257-275`): a
   phishing mail renders a working login form in the reading pane. The sanitizer must drop form
   elements.
3. **Dark mode:** keep designed HTML mail light, in a "paper" card, and restyle only simple
   mail (plain text, replies without colours) to the app's mode. Never invert.
   - Measured: with the Mac in dark mode, un-coloured mail text inherits the light system text
     colour, while author backgrounds stay light.
   - Engine gap: there is no per-subtree `color-scheme`, so a newsletter's own
     `@media (prefers-color-scheme: dark)` rules fire inside the light card.
4. **Crates:** `mail-parser` 0.11 plus `mail-builder` 1.0 (Stalwart, Apache/MIT, one and zero
   dependencies), `ammonia` 4.2 (html5ever 0.40), and charsets through mail-parser
   (`encoding_rs` only for CJK). `rusty-s3` 0.10 (sans-IO SigV4) handles S3. `lettre` 0.11
   (`rustls-no-provider` plus the rustcrypto provider azul already ships) is used only for SMTP
   submission to third-party providers. Not used: `mailparse`, `mail-send` (tokio and
   aws-lc-rs by default), `sequoia-openpgp` (LGPL, not allowed by `deny.toml`) and `rust-s3`
   (heavy).
5. **Mailbox:** a Cloudflare Email Worker streams each message's raw MIME to
   `users/<uid>/mail/<folder>/<yyyy>/<mm>/<ULID>.eml`, plus a small `<ULID>.json` sidecar.
   - Spam is the folder `spam/`.
   - Moves are CopyObject plus DeleteObject.
   - New mail is found with ListObjectsV2 `start-after` (ULIDs sort by time).
   - Flags live in one client-written file per folder-month, updated with `If-Match`.
   - There are no thread files and no mail rows in the DB.
6. **S3 authentication:**
   - **The app holds no long-lived S3 secret.** A per-device Ed25519 key sits in the OS keyring
     (azul has one: `CallbackInfo::keyring_store`, `layout/src/callbacks.rs:5526`).
   - It signs requests to a Worker. The Worker mints R2 **temporary credentials scoped to the
     prefix `users/<uid>/`** (R2 Temporary Credentials, prefix-scoped, derived from a parent
     token).
   - The app signs SigV4 with `rusty-s3` (`Credentials::new_with_token`) and talks straight to
     R2 through azul's `HttpRequestConfig`.
   - A share link is a DB row (`path prefix -> grantee, role, expiry`) that the same endpoint
     turns into read-only or read-write temporary credentials for that prefix.
7. **Sending is broken, and the crash reporter shares the bug.** crash mail and problem reports
   go through `micromail` 0.1.0, which is built without its `tls` feature. Against any server
   that offers STARTTLS (every real MX), it sends `STARTTLS`, gets `220`, and then carries on
   **in plaintext**. The server's TLS handshake fails and the report is lost.
   - Proven with a Python replay against a real TLS server: `[SSL: WRONG_VERSION_NUMBER]`, then
     connection reset.
   - Pinned by RED `c818942d2`, together with three message-format bugs: no `MIME-Version`, no
     dot-stuffing, bare LF.
   - Even fixed, direct-to-MX from a user's machine loses to deliverability. From this network,
     port 25 to Gmail and Outlook is open, but 587/465/2525 on their MX hosts time out, and
     neither provider takes unauthenticated mail from residential IPs without PTR, SPF or DKIM.
   - **Verdict:** crash reports POST over HTTPS to the `crash` Worker, which stores them in R2
     and notifies a *verified* address for free through Cloudflare Email Service. AzMail sends
     through a Worker `POST /v1/send`, using the Email Service binding for the user's own domain
     (Workers Paid; 3,000 mails/month included), or through SMTP submission (587/465 with AUTH)
     for external accounts.
8. **Editor:** use a flat block model with `quote_depth` per block (AzWriter's IR pattern), not
   nested `<blockquote>`s inside the contenteditable host.
   - Why: the engine's Enter and Backspace act on the host's *direct child*
     (`layout/src/window.rs:3683-3722`), so inside `blockquote > p` they split or merge the whole
     quote.
   - Flat blocks turn mail's "Enter inside a quote breaks out of it" into a model rule in the
     app.
   - Quote bars: one `border-left` per block today. Several bars need `linear-gradient` hard
     stops, which are broken (RED `a001cde2a`).
   - The app serializes text/plain (`format=flowed`, `>` quoting) and clean HTML itself.
   - Missing from the engine: raw HTML on paste, plain arrow keys across blocks, typing style at
     a collapsed caret, `href` from XML, and hit areas for inline links.

---

## 1. Mail rendering

### 1.1 Plain text: format=flowed, quoting, long lines, charset

- **Charset:** `mail-parser` decodes every text part to UTF-8.
  - It handles 41 charsets natively, including UTF-7, ISO-8859-x and windows-125x.
  - The `full_encoding` feature adds Shift_JIS, EUC-JP, ISO-2022-JP, Big5, GBK, GB18030 and
    EUC-KR through `encoding_rs`. `encoding_rs` 0.8.35 is already in azul's `Cargo.lock`.
  - azul never sees a charset.
- **format=flowed (RFC 3676):**
  - No maintained Rust crate does it, and `mail-parser`'s README doesn't mention it. It is about
    80 lines in the app:
    - A line ending in SP is soft-broken; join it with the next line at the same quote depth.
    - `DelSp=yes` deletes that space.
    - Remove one leading SP (space-stuffing).
    - Quote depth is the count of leading `>` before the stuffing. A signature starts at `-- `.
  - Without `format=flowed`, keep the lines as they are (senders hard-wrap at 72-78) and still
    count `>` for depth.
- **Model:** the output is the *same* flat block model the editor uses (section 5): a
  `Vec<Block { quote_depth, kind: Para | Signature, runs }>`. Reading and composing share one
  renderer.
- **Rendering in azul:**
  - Each block is a `<p>` with `white-space: pre-wrap; overflow-wrap: anywhere` (TextArea uses
    the same pair, `core/src/ua_css.rs:828-829`).
  - Quote levels render as indent plus a coloured bar (5.3), with long quote runs folded behind
    "[...]" (the Accordion pattern in `mail.md` 2.3).
  - URLs are detected in the app and rendered as links. Inline links are not clickable yet
    (6, E10). Until then, render each URL as a `display: inline-block` `<a>`, which gets a hit
    area.

### 1.2 HTML: what azul's loader does with mail HTML

**Two XML-to-DOM loaders, and they disagree (twins, per the NO DUPLICATION rule):**

| | `layout/src/xml/mod.rs:386-820` `parse_xml_to_fast_dom_with_css` (fast path, `parse_xml_to_styled_dom:278`) | `layout/src/xml/mod.rs:864-1150` `parse_xml_string` -> `core/src/xml.rs:6117` `xml_node_to_dom_fast` (`Xml::from_str`, `dom_from_parsed_xml`, `styled_xml_document:257`, the e2e `mount` render) |
|---|---|---|
| Tag case | lower-cased (`:406-417`) | **not** lower-cased: `normalize_casing("TABLE")` is `t_a_b_l_e`, which falls back to `Div` (`core/src/xml.rs:5360-5383`, `2699-2730`) |
| Unclosed element at end | closed silently (`:809-811`) | `Err(UnclosedRootNode)` (`:1142-1144`) |
| `<o:p>` (foreign namespace) | `o:p` becomes a `Div` box | dropped: `element_draws_nothing` (`core/src/xml.rs:6091-6104`) |
| HTML5-lite auto-close (`li`, `td`, `tr`, `p`, ...) | only via "pop until match" | explicit rules (`:882-925`) |

Both loaders use the same `xmlparser` tokenizer, and it is strict XML:

| Real-world HTML | xmlparser 0.13.6 | Effect |
|---|---|---|
| Unquoted value `class=MsoNormal`, `BGCOLOR=#FFF` | `InvalidQuote` (`lib.rs:981`) | **whole message fails** |
| Boolean attribute `nowrap`, `noshade`, `checked` | `consume_eq` fails (`lib.rs:980`) | **whole message fails** |
| `<` inside an attribute value | refused (`lib.rs:984`) | whole message fails |
| `--` inside a comment (`<!-- ---- -->`) | `InvalidCommentData` (`lib.rs:714`) | whole message fails |
| C0 control characters | `NonXmlChar` (`stream.rs:282`) | whole message fails |
| MSO conditional comments `<!--[if mso]>...<![endif]-->` | a comment | dropped: correct for MSO-only content |
| Named entities beyond `lt gt amp apos quot nbsp` (`&copy; &rarr; &mdash; &zwnj;`) | text | **left literal** (`layout/src/xml/mod.rs:64-72`) |

**Attributes:** everything goes through ONE table, `core/src/xml_attributes.rs:311-354`
(`setting_of:380`). It knows `id class style tabindex colspan rowspan dir contenteditable`, the
form-control attributes and `data-*`. **Ignored:** `bgcolor align valign width height`
(`width`/`height` only count on `<img>`, `core/src/xml.rs:5616-5650`), `border cellpadding
cellspacing color face size background text link`, and **`href`**. `AttributeType::Href` exists
(`core/src/dom.rs:1436`) but has no table entry, so a parsed `<a>` does not know where it points.

**Inline `style`:** `style_declarations` splits on `;` and then on the FIRST `:`
(`core/src/xml_attributes.rs:482-500`), so `background:url(http://x)` keeps only `url(http`.
That is harmless for mail, where remote URLs are stripped anyway.

**Elements** (`core/src/xml.rs:2748-2929`; unknown tags become `Div` = `display: block`):
- `<font>`, `<center>`, `<strike>`, `<tt>`, `<nobr>` and `<big>` (known, but without styles)
  are missing. **`<font>` becomes a block**, so every `<font>` inside a line breaks the line.
- **UA default styles** (`core/src/ua_css.rs`):
  - `em`/`i` get no italic (740, 743).
  - `code`/`pre` get no monospace font (747, 754).
  - `s`/`del` get no line-through (909).
  - `blockquote` gets no margin and no bar (756).
  - `a` gets no link colour (736-737).
  - `sub`/`sup` get no offset.

**Images:**
- `<img src>` becomes a `NullImage` placeholder that carries the `src` bytes
  (`core/src/xml.rs:5616-5650`). It is a grey box on the CPU renderer. Nothing is ever fetched,
  and `data:` URIs are refused (`layout/src/fetch.rs:36-37`).
- `Xml::scan_external_resources` (`core/src/xml.rs:357`) already lists every `img`, `link`,
  `url()` and `@import`. That list can drive the "This message has remote content" banner.

**CSS:**
- `<head><style>` becomes `Dom.css`.
- Component CSS with a real selector is scoped to the owning subtree (`core/src/style.rs:521-531`,
  `Root(range)`). A mail's stylesheet therefore cannot restyle the app's chrome, but its
  `position: fixed/absolute` can still paint outside the pane.
- **CDO/CDC (`<!--`, `-->`) inside `<style>` is not skipped.** Outlook wraps every stylesheet
  in them, and the probe lost the rules that followed (probe `css2`, below).
- `@font-face` and `@page` are skipped correctly (probe `css3`). `margin: 0cm` works.
- `@media (prefers-color-scheme: dark)` is matched against the window's mode
  (`css/src/dynamic_selector.rs:3202`).

### 1.3 Measurement: 8 mail samples, 3 passes each

The samples live in the session scratchpad (`xmail/samples/`): a Mailchimp-style newsletter, a
Gmail reply with two nested quotes, an Outlook/Word reply, a receipt built from `<font>` and
`bgcolor`, an Apple Mail reply, a Thunderbird reply with `<pre>` quotes, a hostile phishing mail
and a 1990s-style upper-case newsletter.

The three passes:
- `raw` is the bytes as received.
- `norm` is lxml's HTML5 parse, serialized as XML. It stands in for html5ever: lower-case tags,
  quoted attributes, entities resolved, comments dropped, head `<style>` moved to the mount CSS.
  Legacy markup is kept.
- `fixed` is `norm` plus the legacy rewrite a sanitizer would do: presentational attributes
  become inline CSS, `<font>` becomes a styled `<span>`, `<center>` a `<div>`, `<strike>` an
  `<s>` and `<tt>` a `<code>`. The UA gaps from 1.2 are patched in CSS, and the result is
  wrapped in a light card.

Each pass was mounted in a 760x1100 headless window. The harness is `xmail/mail_html_probe.py`,
`run_e2e.py` and `try_markup.py`.

| Sample | raw | norm | fixed |
|---|---|---|---|
| 01 newsletter | parses. `&rarr;` and `&copy;` literal. `bgcolor`/`width`/`align` ignored, so white header text sits on grey. Images are grey boxes, the second overflows the window. Body text (h1/p inside a padded `<td>`) invisible | parses | Header, button and footer right. **Body h1/p clipped off the left edge** (reduced to `<table width=600><td style="padding:24px"><h1>`, probe `nl`). The 2-column image row collapses into one overflowing image |
| 02 Gmail reply | parses. Nested quote bars render (Gmail inlines `border-left`). **`<div><br></div>` blank lines collapse to nothing** (probe `br`) | parses | good. Only `<ul>` markers missing |
| 03 Outlook reply | **FAILS**: `InvalidQuote` on `<meta http-equiv=Content-Type` | parses | good: bordered agenda table, From/Sent block. The CDO/CDC-wrapped `p.MsoNormal{margin:0}` is lost, so there are 1em gaps between every line |
| 04 receipt | **FAILS**: boolean `noshade` on `<hr>` | **PANIC** in the CPU renderer, `agg ScanlineU8::add_cell` index out of bounds | parses, but the **right column (prices) is missing** and the `<hr>` is wider than the table |
| 05 Apple Mail reply | parses | parses | good |
| 06 Thunderbird reply | parses | parses | good: `<pre>` quotes, nested bars, signature |
| 07 hostile | parses. `<script>` hidden and never run. **`<form>` rendered as live widgets**: a prefilled email field, a password field and a Sign-in button. `cid:` and remote images are grey boxes. The tracking pixel is inert | parses | (the probe had no sanitizer) |
| 08 legacy upper-case | **FAILS**: `InvalidQuote` on `<BODY BGCOLOR=#FFFFFF>` | parses | good. `<ol>` draws only the marker "2." |

Bisecting the panic took 12 variants and gave a minimal reproduction (RED `a40976f60`):

```html
<table><tr><td colspan="2"><div style="width:100%;height:4px"></div></td></tr>
       <tr><td>a</td><td>b</td></tr></table>
```

It needs all three of: a spanning cell, a percentage-wide block in it, and a following row with
two cells. `width:50px` renders fine, and so does a single-cell second row.

Two smaller probes found engine bugs outside mail's core path, and one found a harness caveat:
- **Gradients** (RED `a001cde2a`): `90deg` runs mirrored compared with `to right`; hard stops and
  stops at a length paint nothing.
- **Harness caveat:** a scenario whose `setup` shrinks the window below AzWidgets' size draws its
  first text line displaced to the right and smeared (seen at 360-500 px wide, not at 640 or
  760). The 760-wide sample runs are not affected. Not root-caused; a follow-up task should say
  whether it is the harness or the engine.
- **Headless screenshot colours:** headless screenshots use a white canvas
  (`follow_system_background = false`, `dll/src/desktop/shell2/headless/mod.rs:295-304`), yet
  the text takes the *system* mode's colour. On a Mac in dark mode, un-coloured text comes out
  light grey on white, so a screenshot is not reproducible across machines.

### 1.4 Safety

What azul already guarantees:
- No script engine.
- No resource loading from parsed markup (`img`, `link`, CSS `url()`, `iframe`/`object`: no
  box, nothing fetched).
- Selector scoping by subtree.

So remote images and tracking pixels are blocked by default *by construction*. "Load images" is
an explicit app action: fetch on a `Thread` with azul's `HttpRequestConfig`, then
`ImageRef::new_rawimage` and `CallbackInfo::change_node_image`.

The sanitizer (app-side, `ammonia` 4.2) must:

1. **Drop** `script style(link) iframe frame object embed applet form input button select
   textarea meta base` and every `on*` attribute. **Forms are the one real hazard:** azul turns
   them into live widgets (`layout/src/xml/mod.rs:241-255`, `layout/src/window.rs:16664-16674`).
2. **Allow** a tag set of `a b i u s em strong small sub sup br p div span blockquote pre code
   ul ol li dl dt dd h1-h6 hr table thead tbody tfoot tr td th caption col colgroup img center
   font` and friends. The rewrite (3) turns legacy tags into CSS before azul sees them.
3. **Rewrite legacy presentation into CSS**, as the `fixed` pass does:
   - `bgcolor` becomes background-color, `align` text-align (or `margin:auto` on a table),
     `valign` vertical-align, `width` width.
   - `cellpadding` and `border` move onto the cells.
   - `<font color face size>` becomes a styled `<span>`, `<center>` a centred `<div>`.
   - `<body bgcolor text>` goes onto the card.

   This is an `ammonia` `attribute_filter` plus a pre-pass over html5ever's DOM.
4. **Filter CSS:**
   - Keep `style` attributes through `filter_style_properties` with an allowlist.
   - Drop `position` (fixed/absolute), negative margins, `url()`, `@import`, `@font-face` and
     `behavior`/`expression`.
   - For head `<style>`: parse it (azul's own parser keeps only what azul understands), drop the
     same rules, strip CDO/CDC until the engine does, and attach the result to the mail's root
     so it stays subtree-scoped.
   - Wrap the mail in an `overflow: hidden` container.
5. **Images:**
   - `cid:` stays and resolves to the MIME part with that `Content-ID`, decoded with
     `RawImage::decode_image_bytes_any`.
   - `https:` becomes a placeholder with `data-remote-src` until the user clicks "Load images"
     or "Always for this sender".
   - 1x1 and `display:none` images are dropped as tracking pixels.
   - `data:` images up to a size cap are decoded by the app.
6. **Links:**
   - Keep `http(s)` and `mailto`; drop `javascript:` and `data:`.
   - Add a visible-text vs `href` host check (feeds the spam reasons in `mail.md` 2.5).
   - Clicks go through an app callback. The engine needs `href` in the XML table (E-XML-2) and
     inline hit areas (E10).
7. **Serialize** as XHTML-safe HTML, which ammonia's html5ever serializer already is: quoted
   attributes, void elements, entities limited to what azul decodes. A fuzz test should assert
   that every sanitizer output parses with `azul_layout::xml::parse_xml` (M1.3).

### 1.5 Dark mode for mail HTML (`DarkLightMode`)

| Option | Verdict |
|---|---|
| Invert (algorithmic lightness flip, like Outlook and Apple Mail dark mode) | **no**. It breaks brand colours and images, needs per-colour contrast heuristics, and is a whole engine feature |
| Keep light, in a card | **yes, for "designed" mail**: any `bgcolor`/`background`/`color`, a sized `<table>` layout, or a `<style>` with colours. The card sets `background:#fff; color:#1a1a1a` explicitly |
| Restyle to the app mode | **yes, for simple mail**: plain text, replies with only b/i/a/lists/quotes and no colours. Render through the same flat-block renderer as plain text, in theme colours (quote bars per level, in both modes) |

Engine gap E-MODE: CSS `color-scheme` per subtree is missing (`ModeCondition`,
`css/src/dynamic_selector.rs:835-842`, is evaluated against the window only). Until it exists,
the sanitizer must remove the `@media (prefers-color-scheme: dark)` blocks of a mail rendered in
the light card, or they fire inside it. A per-message "show dark version" toggle can later honour
such blocks when a mail ships them.

---

## 2. Parsing and building crates

Facts are from crates.io on 2026-09-30. "Deps" means the newest version's non-optional
dependencies.

| Crate | Version (published) | License | Deps (default features) | wasm | For | Verdict |
|---|---|---|---|---|---|---|
| **mail-parser** (Stalwart) | 0.11.9 (2026-09-09) | Apache-2.0 OR MIT | `hashify` (`encoding_rs` only with `full_encoding`) | yes, no I/O | MIME tree, headers (RFC 5322, 2045-2049, 2231, 2557, 6532), 41 charsets, `body_text()`/`body_html()` with automatic HTML<->text, part offsets. Fuzzed, MIRI-tested | **use** |
| mailparse | 0.17.0 (2026-09-06) | 0BSD | `charset`, `data-encoding`, `quoted_printable` | yes | MIME tree; no HTML<->text; copies more | no: fewer features for the same weight |
| **mail-builder** (Stalwart) | 1.0.0 (2026-09-12) | Apache-2.0 OR MIT | none (`gethostname` optional) | yes | multipart/alternative, related (`cid:`), mixed, RFC 2047 headers | **use** |
| lettre | 0.11.23 (2026-08-03) | MIT | `email_address`, `idna`, `nom` (+ features) | builder only | SMTP submission (blocking `SmtpTransport` fits an azul `Thread`); `rustls-no-provider` pairs with the `rustls-rustcrypto` provider already in `Cargo.lock` | **use, only for SMTP submission** (phase 4). Build with `default-features=false` (the default pulls native-tls) |
| mail-send (Stalwart) | 0.6.2 (2026-08-18) | Apache-2.0 OR MIT | tokio, tokio-rustls, rustls-platform-verifier; default `aws_lc_rs` | no | async SMTP + DKIM | no: tokio plus aws-lc-rs goes against layout's pure-Rust TLS policy (`layout/Cargo.toml:125-126`) |
| micromail (own) | 0.1.0 (2026-08-18) | MIT OR Apache-2.0 | base64, bytes, chrono, log, microdns, rand, thiserror | "WASM Edge" claim | direct-to-MX | fix for crash mail only (section 4), not for AzMail |
| **ammonia** | 4.2.0 (2026-09-17) | MIT OR Apache-2.0 | `cssparser`, `html5ever`, `maplit`, `url` | yes | allowlist sanitizer, `attribute_filter`, `filter_style_properties`, url schemes | **use**. 4.2.0 is 13 days old; the lockfile cooldown floor is 14 days (`scripts/supply-chain/lockfile_guard.py:258`), so lock 4.1.4 or wait until 2026-10-01 |
| html5ever | 0.40.1 (2026-09-14) | MIT OR Apache-2.0 | `log`, `markup5ever`, `memchr` | yes | HTML5 tree building (under ammonia) | via ammonia. markup5ever generates atoms in a build script: add an entry to `scripts/supply-chain/build-script-policy.toml` (verify) |
| encoding_rs | 0.8.42 (0.8.35 locked) | (Apache-2.0 OR MIT) AND BSD-3-Clause | cfg-if, ... | yes | charsets | via `mail-parser/full_encoding`; already justified in the tree |
| html2text | 0.17.1 | MIT | html5ever, tendril, thiserror, unicode-width | yes | snippets | optional; `mail-parser` already gives `body_text()` |
| lol_html (Cloudflare) | 3.0.1 | BSD-3-Clause | cssparser, selectors, ... | yes | streaming rewriter | no: it rewrites but does not normalize, so azul would still see malformed HTML |
| css-inline | 0.21.2 | MIT | html5ever, cssparser, selectors | yes | inlining `<style>` | no: azul supports subtree-scoped class rules |
| **rusty-s3** | 0.10.2 (2026-08-01) | BSD-2-Clause | `jiff`, `percent-encoding`, `url`, `zeroize` (+ `hmac`, `sha2` by default) | `wasm_bindgen` feature | sans-IO SigV4: presigned or header-signed Get/Put/Head/Delete/Copy, ListObjectsV2, multipart; `Credentials::new_with_token` | **use** (only `jiff` is new to the tree) |
| aws-sigv4 | 1.6.0 | Apache-2.0 | aws-smithy-* stack, MSRV 1.94.1 | partly | signing only | no: heavy |
| rust-s3 | 0.37.2 | MIT | about 20 (async stack) | no | full client | no: heavy, does its own I/O |
| keyring | 4.2.0 | MIT OR Apache-2.0 | keyring-core + backends | no | OS keyring | **not needed**: azul has `CallbackInfo::keyring_store/get/delete` |
| pgp (rpgp) | 0.20.0 | MIT OR Apache-2.0 | RustCrypto stack | `wasm` feature | OpenPGP | later, for PGP/MIME |
| sequoia-openpgp | 2.4.1 | **LGPL-2.0-or-later** | - | - | OpenPGP | **not allowed** (`deny.toml:35-51` has no LGPL) |
| cms (RustCrypto) | 0.2.3 | Apache-2.0 OR MIT | const-oid, der, spki, x509-cert | yes | S/MIME (CMS) | later, for S/MIME |
| postal-mime (npm) | 4.0.1 | MIT-0 | **none** | Workers | header and MIME parsing inside the Email Worker | **use in the Worker** |

**S/MIME and PGP hooks:** `multipart/signed` has to be verified over the exact bytes of the
signed part. `mail-parser` keeps offsets into the raw message, so the app can hand those bytes to
`cms` or `pgp`. `mail-builder` accepts a pre-built signature part. Neither needs a new azul API.

**Supply chain:**
- Where the crates live: AzMail belongs in `examples/azul-mail` (AzMeet and AzCalendar are
  workspace members, `Cargo.toml` members list), so every new crate goes into azul's
  `Cargo.lock`.
- What that requires:
  - a justification line per crate, direct or transitive (`scripts/dependency-justifications.toml`,
    gate `scripts/check_dep_justifications.py`);
  - a cargo-vet exemption or audit (`supply-chain/config.toml`);
  - the 14-day cooldown;
  - build-script policy for markup5ever.
- The rough count of new crates, from memory rather than a lockfile diff: about 15 (html5ever,
  markup5ever, tendril, string_cache, phf family, cssparser, dtoa-short, maplit, mail-parser,
  hashify, mail-builder, rusty-s3, jiff and one or two more). None of them is C code.

**Recommended set:** `mail-parser` + `mail-builder` + `ammonia` (html5ever) + `rusty-s3`, with
`lettre` added only when SMTP submission is built. None of these goes into the engine's
`layout`/`dll` crates.

---

## 3. The custom mailbox, and S3 authentication

### 3.1 Inbound: the Email Worker

Cloudflare Email Routing hands `email(message, env, ctx)` a `ForwardableEmailMessage`: `from`,
`to` (the envelope), `headers`, `raw` (a ReadableStream), `rawSize`, and
`setReject/forward/reply`.
- Inbound limit: **25 MiB**, and anything bigger is rejected.
- Email handlers on the Workers **Free** plan "may fail with `EXCEEDED_CPU`", so plan on Workers
  Paid. Sources:
  - https://developers.cloudflare.com/email-routing/email-workers/runtime-api/
  - https://developers.cloudflare.com/email-service/platform/limits/

Worker steps:
1. Map `message.to` to `uid` through the account's address table. This is routing, so it belongs
   in the DB per the storage ruling. Unknown recipient: `setReject("550 no such user")`.
2. `id = ULID(now)`: 26 characters that sort by time.
3. `tee()` the raw stream:
   - One branch goes to R2 through `new FixedLengthStream(message.rawSize)`, into
     `users/<uid>/mail/<folder>/<yyyy>/<mm>/<id>.eml`, with the bytes unchanged.
   - The other branch reads **only the header block** (64 KiB at most) with `postal-mime`. That
     keeps CPU low; full MIME parsing is the client's job.
4. Classify, cheaply:
   - `users/<uid>/mail/_state/rules.json` holds allow and block lists, cached per isolate.
   - A blocklisted sender gets `setReject`.
   - A DMARC fail in `Authentication-Results`/ARC, or a block rule, goes to `spam`.
   - Everything else goes to `inbox`.

   The real spam decision is the **local** classifier's ("sorts spam locally", `mail.md`). It
   moves messages afterwards with ordinary S3 moves.
5. Write the sidecar `<id>.json` next to the `.eml`. The sidecar is written after the `.eml`, so
   a listing that sees the `.json` knows the `.eml` is complete.

Sidecar fields: `{id, folder, received_at, size, envelope:{from,to}, message_id, in_reply_to,
references[], from, to[], cc[], reply_to, subject, date, list_id, list_unsubscribe,
content_type, has_attachments (multipart/mixed heuristic), auth:{spf,dkim,dmarc}, spam:{verdict,
reasons[]}}`. It is at most a few KiB. Attachments stay inside the `.eml`, the single source of
truth, and the client extracts them.

### 3.2 Folder layout (all files, browsable in AzFiles)

```
users/<uid>/mail/
  inbox/2026/09/01K6...ULID.eml        raw RFC 5322, immutable
  inbox/2026/09/01K6...ULID.json       sidecar written by the Worker (or by the client on move)
  spam/2026/09/...                     "spam" is literally a folder
  sent/ drafts/ archive/ trash/ <user folders>/
  _state/flags/inbox/2026-09.json      {id: {seen, flagged, answered, ts}}; clients only, If-Match
  _state/rules.json                    allow/block lists, folder rules (read by the Worker)
  _state/spam-model/...                local classifier's training data (client-owned)
  _outbox/<ULID>.eml                   queued sends (optional)
```

- **New mail:** list `mail/<folder>/<yyyy>/<mm>/` with `start-after=<last key seen>`. R2 supports
  `start-after`, and ULIDs make that an arrival feed with no DB. Months partition the listing, so
  3,000 mails a month means 6,000 keys, or 6 pages.
- **Move** (to spam, archive, ...): CopyObject both files to the new prefix, then DeleteObject.
  R2 CopyObject is server-side, so the bytes are never downloaded.
- **Flags:** one file per folder-month, written with `If-Match: <etag>` (R2 PutObject supports
  `If-Match`/`If-None-Match`). On `412` the client re-reads and merges, last writer wins per id
  by `ts`. New mail is unread because it has no entry.
- **Deletes and moves made on another device** show up as keys missing from a re-listed
  folder-month. Only the current month and the months still open in the UI need re-listing. If
  that ever costs too much, a short-lived change feed in the DB counts as "transient state" under
  the ruling.
- **Threads** are built on the client (JWZ over `message_id`/`in_reply_to`/`references` from the
  sidecars) and cached locally. There are no server-side thread files.
- **Size:** 25 MiB inbound. For partial downloads the client can Range-GET the `.eml`. Splitting
  large attachments out into `<id>.parts/` is a later option, not v1.

### 3.3 Authentication: how the desktop app reaches S3

| Option | Verdict |
|---|---|
| A long-lived R2 access key in the app, SigV4 with rusty-s3 | only for **bring-your-own bucket** (AzFiles' "own S3 or R2" mode). The key goes in the OS keyring. R2 tokens scope to buckets, not prefixes, so one key would open every user's folder |
| Presigned URLs minted per object by a Worker | fine for sharing single files. Too chatty for sync: a listing or every flag write would need a Worker round trip |
| **Worker-minted R2 temporary credentials, prefix-scoped** | **recommended.** R2 Temporary Credentials take a bucket, `permission`, `ttlSeconds` and **`prefixes`/`objects`**, and derive from a parent token. They can be minted through the API or signed locally as a JWT with the parent secret (https://developers.cloudflare.com/r2/api/s3/temporary-credentials/). The app then signs SigV4 itself (`rusty-s3` `Credentials::new_with_token`) against `<account>.r2.cloudflarestorage.com` |

- **Device identity:**
  - On first sign-in the app generates an Ed25519 key.
  - The private key is stored in the OS keyring (`CallbackInfo::keyring_store`,
    `layout/src/callbacks.rs:5526`, backends Keychain / KeyStore / libsecret / CredentialLocker,
    `core/src/keyring.rs:1-17`).
  - The app registers the public key with the identity Worker, which is the `device` table in
    `cloud-workers.md` (C5). Enrolment is by a code from an already-trusted device, or by passkey.
- **Every Worker call** carries `Authorization: Azlin device=<id>, ts=<unix>, sig=<Ed25519 over
  method \n path \n ts \n sha256(body)>`. The Worker verifies with WebCrypto Ed25519 and rejects
  a skew over 5 minutes.
- **`POST /v1/credentials`** returns `{accessKeyId, secretAccessKey, sessionToken, expiration,
  bucket, prefix}`:
  - For the device's own data: prefix `users/<uid>/`, read-write, TTL about 1 h.
  - For a share (3.4): the link's prefix, with the link's role.
- **Revoking a device:** delete its row. The credentials it already holds die within one TTL.
- **The HTTP transport exists:** `HttpRequestConfig::http_request` does GET/PUT/HEAD/DELETE with
  custom headers and returns the response headers, including ETag (`layout/src/http.rs:498-534,
  713-724, 1029-1050`). It uses pure-Rust TLS.
- Everything runs on an azul `Thread` (the calls block).

### 3.4 Outbound forwarding / access links (the DB part)

```sql
access_link(id, owner_uid, path_prefix, grantee_kind /* 'user'|'email'|'anyone' */, grantee,
            role /* 'view'|'edit' */, token_hash, expires_at, created_at, revoked_at)
```

- `POST /v1/links {path, grantee, role, expires}`: only the owner, and `path` must start with the
  owner's `users/<uid>/`. It returns `https://<domain>/s/<token>` for `anyone` links.
- `DELETE /v1/links/<id>`: revoke.
- `POST /v1/credentials {link: <token or id>}`: the Worker checks the row (grantee equals the
  caller's uid, or `anyone`), then mints temporary credentials with `prefixes=[path_prefix]` and
  `permission` = object read-only or read-write, with a TTL of at most 1 h and never past
  `expires_at`.
- `GET /s/<token>`: a landing page for people without the app. It lists the objects through the
  R2 binding and hands out **presigned GET URLs**. R2 presigned URLs last at most 7 days, cover
  GET/HEAD/PUT/DELETE only (no POST forms), and work only on the S3 API domain, not on custom
  domains (https://developers.cloudflare.com/r2/api/s3/presigned-urls/).
- This table and these endpoints are shared with AzFiles, AzCalendar and AzMeet (meeting
  folders), so they belong in the `share` Worker of `cloud-workers.md`, not in `mail`.

### 3.5 Minimal Worker endpoints

| Worker | Endpoint | Does |
|---|---|---|
| `mail` | `email(message, env, ctx)` | 3.1 |
| `mail` | `POST /v1/send` (device-signed; body is raw MIME, 5 MiB at most) | checks that `From` is one of the uid's addresses, sends through the Email Service `EMAIL` binding (the legacy `EmailMessage` API takes raw MIME), stores the bytes in `sent/` |
| `identity` (C5) | `POST /v1/devices`, `DELETE /v1/devices/<id>` | device public keys |
| `identity` / `share` | `POST /v1/credentials` | temporary R2 credentials (own prefix, or a link) |
| `share` (C9) | `POST /v1/links`, `DELETE /v1/links/<id>`, `GET /s/<token>` | 3.4 |
| all | `GET /health` | |

**Local development**, per the user's ruling of 2026-09-30 (Python mock servers, one shared
conformance suite): a Python mock of `mail`, `identity` and `share`, with sqlite, JSON and
Ed25519 through `cryptography`, in front of `moto`'s S3 server (Python, Apache-2.0, speaks SigV4
and presigned URLs).
- The mock takes `POST /__email` (envelope plus raw MIME) in place of Email Routing.
- The same HTTP conformance tests run against `wrangler dev`, whose local email trigger the tests
  drive, and against the mock.
- The existing `meet` Worker's mock is Node (`dev-server.mjs` next to that Worker). The mail mock
  should start in Python.

---

## 4. Sending mail: is it broken? Yes

### 4.1 What the code does

- **Callers:**
  - The crash reporter: `layout/src/dialogs/crash_reporter.rs:156-165`, which calls
    `send_dump_file`.
  - The problem reporter: `layout/src/dialogs/report_problem.rs:383-404`, which calls
    `send_attachments` with default `CrashMailConfig::new`.
  - `AppConfig.report_problem` arms it (`dll/src/desktop/app.rs:508-519`, through
    `config_from_report_address`).
- **Build:** `build-dll` includes `crash-mail` (`dll/Cargo.toml:737-740`). Its comment
  "Pulls rustls for STARTTLS" is **wrong**: `layout/Cargo.toml:143` depends on `micromail` with
  `default-features = false`, and `Cargo.lock:5290-5302` lists no rustls under micromail.
- **Transport:** `layout/src/telemetry/crash_mail.rs:257-270` calls `micromail::Mailer::send_sync`
  with ports `[25, 587, 2525]` and `use_tls: true` (`:56-57`).
- **The flagging report:** the mail-transport planning note (2026-09-15; no `scripts/*.md`
  mentions micromail). Re-checked against the published source, it still holds.
  Paths below are in `~/.cargo/registry/src/*/micromail-0.1.0/src`:

| Bug | Where | Effect |
|---|---|---|
| **Plaintext after STARTTLS** | `connection.rs:194-200`: without `tls` it keeps the TCP stream and returns `(conn, true)`. `mail.rs:180-188` then sends EHLO again | every MX offers STARTTLS, so the send fails. **Reproduced:** `xmail/smtp_probe.py starttls` replays the sequence against a Python server with real TLS: `server: TLS handshake FAILED: [SSL: WRONG_VERSION_NUMBER]`, `client: [Errno 54] Connection reset by peer` |
| TLS would skip certificate checks | `tls.rs:63-68` `NoCertificateVerification`; server name from the IP (`connection.rs:205`) | turning on `tls` as declared would pull rustls' default aws-lc-rs, against `layout/Cargo.toml:125-126` |
| No `MIME-Version` | `mail.rs:86-110` | the multipart body can show as raw text |
| No dot-stuffing | `mail.rs:317-318` | a user line starting with `.` loses its dot, and a lone `.` ends DATA early |
| Bare LF | `utils.rs:26-32`: `ensure_crlf` converts only when the body has no CRLF at all, and crash_mail's MIME always has some | multi-line user messages go out with bare LF, which strict servers reject (SMTP-smuggling defences) |
| One recipient, no Cc | `mail.rs:29-37` | |
| DKIM is a no-op | `mail.rs:112-120` | no signature, so mail fails DMARC for any domain with a policy |
| Port fallback is useless | `connection.rs:82-119`, default connect timeout 30 s (`config.rs:48`) | MX hosts listen only on 25. **Measured** from this machine: `gmail-smtp-in.l.google.com` and `outlook-com.olc.protection.outlook.com` accept 25, and 587/465/2525 time out. On a port-25-blocked network a send waits up to MX count x 3 ports x 30 s |

**Deliverability** also rules out direct MX sending from user machines, even with every bug
fixed:
- Google requires SPF or DKIM, plus valid forward and reverse DNS (PTR), from **all** senders
  (https://support.google.com/a/answer/81126).
- Residential IPs are on Spamhaus PBL.
- Many ISPs block outbound port 25 (leads in `mail-transport.md` section 4).

### 4.2 RED tests (committed, not fixed)

`c818942d2`, `layout/src/telemetry/crash_mail.rs` module `smtp_sink_tests`:
- The tests run a std-only, one-shot SMTP sink on `127.0.0.1:0`. `micromail` routes any
  `...@localhost` recipient to 127.0.0.1 on the configured ports (`dns.rs:26-31`), so no DNS is
  involved.
- The contact is exactly what `config_from_report_address` builds, pointed at the sink.

| Test | Asserts | Today |
|---|---|---|
| `a_crash_mail_never_speaks_plaintext_after_the_server_agreed_to_starttls` | after the sink's `220` to STARTTLS, the next bytes are a TLS record (`0x16`) or nothing | RED: `EHLO localhost` |
| `a_crash_mail_declares_mime_version_1_0` | the header block has `MIME-Version: 1.0` | RED |
| `a_user_message_line_that_starts_with_a_dot_arrives_intact` | `.config/azul was missing` survives RFC 5321 section 4.5.2 | RED: arrives as `config/...` |
| `a_multi_line_user_message_goes_out_with_crlf_line_ends` | no bare LF in DATA | RED |

Run with `cargo test --release -p azul-layout --features crash-mail --lib
telemetry::crash_mail::smtp_sink_tests`.

### 4.3 Verdict: the send path

- **Crash and problem reports:** **HTTPS POST to the `crash` Worker** (C2 of the build ledger).
  - Transport: azul's `http` feature (ureq with rustls-rustcrypto), which telemetry already uses.
  - Store: the Worker writes `crash/<app>/<version>/<yyyy>/<mm>/<id>.json` in R2.
  - Notify: it mails the owner through Cloudflare Email Service's `send_email` binding to a
    **verified destination address**. That is free on all plans, does not count against the
    quota, and allows 25 MiB
    (https://developers.cloudflare.com/email-service/platform/pricing/).
  - Keep `crash-mail` as an explicit opt-in only after micromail 0.1.1 fixes the list above.
    Until then, take it out of `build-dll`'s default feature list (`dll/Cargo.toml:737-740`).
  - The report dialog's ladder (`report_problem.rs:350-404`) becomes: Worker, then mail if
    opted in, then disk.
- **AzMail, the user's own domain:** `POST /v1/send` on the mail Worker, through the Email
  Service `EMAIL` binding.
  - Plan: Workers Paid, 3,000 mails/month included, then $0.35 per 1,000. Outbound limits are
    5 MiB and 50 recipients (https://developers.cloudflare.com/email-service/api/send-emails/workers-api/).
    Email Sending is still in beta.
  - SPF, DKIM and DMARC are Cloudflare's once the domain is onboarded.
- **AzMail, external accounts** (Gmail, Fastmail, ...): SMTP submission from the app, on 587
  with STARTTLS or 465 with implicit TLS, using AUTH PLAIN or LOGIN (OAuth2 XOAUTH2 for
  Gmail/M365 later).
  - Library: `lettre` (blocking) on an azul `Thread`.
  - Build: `default-features = false`, with `smtp-transport`, `rustls-no-provider` and the
    rustcrypto provider.
  - Credentials go in the keyring.
- **Direct MX delivery** ("send without a server"): not a product path. It stays an experiment
  behind the deliverability rig (U3 in the build ledger).

---

## 5. Rich-text editing for mail

### 5.1 What the editing stack gives today

These points come from the Explore survey, with the key lines spot-checked:
- **Hosts:** `TextArea` is a contenteditable host with `white-space: pre-wrap` and **plain text
  only** (`layout/src/widgets/text_area.rs:236-259`; the "KNOWN GAP" note at `:23-28` still
  holds for Enter and delete). Any `contenteditable` element is a rich host. The flag is
  `core/src/dom.rs:3486-3492`, and the XML attribute is `core/src/xml_attributes.rs:173-184`.
- **Typing:** it goes into the styled run under the caret (`layout/src/text3/edit.rs:553-629`).
  There is no pending "typing style" at a collapsed caret. Backspace across runs keeps the
  previous run's style (`:890-915`).
- **Enter and Backspace in a normal host:**
  - They record `SplitNode`/`MergeNodes` (`layout/src/default_actions.rs:162-176, 210-224`) on
    `structural_edit_node`, the host's **direct element child** on the caret's path
    (`layout/src/window.rs:3683-3722`).
  - The app applies the edit (`DocumentEdit` event, `layout/src/document_edit.rs:99-158`).
  - Only one structural edit can be pending (`window.rs:1783`).
- **Cross-block delete and paste** flatten to plain text in v1 (`window.rs:4148-4154, 4162`).
- **Clipboard:** copy writes RTF + HTML + text. Paste inserts **plain text** by default. An app
  can intercept `Paste` and read `styled_runs` (`StyledTextRun`: bold, italic, size, colour,
  family; no links, lists or quotes; `layout/src/managers/selection.rs:30-74`). **Raw HTML never
  reaches the app.**
- **Shortcuts:** the built-in ones are Copy, Cut, Paste, SelectAll, Undo and Redo only
  (`core/src/events.rs:4740-4747`). Ctrl+B, I and K are the app's job through `VirtualKeyDown`.
- **Serialization:** `StyledDom::get_html_string` (`core/src/styled_dom.rs:2778`) is debug output
  (node ids, computed styles). There is no usable DOM-to-HTML or DOM-to-quoted-text serializer.
- **AzWriter** (`examples/azul-writer`) is the working pattern:
  - An app-side IR renders one DOM child per block and one node per run.
  - Formatting toggles in the IR, followed by `RefreshDom`.
  - `DocumentEdit` handles `SplitNode`/`MergeNodes` in the IR.
  - Quotes are a paragraph style, each drawn with its own `border-left`.

### 5.2 Why nested blockquotes in the editable host do not work

In `div[contenteditable] > blockquote > blockquote > p > "abc|def"`:
- **Enter** records `SplitNode{node: blockquote(outer), at: before_child(...)}`. It clones the
  outer quote and moves the rest in, and the `<p>` is not split at the caret
  (`window.rs:3683-3722`, `3866-3964`).
- **Backspace** at the start of any quoted `<p>` merges the whole outer quote into the previous
  block (`window.rs:4452-4492`).
- **Arrow keys** without Shift never leave the current block (`window.rs:11738-11767`).

### 5.3 The smallest design that works

- **Model** (app-side, in `azmail-core`):

  ```
  MailDoc { blocks: Vec<Block> }
  Block  { quote_depth: u8, kind: Para | ListItem { ordered, level } | Signature,
           runs: Vec<Run { text, bold, italic, link: Option<String> }> }
  ```

  Reading plain text (1.1), simple HTML replies and compose all use it.
- **Render:** each block is a **direct child** of one contenteditable host, like AzWriter, so the
  engine's split and merge target the block. A block is a `<p>` (or an `<li>`-looking `<p>`
  with a marker) with classes `q1`/`q2`/`q3`/`q4+`.
  - Quote level:
    - Today: `padding-left: 12px * depth; border-left: 3px solid var(level colour)`, one bar
      in the innermost level's colour.
    - After the gradient fix: every level's bar, drawn by a hard-stop `linear-gradient`
      background (RED `a001cde2a` shows that is broken today).
  - Colours per level, in both modes, come from the theme (`@theme`/mode rules). For example,
    light: `#1a73e8`, `#188038`, `#e37400`; dark: `#8ab4f8`, `#81c995`, `#fdd663`.
- **Mail semantics** live in the model, triggered by the engine's events:
  - Enter in a quoted block at a non-empty caret splits it into
    `[depth d, text before] + [depth 0, empty] + [depth d, text after]` (inline reply).
  - Enter on an empty quoted line drops one level.
  - Backspace at the start of a quoted block lowers its depth instead of merging.
  - The app acknowledges each engine `SplitNode`/`MergeNodes` with its own inverse
    (`mark_document_edit_applied_with_inverse`) and `RefreshDom`s.
- **Formatting:** the app handles B, I and link in `VirtualKeyDown` for Ctrl/Cmd+B, I and K, and
  in the toolbar. Selection spans (`CallbackInfo::get_document_selection`) map to block byte
  ranges, the runs toggle, then `RefreshDom`. This is AzWriter's `toggle_format_range`; reuse it
  rather than write a twin. Lists are block kinds.
- **Paste HTML:** until the engine passes raw HTML (E-PASTE), map `styled_runs` to runs (bold and
  italic survive; links, lists and quotes don't). After it, run the pasted HTML through the same
  sanitizer (1.4) and turn the result into blocks.
- **Output:** the app writes `multipart/alternative` from the model with `mail-builder`. No DOM
  serializer is needed.
  - **text/plain** is `format=flowed; delsp=no`: `> ` repeated `depth` times, soft breaks at 72
    (78 hard), space-stuffing, `*bold*`/`_italic_` optional, links as `text <url>`.
  - **text/html** is clean markup: consecutive blocks of the same depth are regrouped into
    nested `<blockquote type="cite" style="margin:0 0 0 .8ex;border-left:1px solid #ccc;
    padding-left:1ex">`, the de-facto interop form Gmail and Thunderbird use, with runs as
    `<b>`/`<i>`/`<a href>`.
  - Round-trip tests: model to HTML to (sanitizer + import) to model is the identity. The same
    holds for plain text.
- **Clearing the body after Send:** call `mark_text_revision_synced(get_document_text_revision())`
  first, or the engine's uncommitted typing outranks the new DOM (`foundation/09` section 2;
  `scripts/W1_INPUT_TYPES_2026_09_29.md:157`).

### 5.4 Engine gaps for the editor (file:line where the fix goes)

| Id | Gap | Where |
|---|---|---|
| E-PASTE | raw HTML (or a parsed fragment) on paste | `ClipboardContent` `layout/src/managers/selection.rs:66-74`; keep the HTML in `dll/src/desktop/shell2/common/clipboard.rs:251-303`; default paste `dll/src/desktop/shell2/common/event.rs:8009-8056` |
| E-ARROW | plain Up/Down/Left/Right cross blocks | `layout/src/window.rs:11738-11767` (reuse `step_document_focus`, `:11556`) |
| E-TYPESTYLE | typing style at a collapsed caret (Ctrl+B, then type) | `layout/src/text3/edit.rs:553-629` |
| E-NESTED | Enter/Backspace act on the caret's innermost block (needed only for nested DOM editing; the flat model avoids it) | `layout/src/window.rs:3683-3722, 3866-3964, 4452-4492` |
| E-XBLOCK | cross-block delete/paste keeps runs | `layout/src/window.rs:4162-4293` |
| E-GRAD | gradient direction and stops (quote bars) | RED `a001cde2a`; `css/src/props/style/background.rs` (LinearGradient `:200`) and the CPU and GPU gradient paths |
| E-SET | replace a live editor's content from code (quote the original, insert a signature) | text-change / content-overlay path (`scripts/W1_INPUT_TYPES_2026_09_29.md:157`) |

---

## 6. Engine gaps, consolidated

| Id | Gap | Where | Evidence |
|---|---|---|---|
| E-TABLE | a spanning cell with a %-wide child crashes the CPU renderer; table columns vanish or clip (receipt, newsletter) | `layout/src/solver3` table sizing; harden `layout/src/cpurender/raster.rs:3315` (`render_glyphs_lcd`) against degenerate clips | RED `a40976f60`; probes `bis_*`, `nl`, sample 04 |
| E-XML-1 | HTML5 leniency: unquoted and boolean attributes fail the whole document | `layout/src/xml/mod.rs:864` / xmlparser. **Decision:** leave the engine strict and normalize with html5ever in the app (1.2). A lenient azul HTML mode is a large feature |
| E-XML-2 | `href` not in the XML attribute table | `core/src/xml_attributes.rs:311-354` (add `entry("href", AnyElement, ..)` giving `AttributeType::Href`, `core/src/dom.rs:1436`) | 1.2 |
| E-XML-3 | the two loaders disagree (case, unclosed-at-end, foreign elements) | `layout/src/xml/mod.rs:386-820` vs `:864-1150` + `core/src/xml.rs:5360, 6091` | 1.2 table |
| E-XML-4 | only 6 named entities | `layout/src/xml/mod.rs:64-72` (the HTML5 entity table, or leave it to html5ever) | sample 01 raw |
| E-CSS-1 | CDO/CDC inside `<style>` loses rules | CSS parser (`css/src/parser2.rs`) | probe `css2`, sample 03 |
| E-UA | italic em/i/cite, line-through s/del, monospace code/pre/kbd/samp, blockquote margin, link colour | `core/src/ua_css.rs:736-756, 909` | smoke, samples |
| E-BR | a block holding only `<br>` has no height (Gmail's blank lines) | inline layout of a lone hard break (`core/src/ua_css.rs:847` + IFC collection) | probe `br`, sample 02 |
| E-OL | list markers drop on some items | list marker generation | samples 02, 08 |
| E10 | inline elements (links in text) get no hit area | `scripts/ideas/todo-ledger.md` E10 (`layout/src/solver3/fc.rs:9665-9690`, `4309-4325`) | ledger |
| E-MODE | per-subtree `color-scheme` (light card in a dark app) | `css/src/dynamic_selector.rs:835-842`; mode context in `core/src/styled_dom.rs` | 1.5 |
| E-IMG | a hook to resolve `<img src>` (a `cid:` map, remote after consent) while loading XML | `core/src/xml.rs:5616-5650` (today the app swaps images after load with `change_node_image`) | 1.2 |
| E-SHOT | headless screenshot text follows the system mode on a fixed white canvas | `dll/src/desktop/shell2/headless/mod.rs:295-304` | 1.3 |

---

## 7. Phased build plan (each M-task is one agent)

Every task is RED first. "Worker" tasks happen in the Workers' own repository with a Python mock
and one shared conformance suite.

**Phase 0: fix what this exploration proved** (engine and transport; independent of AzMail)
- **M0.1 Crash and problem reports over HTTPS.**
  - Scope: a `crash` Worker plus its Python mock; a `crash_http` transport in
    `layout/src/telemetry`; the report ladder in `dialogs/report_problem.rs`/`crash_reporter.rs`.
  - Drop `crash-mail` from `build-dll` defaults.
  - Green: dumps land in R2 (moto locally); the owner is notified at a verified address.
- **M0.2 micromail 0.1.1** (our crate).
  - TLS: `tls` with `rustls` `default-features=false` plus the rustcrypto provider, webpki-roots
    verification, SNI equal to the MX host, and a require-TLS option.
  - Message format: `MIME-Version`, dot-stuffing, CRLF normalization.
  - Connection: ports `[25]`, a 10 s connect timeout, multiple RCPT.
  - Then bump `layout/Cargo.toml:143`.
  - Green: the 4 RED tests of `c818942d2`.
- **M0.3 Table colspan crash.** Green: RED `a40976f60`. Then, as new REDs, the receipt's missing
  price column and the newsletter's clipped cell text (reduced probes `bis_D`, `nl`).
- **M0.4 Gradients.** Green: RED `a001cde2a`. It unblocks multi-bar quotes.
- **M0.5 XML and UA for mail.**
  - E-XML-2 (`href`), E-CSS-1 (CDO/CDC), E-UA (defaults), E-BR (`<div><br></div>` height),
    E-OL.
  - One RED per item (reftest-style layout tests).
- **M0.6 Inline hit areas (E10).** L-sized, already designed in `todo-ledger.md` E10. It makes
  links in mail clickable.
- **M0.7 `color-scheme` per subtree (E-MODE).**

**Phase 1: azmail-core** (`examples/azul-mail/core` or `azlin-kit`, no UI)
- **M1.1 parse.** A `mail-parser` wrapper giving `MessageView` (headers, text and HTML bodies,
  attachments, a `cid` map, auth-results). Corpus: the 8 samples plus RFC edge cases (2047
  headers, 2231 filenames, UTF-7, nested multiparts, TNEF left as an attachment).
- **M1.2 plain.** RFC 3676 decode and encode, the quote-depth block model, signature and URL
  detection. Round-trip tests.
- **M1.3 sanitize.**
  - The html5ever/ammonia pipeline, with the policy from 1.4: legacy rewrite, CSS filter,
    form removal, image policy and tracking-pixel drop.
  - Plus the "designed vs simple" classifier (1.5).
  - Fuzz test: every output parses with azul's XML parser.
  - Dependency justifications, a cargo-vet exemption, and the build-script policy for
    markup5ever.
- **M1.4 RichTextView (reading pane).**
  - Mounts the sanitized XHTML, resolves `cid:` images, offers "Load images" through the HTTP
    thread, and routes link clicks through a callback.
  - Light card or themed flat blocks, chosen per 1.5.
  - E2E screenshot tests of the 8 samples in both modes.

**Phase 2: cloud mailbox**
- **M2.1 mail Worker, inbound:** 3.1 and 3.2. ULID keys, sidecar, `rules.json`, DMARC-fail to
  spam, 25 MiB. Python mock with `POST /__email`, plus the conformance suite.
- **M2.2 identity + credentials:** device keys (keyring), signed requests, R2 temporary
  credentials scoped to `users/<uid>/` (local JWT signing), and a mock with moto.
- **M2.3 app sync:** `rusty-s3` over `HttpRequestConfig` on a `Thread`: month listing with
  `start-after`, a local sidecar cache, moves as copy plus delete, and the flags file with
  `If-Match`/`412` merge.
- **M2.4 send:** `POST /v1/send` through the Email Service binding, with a copy to `sent/`. The
  app builds MIME with `mail-builder`.
- **M2.5 share links:** the `access_link` table and endpoints (3.4), in the shared `share`
  Worker.

**Phase 3: compose**
- **M3.1 MailDoc editor:** the flat block model (5.3), render and sync with AzWriter's pattern
  (promote AzWriter's IR instead of copying it), and mail Enter/Backspace semantics.
- **M3.2 serializers:** text/plain (flowed, `>`) and clean HTML (nested `blockquote
  type=cite`) as `multipart/alternative`, with round trips.
- **M3.3 toolbar and shortcuts:** B, I, link, lists, quote and unquote; a link dialog.
- **M3.4 HTML paste:** engine E-PASTE, then sanitizer to blocks.
- **M3.5 engine editing gaps** as needed: E-ARROW, E-TYPESTYLE, E-SET.

**Phase 4: external accounts** (unchanged from `mail.md` section 7): IMAP/JMAP, SMTP submission
with `lettre` (4.3), OAuth2, and the local Bayesian classifier.

---

## 8. RED tests on this branch

| Commit | Test | Run |
|---|---|---|
| `a40976f60` | `layout/tests/a_full_width_rule_in_a_spanning_table_cell_renders.rs` (2 tests: finite text geometry; the CPU render does not panic) | `cargo test --release -p azul-layout --test all a_full_width_rule_in_a_spanning_table_cell_renders` |
| `c818942d2` | `layout/src/telemetry/crash_mail.rs` `smtp_sink_tests` (4 tests, 4.2) | `cargo test --release -p azul-layout --features crash-mail --lib telemetry::crash_mail::smtp_sink_tests` |
| `a001cde2a` | `layout/tests/a_linear_gradient_puts_its_colours_where_css_says.rs` (3 tests) | `cargo test --release -p azul-layout --test all a_linear_gradient_puts_its_colours_where_css_says` |

---

## 9. Open questions for the user

1. **Addresses:** does AzMail host mail on the user's own domain (Email Routing plus Email
   Service; needs the domain on Cloudflare and Workers Paid), on a shared Azlin domain, or both?
   This decides the `to -> uid` table and the send path.
2. **Crash reporting:** is it fine to drop `crash-mail` from `build-dll` defaults now, since it
   cannot deliver today, and route crash reports through the `crash` Worker?
3. **Where the mail stack lives:** AzMail in `examples/azul-mail` means about 15 new crates in
   azul's lockfile. A separate `azul-apps` workspace would keep the engine's supply-chain surface
   unchanged. Which one?
4. **Read and flag state:** is one client-written flags file per folder-month acceptable, or
   should it be DB "transient state" for instant multi-device sync?
5. **Sharing:** should the `share`/`identity` Workers be built for AzMail, or wait for the cloud
   spine (C5-C9) and let AzMail start with single-device BYO-bucket mode?
