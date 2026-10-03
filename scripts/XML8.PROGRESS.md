# XML8 - the lenient (HTML5-like) parser: progress

Branch `wt/xml8` from `45c6bf98b`. Brief: scripts/waves/wave8/PLAN.md section XML8. Rules:
scripts/waves/house_rules.md. Report: scripts/XML8_2026_10_03.md (date of finishing).

## What exists (read 2026-10-03)

- `core/src/xml_html.rs` (= `azul_core::xml::html`, LENIENT wave 4, report scripts/LENIENT_2026_10_01.md):
  character references (ONE decoder, `CharRefMode`), the ONE encoder (`encode_text` / `encode_attribute`, in
  api.json via autofix - they must stay in THIS file, the autofix tool reads it), `HtmlTokenizer` (a slice
  scanner, no explicit states), `TreeBuilder` + `TreeRules {Xml, XmlFolded, Html}` (streams open / close / text
  into a `TreeSink`), `XmlTreeSink`, `parse_html_into`, `parse_html_nodes`, `outline`.
  Rule knowledge is spread over ~15 `matches!` fns (`is_void_element`, `closes_p`, `is_special`, ...) and
  `close_implied_by` / `document_start_tag` / `before_body_content` code paths with flags.
- Known gaps vs Chrome (LENIENT report): no foster parenting (2 corpus mails: postmark invoice / receipt), a
  simplified adoption agency (`<b>x<p>y</b>z` keeps z bold), no quirks mode (`<table>` always closes `<p>`), no
  head when the document has no head content, `</span>` crosses special elements, CDATA outside foreign
  content is a CDATA token (spec: bogus comment), `plaintext` ends at `</plaintext>`.
- Users: strict loaders `layout/src/xml/mod.rs` (`feed_xml_tokens` -> `TreeBuilder` with `TreeRules::Xml` /
  `XmlFolded`, xmlparser = crates.io RazrFalcon crate, NOT ours - LENIENT decided against a fork: `[patch]` is
  forbidden by the release gate), lenient loaders (`parse_html_string`, `parse_html_to_styled_dom`,
  `Xml::create_from_html`), the paste parser (`layout/src/paste_html.rs` -> `parse_html_nodes`: already
  lenient), AzMail (`examples/azul-mail/src/html.rs` -> `Xml::create_from_html`: already lenient), the e2e
  builder (`is_void_element`, `start_tag_closes`).
- Tests: core/src/xml_html_test.rs (rows vs Chrome), layout/tests/real_mail_html_parses_like_a_browser.rs (the
  corpus tests/mail_corpus/*, Chrome's trees in tests/mail_corpus/outlines, scripts/refci/mail_outlines.py).

## Target design

Files (all `azul_core::xml::html`, re-exported from xml_html.rs so every path stays):
- `core/src/xml_html.rs` - module root: docs, character references, the encoder, `TreeSink`, `XmlTreeSink`,
  `parse_html_into` (the driver), `parse_html_nodes`, `outline`.
- `core/src/xml_html_rules.rs` - THE DATA. Every special case is a table row citing the HTML Living Standard:
  - `ELEMENTS`: one row per known element, sorted, binary-searched: category flags (VOID, FORMATTING, SPECIAL,
    CLOSES_P, HEADING, HEAD_CONTENT, MARKER, IMPLIED_END, BREAKOUT, FOSTER_TARGET ...) and its text mode
    (RCDATA / RAWTEXT / PLAINTEXT). Replaces the `matches!` fns.
  - `SCOPES`: the boundary flag of each scope (default / list item / button / table / select).
  - `START_TAGS`: in-body start tags -> the steps before / as they are inserted (close a p, close the list
    item, pop the current option, reconstruct the formatting elements, insert / insert void / insert
    formatting / insert marker, ignore, adopt an open `a` ...). XML runs the same rows with the HTML-only steps
    off (so the XML conveniences of today are the same rows, not a twin).
  - `END_TAGS`: in-body end tags -> the scope they look in (XML: find in that scope, pop) and the HTML action
    (close in scope, `</p>` without p, heading, adoption agency, `</br>`, any other end tag ...).
  - `TABLE_START_TAGS` / `TABLE_END_TAGS`: the table insertion modes (in table, table body, row, cell, caption,
    column group) as rows: clear back to a context and insert, imply a wrapper (tbody / tr / colgroup), close
    and reprocess, insert in place, ignore. Anything else in a table: foster parenting.
  - `QUIRKS_PUBLIC_ID_PREFIXES` & co: the doctype -> quirks mode table.
- `core/src/xml_html_tokenizer.rs` - `HtmlTokenizer`, a state machine with the spec's states (data, RCDATA,
  RAWTEXT, PLAINTEXT, tag open, end tag open, tag name, before / after attribute name, attribute name, before
  attribute value, quoted / unquoted value, after quoted value, self-closing, markup declaration open, comment,
  bogus comment, doctype, CDATA section). The TREE BUILDER switches its text mode (as in the spec), and allows
  CDATA only in foreign content. Emits `Doctype` tokens (quirks).
- `core/src/xml_html_tree.rs` - `TreeBuilder`: HTML builds into an arena (nodes can move: foster parenting,
  the adoption agency) and replays it into the sink at `finish`; XML modes stream into the sink exactly as
  today. Document phases (initial / before html / before head / in head / after head / in body) as an enum;
  the table insertion modes derived from the stack ("reset the insertion mode appropriately", cached per stack
  entry); the full adoption agency; foster parenting; reconstruct the active formatting elements (Noah's ark);
  foreign content (svg / math: breakout table, self-closing).
- Public API kept: `TreeBuilder::{new, open_elements, start_tag, end_tag, text, comment, cdata, finish}` (same
  signatures), `TreeRules`, `TreeSink`, `XmlTreeSink`, `HtmlTokenizer`, `HtmlToken` (+ `Doctype` variant),
  `parse_html_into`, `parse_html_nodes`, `outline`, `is_void_element`, `start_tag_closes`, the decoder and
  encoder. New Rust-only: `TreeBuilder::{doctype, text_mode_for, in_foreign_content}`, `TextMode`,
  `HtmlTokenizer::{set_text_mode, set_cdata_allowed}`. No api.json change expected.

## Decisions (unattended run)

- D1: own layer in azul-core stays (no xmlparser fork; LENIENT's reason holds). Strict loaders keep xmlparser.
- D2: never compile (house rule). Type-check the core files with `rustc --emit=metadata` on a stub harness (as
  LENIENT did; no codegen, no binary, no target dir). Behaviour checked with a Python mirror (scratchpad, not
  committed: a mirror is a twin) against headless Chrome (scripts/refci/cdp.py).
- D3: XML modes (strict loaders) keep TODAY's behaviour bit for bit: streaming, the simplified misnesting
  (`pending_close`), the same implied end tags (now read from the same rule rows with HTML-only steps off).
- D4: HTML mode follows DOMParser (text/html, scripting off): quirks mode when there is no doctype, the head
  always exists, comments are dropped (not nodes; text across a comment stays one run).
- D5: `select` keeps today's rules (option / optgroup close their own kind); the 2025 customizable-select parser
  change and "in select" modes are not modelled. `template` content is parsed as body content. Script data
  escape states are not modelled (script = raw text).

## DONE

- `bd4963f9d` progress file + design.
- Chrome probe written (UNCOMMITTED on purpose, in the worktree): `scratchpad/xml8/chrome_outline.py` (stdin JSON
  list of snippets -> Chrome outlines), `scratchpad/xml8/rows.py` (the ~110 candidate rows), its output
  `scratchpad/xml8/rows_chrome.txt` (Chrome's body / document outline + compatMode per row). Re-run:
  `python3 scratchpad/xml8/rows.py > scratchpad/xml8/rows_chrome.txt`.
  Findings: no doctype => BackCompat (quirks: `<p>a<table>` nests the table in the p); HTML 4.01 Transitional
  with a system id / XHTML 1.0 Transitional / about:legacy-compat => CSS1Compat; the head always exists;
  `<body id=c>` merges missing attributes; foster parenting puts text / div / span / b / p / `input type=text`
  before the table (`input type=hidden`, `form`, `style` stay in it); a second `<table>` in a row closes the first;
  `<col>` implies a colgroup; `</span>` with a div inside is ignored; `</div title=">">` ends at the second `>`;
  CDATA outside svg is a bogus comment (`a<![CDATA[x>y]]>b` -> "a" "y]]>b"); `<svg><font color>` breaks out,
  `<svg><font>` does not; `<image>` is img; `<noframes>` before the body goes into the head; `<noscript>` (scripting
  off) is markup; `<select><div>` keeps the div (customizable select: Chrome's current parser); `<frameset>`
  replaces the body (not modelled: D5).

- `16b9534e0` RED: adoption agency rows + head / body rows -> Chrome's trees (core/src/xml_html_test.rs).
- `f034b9073` RED: new tests (foster parenting, quirks, table modes, any other end tag, tokenizer states, body
  start tags, foreign content) in core/src/xml_html_test.rs.
- `60f112c71` RED: layout/tests/real_mail_html_parses_like_a_browser.rs - all 18 corpus mails exact (FOSTER_PARENTED
  exclusion + elements_only + the weaker test removed). Outlines regenerated: identical to the committed ones.

## IN PROGRESS

- (nothing half-edited)

## NEXT

1. (done: Chrome probe, see DONE.)
2. (done: RED.)
3. GREEN (next: write core/src/xml_html_rules.rs, the data tables, not yet wired): xml_html_rules.rs, xml_html_tokenizer.rs, xml_html_tree.rs, wire in xml_html.rs.
4. Type-check harness; Python mirror vs Chrome (rows + corpus); fix.
5. Corpus test: the two foster-parented Postmark mails join the exact-tree test.
6. Report.

## Open questions

(none)
