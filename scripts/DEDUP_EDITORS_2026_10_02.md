# DEDUP review - AREA: EDITORS (mail / notes / writer / tasks / review / contacts / calendar + their widgets)

Reviewed commit: 53c978b33 (branch fix/input-bugs-2026-09-19), range 112cf8342..HEAD. Read-only review; nothing built.

(Status: COMPLETE.)

Summary
- ONE rich-text editor: yes. Three copies (AzNotes doc.rs/editor.rs/markdown.rs, AzWriter ir.rs, AzMail editor.rs +
  compose.rs MailDoc) over the same engine contract; AzNotes is the most complete and the base (A1, A2). Tasks /
  Calendar / Contacts use a plain TextArea for notes.
- New bugs found while comparing: AzMail typing wipes a paragraph's bold/links from the model (A3.1); Ctrl+B/I/U over a
  selection does nothing in AzMail and AzWriter (A3.3); AzNotes mints note FILE ids with the deterministic `Uuid::v4`
  (B1, data loss); AzCalendar refuses `"Last, First" <a@b>` attendees (B14); AzCalendar's navigator splits the shown week
  because DatePicker is Sunday-first (B25); AzCalendar editor and AzWriter lose edits on close (B29).
- Known items: Ctrl+B-not-reported confirmed and widened (inline formatted paste loses formats too, D1); 300x150 img hole
  confirmed with its engine root (A3.8); stale/ Sent bug is ALREADY FIXED in code (A3.9); every helper twin named by the
  TASKS / NOTES / MAIL2 / SEND / CAL3 / EDITOR reports verified (status table before the task list).
- 33 duplication findings (B), 7 reuse candidates (C), 17 api.json items (D), 8 placement items (E), 7 other (F),
  prioritized list at the end.

## A. The central question: one rich-text editor for Mail compose / Notes / Writer (/ Tasks notes)?

Answer: YES. There are three independent rich-text editors over the same engine contract (Path 2:
`TextChanged` -> `get_unsynced_text_edits` -> model; `DocumentEdit` -> `get_document_edit_clone` -> model ->
`mark_document_edit_applied*`; `get_document_selection` for selection commands; `toggle_text_format` /
`reset_editor_content`). They share ~60% of their logic, each has bugs the others fixed, and none is reachable
by a fourth app (Tasks, Calendar, Contacts all use a plain `TextArea` for notes). AzNotes' copy is the most
complete and the one to promote; AzWriter's `apply_operation`+undo stack and AzMail's HTML writer are the
parts to graft onto it.

### A1. The three copies, line by line (all read in full; file:line)

| Aspect | AzNotes (`examples/azul-notes/src/doc.rs` 1399 l, `editor.rs` 1078 l, `markdown.rs` 1190 l) | AzWriter (`examples/azul-writer/src/ir.rs` 1390 l, `lib.rs` 446-760, `ribbon_ui.rs` 20-100) | AzMail (`examples/azul-mail/src/editor.rs` 719 l, `compose.rs` 245-460 + 660-735, `ui_compose.rs` 760-870) |
|---|---|---|---|
| Run type | `doc::Run {text,bold,italic,underline,strike,code,link}` doc.rs:17 | `ir::IrRun` - the SAME 7 fields, same order, ir.rs:48 | `compose::Run {text,bold,italic,underline,link}` compose.rs:247 (no strike, no code) |
| Block model | FLAT `Vec<Block{kind,runs}>`; `BlockKind` = Paragraph, Heading(1-6), Bullet(indent), Numbered(indent), Check{indent,checked}, Quote, Code{lang}, Rule, Image{src,alt} (doc.rs:155) | TREE-ish `Vec<IrBlock>`: Paragraph{style Body/Heading/Quote/CodeBlock, align}, List{ordered, items} (ONE level, no nesting, no check items), Table (plain-string cells), Rule, PageBreak (ir.rs:12-100) | the live `Dom` IS the model (Path 2, `apply_to_dom`); `MailDoc {blocks: Vec<Block{quote: u8, kind: Paragraph/Bullet/Numbered, runs}>}` only for load/save (compose.rs:267-306) |
| Rendering | one host child per block, one span per run (`editor.rs:63 run_dom`, :130 `with_runs`); `li` directly under the host, numbering by `counter-reset` | one host child per block, one node per run (`ir.rs:593 run_to_dom`, strong/em/span+css) - same strategy as Notes | nested `blockquote`/`ul`/`ol`; a run is up to FOUR nested elements `<a><b><i><u>text` (`editor.rs:135 run_dom`), so a run index is NOT a child index |
| Typing sync | block text diff (`doc.rs:513 text_diff`, :686 `sync_block_text`): prefix/suffix keep formats; inserted text continues the LEFT run (browser rule) or the typing style | same diff (`ir.rs:1098 sync_block_text`) but the template is `runs.get(covered.start)` = the RIGHT-hand run at a run boundary (typing after `**bold**` before plain text comes out plain; Notes fixed it, test `typing_at_the_end_of_a_bold_run_continues_it..` doc.rs:1101); no typing style | **BUG** - see A3: replaces the block's children with one plain text node |
| Typing style (Ctrl+B at a caret) | mirrored app-side: `editor.rs:505 Typing`, `EditorState::typing_for` :523, fed into `sync_block_text(.., typing)` | none (engine paints it, the IR never learns it) | none: `toggle_text_format` only (ui_compose.rs:826-831), engine paints, model never learns (the known gap, A3) |
| Selection formatting | `Doc::toggle_format` doc.rs:940 (B/I/U/S/code), `set_link` :989, `has_format` :968 | `ir::toggle_format_range` ir.rs:1028 (B/I/U/S - Strike has no UI), paragraphs only, list items cannot be formatted | `editor::wrap_selection` editor.rs:446 (engine `WrapRange` on the Dom; no toggle-off: Bold twice nests `<b><b>`) |
| Run helpers | `flatten` :310, `push_run` :316, `normalize_runs` :330, `floor_char_boundary` :339, `split_at_byte`+`isolate` :348-386, `split_runs`, `slice_runs` | `flatten_runs` :139, `push_run` :143, `normalize_runs` :862, `floor_char_boundary` :854, `split_runs_at_range` :982 - IDENTICAL bodies modulo names (diffed), `isolate`/`split_runs_at_range` diverge: Notes floors a cut to a char boundary, Writer skips a non-boundary cut | n/a (Dom) |
| Block commands | set/toggle kind (H1-H6, lists, check, quote, code), indent/outdent with bounds, rule, check toggle, insert_after, number_of, normalize (doc.rs:558-1060) | `set_block_style` (Body/H1-H3 via the style gallery; index 2 AND 4 both map to Heading(1), ribbon_ui.rs:28-33) | `toggle_list` editor.rs:532 (paragraph <-> ul/ol), `insert_link` :583 (no unlink) |
| Markdown shortcuts as you type | yes (`shortcut_in`/`typed_shortcut` doc.rs:423-512) | no | no |
| Structural edits | Split / Merge / host-level ReplaceChildren (multi-block paste) mirrored into the flat model (editor.rs:610-686), acked WITHOUT an inverse | Split / Merge only (`ir::apply_operation` ir.rs:869, keyed on the resume path, returns the inverse); ReplaceChildren (cross-block delete, rich paste) => "not mirrorable" eprintln (lib.rs:604-616) | every op through the engine's own `DocumentChangeset::apply_to_dom` (editor.rs:401-431), acked with the inverse |
| Undo | engine text undo; structural edits not undoable | app undo/redo stacks of inverse ops (lib.rs:89-90, 694-725) on the QuickAccess buttons - a SECOND undo stack beside the engine's (it was also handed the inverse), the buttons never call `reset_editor_content` | engine's (inverse handed over) |
| Keys handled by the app | B/I/U, Shift+X strike, E code, 0-3 headings, Shift+7/8/9 lists, Enter ticks a check, Enter in code / empty list item, Backspace at a block start, Tab/Shift+Tab (editor.rs:882-1030) | NONE (no VirtualKeyDown on the editor) | only Ctrl+Enter / Ctrl+S (ui_compose.rs:725-743) |
| Serialization | Markdown both ways, canonical writer with escaping, `<u>`, fences that grow, nested list indents, front matter (markdown.rs) + PDF (`print_dom`) | Markdown both ways (ir.rs:157, :401) - naive writer: no escaping (text `*x*` re-reads as italic), underline dropped, flat lists, H4-6 render as H3 (ir.rs:543); plus a SECOND writer `document::dom_to_markdown` (document.rs:540) from the Dom; DOCX import (wire JSON); PDF | HTML (`MailDoc::to_html`, nested `<blockquote type=cite>`) + text/plain with `> ` quotes (compose.rs:365-446); NO HTML -> model reader, so a reopened draft loses bold/italic/links (MAIL2 report sec. 7) |
| Links | in the model; Ctrl/Cmd+click opens (`on_link_click` editor.rs:114); pasted `<a>` loses its href (`collect_runs` :452 ignores `A`) | rendered as a coloured span, no `<a>`, nothing opens (ir.rs:633) | href smuggled in a CLASS `azmail-href:<url>` (editor.rs:35, :153, :324) because NodeData has no attribute getter; a pasted link keeps only its text |

Most complete: **AzNotes** (formats 5/5, blocks 9 kinds, nested lists, check items, typing-style mirror,
shortcuts, the correct left-run rule, a canonical Markdown writer, 20+ model tests). AzWriter adds tables /
alignment / page breaks / DOCX import and the only app-side undo; AzMail adds quote depth and the HTML + text/plain
writers.

Confirmation: read all three files; `diff <(sed -n 316,345p examples/azul-notes/src/doc.rs) <(sed -n 143,155p
examples/azul-writer/src/ir.rs)` style comparisons of push_run / normalize_runs / floor_char_boundary (identical
bodies), `text_diff` vs Writer's inline prefix/suffix (Notes adds `new.is_char_boundary` guards - redundant but
harmless), template choice (diverged as above).

### A2. Proposed shared widget: `RichTextEditor` (layout/src/widgets/rich_text_editor.rs + core model in
`layout/src/widgets/rich_text/doc.rs`, markdown/html in `layout/src/widgets/rich_text/{markdown,html}.rs`)

- Model (plain data, `repr(C)` for api.json): `RichTextDoc { blocks: RichBlockVec }`, `RichBlock { kind:
  RichBlockKind, runs: RichRunVec, quote_depth: u8, align: RichAlign }`, `RichBlockKind` = Paragraph |
  Heading(u8) | Bullet(u8) | Numbered(u8) | Check{indent, checked} | Quote | Code{lang} | Rule | Image{src, alt} |
  PageBreak | Table(RichTable) (the union of the three; `quote_depth` from Mail, `align`/PageBreak/Table from Writer),
  `RichRun { text, formats: RichFormats (bitflags B I U S CODE), link: OptionString }`. Start from AzNotes'
  `doc.rs` (rename `Doc` -> `RichTextDoc`), keep its tests.
- Edits (pure, testable, from doc.rs + ir.rs): `sync_block_text(i, text, typing)`, `split_block`,
  `merge_into_previous`, `replace_with` (paste), `toggle_format(range)`, `set_link(range, Option)`, `set_kind` /
  `toggle_kind`, `indent`, `toggle_check`, `apply_shortcut` - each RETURNING the inverse edit (Writer's
  `apply_operation` idea) so the widget owns ONE undo/redo stack.
- Widget: `RichTextEditor::create(doc: RichTextDoc) -> RichTextEditor`, builders `with_toolbar(RichToolbarSet)`
  (which buttons: formats, headings, lists, check, quote, code, link, rule, indent), `with_markdown_shortcuts(bool)`,
  `with_placeholder`, `with_id(String)`, `with_theme`, `with_min_height`; `dom() -> Dom` renders the flat
  one-child-per-block host (Notes' `host_dom`) + optional toolbar. Callbacks: `with_on_change(RefAny,
  RichTextEditorOnChangeCallbackType(RefAny, CallbackInfo, RichTextDoc) -> Update)` (fires after every synced
  edit), `with_on_link_open(..., url: String)`, `with_on_image_request(...)` (Notes' images), `with_on_key(...)`
  (app keys first: Mail's Ctrl+Enter send). State helpers on `CallbackInfo`-side: `RichTextEditor::
  get_doc(info, host)`, `set_doc(info, host, RichTextDoc)` (calls `reset_editor_content`), `apply_command(info,
  host, RichCommand)` for ribbon buttons (Mail / Writer ribbons stay app-owned and just call this).
- Serialization (free functions, also exported): `RichTextDoc::from_markdown` / `to_markdown` (Notes' writer),
  `to_html` / `to_plain_text` (Mail's, with `quote_depth`), `from_html` (NEW - via the engine's `paste_html`
  parser, closes the reopened-draft gap), `word_count`, `plain_text`, `preview`.
- What each app keeps: AzNotes - notebooks, front matter, history, image storage (the editor asks via a callback);
  AzMail - the ribbon, From/To/Subject, quote header/forward block builders (`MailDoc::reply_quote` / `forward_quote`
  become functions producing a `RichTextDoc` with `quote_depth`), MIME (`compose::outgoing`), Ctrl+Enter;
  AzWriter - pagination / the virtual-view page split (`document.rs`), DOCX import, the ribbon/style gallery;
  AzTasks / AzCalendar / AzContacts - swap `TextArea` notes for `RichTextEditor` with a minimal toolbar
  (B/I/lists/link) and `to_markdown` storage (Tasks' `notes` field is a String today, Markdown keeps it a String).
- Effort L (2-3 agent-days: model + tests moved verbatim first, widget second, apps one per commit). Risk: medium -
  the Mail model change from Dom to flat blocks loses arbitrary pasted HTML structure (acceptable: MailDoc already
  flattens on Send). Benefits: Notes, Mail, Writer, Tasks, Calendar, Contacts (+ Meet chat, Review comments later).

### A3. Editor bugs found while comparing (each verified by reading the engine side too)

1. **AzMail typing flattens a paragraph's formatting in the model (NEW, high).** `examples/azul-mail/src/editor.rs:367-399`
   `sync_text` treats `DocumentTextEdit.node` as a text node and, when it is an element, replaces the element's
   children with ONE plain text node (`node.children = vec![text].into()`, :386). But the engine keys every text edit
   to the caret's BLOCK ELEMENT (`layout/src/text_block.rs:495-522 edit_element`, overlay keyed by IFC root
   `layout/src/overlay.rs:446-466`, `LayoutWindow::unsynced_text_edits` window.rs:3061 returns the block's whole
   flattened text). So the first keystroke into `<p>ab<b>c</b><a class=azmail-href:..>d</a></p>` makes the model
   `<p>abcXd</p>`: bold and the link vanish on the next rebuild (any ribbon command, Enter, Send's `host_to_doc`).
   AzNotes (`editor.rs:544-588`, block diff) and AzWriter (`lib.rs:446-500`, `rel` empty -> `ir::sync_block_text`)
   handle this correctly. Fix: diff the block text into the block's runs (exactly AzNotes' `sync_block_text`), i.e.
   the shared editor. Effort S as a stop-gap (port `text_diff` + run splice onto the Dom children), L via A2.
   Test to write first: `typing_into_a_formatted_paragraph_keeps_its_bold_and_link` (AzMail lib tests, Dom-only).
   Likely knock-on (derived, not run): after the flattening the model's child indices no longer match the laid-out
   DOM, so the next Enter's `apply_to_dom` (editor.rs:420) can fail and the edit is never acked.
2. **Known item, VERIFIED: AzMail Ctrl+B at a collapsed caret is never reported to the app.** The engine's
   `DefaultAction::ToggleTextFormat` (`layout/src/default_actions.rs:146-163`) sets the typing style
   (`LayoutWindow::toggle_text_format` window.rs:12552-12640); typed text lands in a formatted overlay run, but
   `DocumentTextEdit` (`core/src/selection.rs:1581`) is `{node, text, revision}` - no runs - and AzMail has no
   VirtualKeyDown for B/I/U (`ui_compose.rs:725-743` handles only Return/S). AzNotes works around it by mirroring
   (`editor.rs:505-536 Typing`, its own key handler :882). Root fix (engine, M): `DocumentTextEdit` gains
   `formats: TextFormatSpanVec` (byte ranges + TextFormat set of the inserted text) or a `TypingStyleChanged` focus
   event; then AzNotes' mirror is deleted too. Same gap in AzWriter (no key handler at all). WIDER than the ledger
   says: an inline formatted PASTE takes the same road (`LayoutWindow::paste_inline_formatted` window.rs:4569-4620
   writes styled runs to the overlay via `update_text_cache_after_edit`), so pasting `a <b>b</b> c` into a paragraph
   loses the bold in all three editors' models (AzNotes' mirror covers typing only).
3. **NEW: Ctrl/Cmd+B / I / U over a SELECTION does nothing in AzMail and AzWriter.** The engine deliberately leaves a
   selection to the app (`toggle_text_format` returns false for a range or a cross-block selection, window.rs:12583-12613,
   doc comment :12552-12560), and neither app handles the key (AzWriter has no VirtualKeyDown on its editor;
   AzMail's handler returns DoNothing for B). Only the ribbon buttons work. AzNotes handles it (editor.rs:905-930).
   Fix: S per app now (call the existing button action from a VirtualKeyDown arm, `prevent_default`), or free with A2.
4. **NEW: AzMail Bold/Italic/Underline on a selection can only wrap, never unwrap**: `wrap_selection`
   (editor.rs:446-481) always inserts a `WrapRange` of `<b>`; pressing Bold twice nests `<b><b>`; there is no
   "remove bold" path. AzNotes/AzWriter toggle (all-on -> off).
5. **NEW: AzWriter's ribbon B/I/U pressed state is a blind flip** (`ribbon_ui.rs:74-97 state.bold = !state.bold`),
   independent of the caret's run; AzNotes derives pressed state from the caret block. Style gallery maps index 2 and 4
   both to `Heading(1)` (ribbon_ui.rs:28-33) and `IrParaStyle::Heading(4..=6)` renders as `<h3>` (ir.rs:543).
6. **NEW: AzWriter has two undo stacks.** The app keeps `undo_stack`/`redo_stack` of IR inverses (lib.rs:89-90,
   619-620, 694-725) AND hands the same inverse to the engine (`mark_document_edit_applied_with_inverse`, :626), so
   Ctrl+Z (engine) and the QuickAccess Undo (app) undo different histories; the app's undo never calls
   `reset_editor_content`, so the engine overlay is not told the DOM changed. One stack (the engine's, or the
   widget's in A2) - M.
7. **AzWriter dead code**: `if !applied {}` (lib.rs:490-491, left by the comment strip 43cd7bb80); `set_run_text`
   branch (lib.rs:479) is unreachable in practice (edits are keyed to the block element, `rel` is always empty for a
   paragraph). S.
8. **Known item, VERIFIED: an unfetched `<img>` without width/height is a 300x150 hole.** AzMail's sanitizer emits
   `<img src>` for every web picture as soon as "download pictures" is pressed, before the bytes arrive
   (`html.rs:372-395`, width/height only when the mail had them). The XML loader makes it a `NullImage` tagged with
   its src (`core/src/resources.rs:2043-2055 source_tag`); until the app caches it (`overlay.rs:700-715`) it has no
   size, and `layout/src/solver3/sizing.rs:609-624` gives a sizeless replaced element the CSS 300x150 fallback - a
   failed download stays a hole forever. Browsers render a not-(yet)-available `<img>` with no size as nothing (or
   its alt text inline). Fix (engine, S-M): in sizing.rs, a `NullImage` with a `source_tag` (an unresolved markup
   image) sizes 0x0 (or as its alt text) instead of 300x150; keep 300x150 for render-callback images. RED:
   `layout/tests/an_unresolved_img_from_markup_takes_no_space.rs`. App-side alternative: emit the `<img>` only after
   the bytes are cached (re-sanitize per finished download).
9. **Known item, ALREADY FIXED in the tree (close the ledger line after the AzMail suite runs):** "locally sent
   mails moved to stale/ on the first IMAP sync of Sent". `sync::plan_folder` adopts a `uidvalidity == 0` folder
   without moving it aside (`examples/azul-mail/src/sync.rs:184-188`, commit a9e67753b), and a real renumbering
   carries `uid >= send::LOCAL_UID_FLOOR` entries across (:318-343, 6420d63ab); tests
   `mail_sent_from_here_stays_in_sent_when_the_first_sync_adopts_the_folder` (:815) and
   `mail_sent_from_here_survives_the_server_renumbering_the_folder` (:833). Not run tonight (no compile). Remaining
   (unverified) risk: a server that auto-files relayed mail into Sent (Gmail) will show the message twice (local
   UID + server UID); dedupe by Message-ID in `sync.rs` when adopting.

## B. Duplication (helpers, storage glue, dates, ids) - each confirmed by reading both sides

B1. **Random record ids: three copies, and AzNotes uses the WRONG mint (data-loss bug).**
- `examples/azul-calendar/src/event.rs:698-724` (`new_event_id` + `random_seed`) and `examples/azul-tasks/src/state.rs:170-194`
  (`new_id` + `random_seed`): IDENTICAL bodies (diffed by eye line for line: RandomState + counter + nanos + pid ->
  `Uuid::from_seed`). Third copy, different mixing: `examples/azul-appkit/src/data.rs:76-110` (`new_uuid` +
  `uuid_from_words`), used by AzContacts. `is_uuid` (appkit data.rs:114) vs AzCalendar `is_event_id` (event.rs) same check.
- **BUG**: `examples/azul-notes/src/jobs.rs:296` mints a NOTE FILE id with `azul::uuid::Uuid::v4()`, which is a
  deterministic per-process marker sequence (`layout/src/uuid.rs:1-50`, "the first Uuid::v4 in every process is
  00000000-0000-4000-..."; AzCalendar's test event.rs:1000-1012 documents the overwrite it caused there). The first
  note made in each run can therefore get the id of the first note made in the previous run -> same
  `notes/<notebook>/<uuid>.md` key, same `.history/<uuid>/`: the new note overwrites the old one. Same misuse outside
  this area: azul-photo lib.rs:167, azul-show commands.rs:50, azul-videocut lib.rs:2839/2858/2874.
- Proposal: export `Uuid::random() -> String` (layout/src/uuid.rs, module `uuid`; std: RandomState+clock seed into
  `from_seed`, the existing helper; wasm: documented as needing a seed) and `Uuid::is_valid(text) -> bool`; delete the
  three app copies; switch AzNotes (S, urgent) and the other three apps. Effort S. Risk low. Benefits: Notes, Tasks,
  Calendar, Contacts, Photo, Show, VideoCut, Sheets (uses the `uuid` crate instead, storage.rs:25).

B2. **Atomic file write: six copies.** `examples/azul-calendar/src/event.rs:627-639`, `calendars.rs:269-280`,
`tasks.rs:88-99`, `settings.rs:58-70` (four copies INSIDE one app: write `.x.tmp`, rename, remove on failure) +
`examples/azul-mail/src/store.rs:75-100 write_atomic` (adds fsync/`durable`) + `examples/azul-storage/src/local.rs:58
write_atomically` (the library one, used by LocalDrive). Calendar also does these writes synchronously inside
callbacks. Proposal: AzCalendar moves onto `azul_storage::LocalDrive` + `azul_appkit::files` jobs on a Thread (as
AzNotes / AzTasks / AzContacts do) - its own report lists this as next step. M. Benefits: Calendar, Mail.

B3. **AzMail's `store::LocalFolder` is a second LocalDrive.** `examples/azul-mail/src/store.rs:38-190`
(`is_valid_key`, `put/get/delete/size_of/move_prefix/folders`, `write_atomic`) vs `examples/azul-storage/src/local.rs`
(`put/get/delete/head/rename/list/delete_folder`, key checks in `key.rs`). Same semantics (keys, atomic put, prefix
move); LocalFolder lacks listing pages and S3. MAIL2 report sec. 4 names the swap as MAIL1's next step. M. Benefit:
Mail gets S3 for free (the Azlin cloud-storage split).

B4. **UTC ISO-8601 formatter twice.** `examples/azul-mail/src/message.rs:9-13 rfc3339_utc(i64)` (chrono) vs
`examples/azul-storage/src/time.rs:102-111 iso8601(u64)` (civil arithmetic); same output `YYYY-MM-DDTHH:MM:SSZ`;
diverged only at the edges (Mail returns "" out of range, storage takes u64). Proposal: Mail depends on azul-storage
(it will for B3) and drops its copy. S.

B5. **days_in_month x5, add_months x2.** `examples/azul-calendar/src/rrule.rs:169-178` and
`examples/azul-tasks/src/recur.rs:287-296` are IDENTICAL (chrono `pred_opt` trick; only `== 12` vs `>= 12`);
`examples/azul-storage/src/time.rs:48`, `layout/src/widgets/date_picker.rs:422` (const fn, u32), azul-calculator
`datecalc.rs:32` are table versions. `add_months`: rrule.rs:181 returns `(year, month)`, recur.rs:309 clamps the day -
diverged on purpose (RRULE skips a missing day, a task clamps it), both correct for their domain. Weekday/month names:
`rrule.rs:746 weekday_name` / `:758 month_name` / `:780 ordinal`, `recur.rs:318 weekday_name` (file codes) / `:332
weekday_short`, `azul-mail listing.rs:63 weekday_name`. Proposal: one `pim_dates` module in a shared crate (see C1)
holding days_in_month, ymd_clamped, add_months{_clamped,_strict}, weekday/month names, ordinal. S.

B6. **Recurrence twice.** AzCalendar `rrule.rs` (1163 l, RFC 5545 subset: FREQ/INTERVAL/COUNT/UNTIL/BYDAY nth/
BYMONTHDAY/BYMONTH/WKST, describe(), parse/write) vs AzTasks `recur.rs` (587 l, own `Repeat{every, unit, days, day,
from_completion}`, month-end clamping, describe labels). Not a byte twin; overlapping features (every N weeks on days,
monthly on day N, yearly, weekdays, human description). AzTasks cannot export/import VTODO RRULE (its report lists
CalDAV VTODO as left); AzCalendar cannot repeat "after completion". Proposal: one `Recurrence` type with
`to_rrule/from_rrule` + `from_completion` + `clamp_month_end` flags in the shared PIM crate; both editors (Calendar's
repeat segments editor_ui.rs, Tasks' RecurrenceEditor in detail.rs) then become one `RecurrenceEditor` widget
(layout/src/widgets/recurrence_editor.rs: `create(Recurrence)`, `with_on_change(RefAny, cb(Recurrence))`). L.

B7. **Relative-date labels: four schemes for one concept.** `examples/azul-mail/src/listing.rs:82-131`
(`date_group`: Today / Yesterday / weekday / Last Week / Two/Three Weeks Ago / Last Month / Older; `list_date`
"21:12" / "Wed 21:12" / "2026-09-28"), `examples/azul-notes/src/model.rs:721-773` (`date_bucket`: Today / Yesterday /
Previous 7 days / Previous 30 days / "September 2026"; `short_date` "09:14" / Yesterday / "Tue" / "Sep 30" /
"2025-09-30"; `long_date`), `examples/azul-tasks/src/model.rs:1066-1087` (`day_label` / `day_heading`: Today /
Tomorrow / Yesterday / "Sat 3 Oct" / "Saturday 3 October"), AzCalendar view titles (views.rs). All hard-code English
chrono formats; `CallbackInfo::format_date` / `format_datetime` (ICU, already in api.json) is used by NO app. Each
list widget (`MessageList` rows, `ToDoBar`) takes a pre-formatted `String`. Proposal: `RelativeDate` helper in the
shared crate (`relative_day(then, today) -> RelativeDay {Today, Yesterday, Tomorrow, ThisWeek(Weekday), WeeksAgo(n),
LastMonth, Month(y,m), Older}` + `group_label` / `row_label(style)` rendered through the ICU localizer); MessageList
gains `MessageList::with_date_groups(bool)` using it for its section headers. M. Benefits: Mail, Notes, Tasks,
Calendar, Drive.

B8. **TextArea text extraction re-implemented 5x because `TextAreaState::get_text` is not exported.**
`examples/azul-tasks/src/detail.rs:622-628 area_text`, `examples/azul-calendar/src/editor_ui.rs:664-669`,
`examples/azul-contacts/src/ui.rs:1798`, azul-show views.rs:46, azul-widgets lib.rs:1266 all do
`text.as_slice().iter().filter_map(char::from_u32).collect()` - the body of `TextAreaState::get_text`
(`layout/src/widgets/text_area.rs:364-372`), which exists but is missing from api.json (TextInputState::get_text IS
exported). API item: export `TextAreaState.get_text` (module widgets). S.

B9. **`box_str` x3 and `reborrow_info` x3 (both unnecessary).**
- `box_str(&BoxOrStaticString) -> &str` unsafe deref: `examples/azul-mail/src/editor.rs:57-66`,
  `examples/azul-notes/src/editor.rs:416-425`, `examples/azul-writer/src/document.rs:36-43` - identical. Cause:
  `BoxOrStaticString` has no functions in api.json. API item: `BoxOrStaticString.as_str()` (module css) or, better,
  `NodeType::text() -> OptionString` / `Dom::text_content() -> String` (module dom). S.
- `reborrow_info(&CallbackInfo) -> CallbackInfo` field-by-field copy: `examples/azul-notes/src/ui.rs:1237-1245`,
  `examples/azul-writer/src/lib.rs:630-638`, azul-sheets lib.rs:2546. `CallbackInfo` derives Copy in api.json
  (`derive: [Debug, Copy, Clone]`, layout/src/callbacks.rs:1371) and AzShow already passes `*info`
  (azul-show commands.rs:342). Delete all three, use `*info`. S. Also: `Pdf::from_dom_in_callback` takes
  `callback_info: CallbackInfo` BY VALUE in api.json while the Rust fn takes `&CallbackInfo`
  (dll/src/desktop/extra/pdf/mod.rs:140) - should be `ref` (semantics, S).

B10. **Command-line skeleton copied per app although `azul_appkit::args` exists.** The same loop (`split_once('=')`,
`let mut value = |what| ..`, `--size WxH` via `split_once('x')`, help-as-Err, unknown flag refused) in
`examples/azul-mail/src/args.rs:80-160`, `azul-notes/src/args.rs:68-140`, `azul-writer/src/args.rs:54-140`,
`azul-tasks/src/args.rs:77-155`, `azul-calendar/src/args.rs:141-210` (and sheets/show/photo/videocut/drive/meet
outside this area). `examples/azul-appkit/src/args.rs:114-300` (`AppSpec`, `AppArgs::parse`, Theme/ModePref enums)
does exactly this and only AzContacts uses it. Theme/Mode enums are re-declared in Mail (args.rs:24-46), Tasks
(:35), Calendar (:90); Notes keeps them as Strings. Proposal: move the five apps onto `AppArgs` + `AppSpec { extra
flags }` (appkit needs an "extra switches" hook for --note / --view / --date / --open). M. Risk: E2E scripts pass
`--data`, appkit spells `--data-dir` - keep `--data` as an alias.

B11. **Data root + settings file: five conventions.** Root: Mail `<data>/AzMail` (`account.rs:299-305`,
`AZMAIL_DATA`), Notes `<data>/AzNotes` (`lib.rs:322-333`, `AZNOTES_DATA`), Calendar `<data>/AzCalendar`
(`event.rs:99 APP_DIR`, `AZCAL_DATA`), Tasks `<data>/Azlin` (`lib.rs:408-420`, `AZTASKS_DATA`), Contacts `<data>/Azlin/
contacts` (appkit `data.rs:26 data_root`, `AZLIN_DATA`). Settings: Notes `aznotes-settings.txt` key=value
(`lib.rs:167-240`), Calendar `settings.txt` key=value (`settings.rs`), Tasks versioned JSON `tasks/settings.json`
(`model.rs:485`), Contacts appkit `contacts/settings.json`, Mail none for theme/mode. The Azlin storage design (memory:
per-user S3 folder) wants ONE root (`Azlin/<app>/...`). Proposal: all apps use `azul_appkit::data::data_root` +
`app_key(app, name)` + `AppSettings`; migrate old folders once on start. M. Risk: existing user data paths (dev only
tonight).

B12. **Two incompatible task stores claim the same layout.** `examples/azul-calendar/src/tasks.rs:1-6` says "the
layout tasks/<list>/<task-uuid>.json the tasks of every azul app share" but writes `format: "azcalendar.task"` under
`<data>/AzCalendar/tasks/default/` and refuses anything else (`from_json` :70-75); AzTasks writes `format:
"aztasks.task"` under `<data>/Azlin/tasks/<list>/` (`model.rs:15-60`) and AzMail's To-Do bar keeps tasks in memory for
one run. Three To-Do bars, three task sources. Proposal: AzTasks' `model.rs` (Task, files, keys) moves into the
shared PIM crate; AzCalendar and AzMail read/write the same files through it (and the same data root, B11). M.
Benefits: Tasks, Calendar, Mail (the Outlook To-Do bar finally shows real tasks).

B13. **Debug-server clients per E2E script** (NOTES/DIALOGS/SMALLAPPS reports): `scripts/aznotes_e2e.py`,
`scripts/aztasks_e2e.py`, `scripts/azmail_e2e.py`, `scripts/azcalendar_e2e.py`, `examples/azul-mail/scripts/sync_e2e.py`
each carry a client while `scripts/azlin_e2e.py` (SMALLAPPS) is the shared driver. Not re-diffed here (outside Rust
scope, reported by three agents). S per script.

B14. **E-mail address checks: three rules, one concept (diverged -> visible inconsistencies).**
- `examples/azul-mail/src/account.rs:209-222 is_email`: exactly one `@`, NO dot required (accepts `x@localhost`, needed for
  test servers), rejects `..`, controls.
- `examples/azul-calendar/src/event.rs:241-257 is_email`: dot REQUIRED in the domain, rejects `< > , ; "`.
- `examples/azul-contacts/src/contact.rs:326-331` (inline in the form check): dot required, but `split_once('@')` lets
  `a@b@c.de` through (second `@` lands in the domain).
- Address LINE splitting: `examples/azul-mail/src/compose.rs:89-134 split_addresses` (quote- and angle-aware) +
  `bare_address` :137 + `same_address` :147 vs `examples/azul-calendar/src/editor.rs:558-579 parse_attendees`
  (naive `split(',' ';' '\n')`): **BUG** - an attendee typed or pasted as `"Lovelace, Ada" <ada@example.org>` (what
  AzMail's own To line and Outlook produce) is split at the comma and refused as "not an e-mail address".
- Proposal: one `mail_address` module in the shared PIM crate: `split_address_line`, `parse_mailbox(entry) ->
  {name, address}`, `is_email(strict: bool)`, `same_mailbox`; AzCalendar's attendee field and AzMail's To/Cc/Bcc become
  the same `TokenInput` (C3). S for the module, the Calendar bug S.

B15. **vCard (RFC 6350) and iCalendar (RFC 5545) content-line code twice.** `examples/azul-contacts/src/vcard.rs:181-365`
(`unfold`, `fold`, `escape_text`, `unescape`, `split_unescaped`, `parse_line`, `quote_param`, `split_param_values`) vs
`examples/azul-calendar/src/ics.rs:46-192` (`ContentLine`, `unfold`, `fold`, `escape_text`, `unescape_text`,
`parse_line`, `param`). `fold` is the same algorithm with renamed variables (diffed: 75 octets, CRLF+space, never inside
a char); `unfold` diverged (vCard keeps inner empty lines, ICS drops them); `escape_text` identical modulo arm order.
~150 lines twice. Proposal: `content_line.rs` (fold/unfold/escape/params/`ContentLine`) in the shared PIM crate; AzMail
needs it next (meeting invites = `text/calendar` parts, contact cards = `.vcf` attachments). S-M.

B16. **"Every word, ignoring case" search, four copies, one with diacritic folding.** `examples/azul-mail/src/
listing.rs:204-209 matches_search` (from/to/subject), `examples/azul-notes/src/model.rs:516-528` (+ `#tag` prefix
match, cached lower-cased haystack :161-172), `examples/azul-tasks/src/views.rs:232-247 search_matches` (strips `#`,
empty query matches NOTHING - opposite of Mail/Notes where empty matches everything), `examples/azul-contacts/src/
book.rs:46-80 fold` + `:137 matches` (diacritics folded: "Kruger" finds "Krüger"; none of the other three does).
Proposal: `SearchQuery::parse(text)` -> words + `#tags`, `matches(haystack_folded)`, `fold()` in the shared crate;
consistent empty-query semantics. S. Benefits: Mail, Notes, Tasks, Contacts, Drive, Calendar (has none - its List
view cannot be searched).

B17. **Multi-selection model re-implemented.** `layout/src/widgets/message_list.rs:418-495 MessageListSelection`
(exported: `create/apply(index, shift, ctrl)/contains/len/is_empty`, fields `rows`, `anchor`; used by AzMail
lib.rs:119) vs `examples/azul-tasks/src/state.rs:367-398 select(id, shift, ctrl)` (same plain/Ctrl/Shift-range-from-
anchor logic, keyed by ID over the visible order - survives re-sorting, which the index model does not) vs AzContacts
single selection (`ui.rs:262-280`). `cell_grid.rs:1557-1600` has the 2-D variant. Proposal: generalize to an exported
`ListSelection` (widgets, `list_selection.rs`): ids as `StringVec` + anchor, `apply(id, order: &StringVec, shift,
ctrl)`, `select_all`, `retain(existing)`; `MessageListSelection` becomes a thin alias; ListView/TreeView/task rows use
it. S-M. Benefits: Mail, Tasks, Contacts (multi-delete/export), Notes (bulk move), Drive.

B18. **Shortcut tables: written twice per app (display text vs key handler), four spellings.** Notes
`examples/azul-notes/src/ui.rs:57-75 SHORTCUTS` ("Ctrl/Cmd+N") is separate from the handlers (editor.rs:882-1030,
ui.rs key handler); AzMail About text `ui_main.rs:294-297` ("Ctrl+N") separate from its key match; AzTasks
`chrome.rs:5-190` has ONE `Command` table driving ribbon, palette, shortcuts page and keys ("Cmd+N"); appkit
`shortcuts.rs:10-80` (`Shortcut`, "Mod+N", `display_keys(mac)`, `matches`). api.json already has `GlobalHotkey::parse`
/ `to_display_string`, `HotkeyModifiers::primary`, `VirtualKeyCodeCombo` and `ShellSettingShortcut` - but no way to
test a key event against a parsed combo. Proposal (API): `GlobalHotkey::matches(KeyboardState) -> bool` (or a
window-local `KeyBinding` type sharing its parser) so every app keeps AzTasks' one-table design; display via
`to_display_string` (Cmd on macOS, Ctrl elsewhere). S (API) + S per app.

B19. **WAV writer x3.** `examples/azul-review/src/session.rs:100-125 wav(samples, rate)`, `examples/azul-drive/src/
args.rs:~165-190` (sample tone), `examples/azul-drive/src/preview.rs:288-306` (test helper; the same file also parses
WAV for previews). Same 44-byte header code. Proposal (API): `AudioSink`/audio module `encode_wav(samples, rate,
channels) -> U8Vec` + `decode_wav(bytes)`. S. (AzReview is in this area; Drive is not.)

B20. **AzMail's HTML sanitizer carries its own lenient HTML tokenizer, now duplicated by core (LENIENT).**
`examples/azul-mail/src/html.rs` (1428 l): its own tokenizer / attribute parser (`run` :259-326, `parse_attributes`
:515-566), implied-end-tag repair (`implied_ends` / `close_item` / `pop_through` :442-486), character references with a
~120-name partial table + Windows-1252 repair (`decode_references` / `reference` / `numeric_char` / `named_char`
:923-1080), then it re-serializes XHTML and parses it with the STRICT `Xml::from_str` (`ui_main.rs:1183`). Since
LENIENT, `core/src/xml_html.rs` has `HtmlTokenizer`, `TreeBuilder` (implied ends, scopes, adoption), the full 2231-name
entity table (`core/src/xml_entities.rs`) and the same Windows-1252 rule, exported as `Xml::create_from_html(String)`
(api.json module xml). ~600 of AzMail's lines are now a weaker twin (fewer entities, simpler repair). Keep the POLICY
(drop list, style scoping, dark-rule detection, image blocking, tracking pixels) and run it as a walk over the
`XmlNode` tree `Xml::create_from_html` returns. M. Risk: the sanitizer's 40+ tests pin current output - rerun them.
Second sanitizer with an overlapping policy: `layout/src/paste_html.rs` (`DROPPED` :49, `safe_href` :330) for paste.
Proposal: one exported `HtmlSanitizePolicy` + `Xml::sanitize(policy)` (core/src/xml_html.rs) used by AzMail's
reading pane, the engine's paste and the shared editor's `from_html` (A2).

B21. **"Safe link" rule x3**: `examples/azul-mail/src/compose.rs:685-690 safe_link`, `examples/azul-mail/src/html.rs:583`
(inline), `layout/src/paste_html.rs:330-335 safe_href` - identical scheme lists (http, https, mailto). Folds into B20.

B22. **MIME type by extension x3 (+ core's, private to the XML loader).** `examples/azul-mail/src/compose.rs:546-584
mime_type_for` (documents + media, 35 types), `examples/azul-storage/src/s3.rs:239-255 content_type_for` (14, with
charset), `core/src/xml.rs:233-265 MimeTypeHint::from_extension` (web assets only, no pdf/docx/txt; `MimeTypeHint` is
in api.json with NO functions). Proposal (API): extend `MimeTypeHint::from_extension` with the document types and
export `MimeTypeHint.from_extension(ext)` / `from_file_name(name)` (module xml, or move the type to `file`); Mail and
azul-storage call it. S.

B23. **Byte-size formatting x4.** `examples/azul-mail/src/ui_main.rs:1142-1148 human_size` (KB rounded UP, MB one
decimal), `examples/azul-mail/src/ui_compose.rs:581` (inline `(size + 1023) / 1024` KB), `examples/azul-drive/src/
browse.rs:263 format_size` (one decimal always), `layout/src/widgets/tile.rs:145-162 format_bytes` (pub, used by
wizard_pages; one decimal below ten). Three different outputs for the same size (1536 B: "2 KB" / "1.5 KB" / "1.5
KB"). Proposal (API): export `format_bytes` (e.g. as `FileMetadata::format_size(u64) -> String`, module file); Mail
and Drive call it. S. (DRIVE2 report also lists the Drive/tile pair.)

B24. **Avatar initials x3.** `examples/azul-contacts/src/book.rs:108-127 initials` (given/family aware, CJK, "?"),
`examples/azul-meet/src/ui.rs:440-452 initials`, `examples/azul-mail/src/ui_main.rs:1106-1111` (inline, same as Meet's).
`Avatar::create(initials)` (layout/src/widgets/avatar.rs:224) takes ready-made initials and `ReadingPane::with_people`
takes initials strings. Proposal (API): `Avatar::create_from_name(name: String)` computing initials (Contacts' rule,
first+last word) in avatar.rs; `ReadingPane::with_people` takes names. S.

B25. **DatePicker's date grid is hard-wired Sunday-first** (`layout/src/widgets/date_picker.rs:1342-1352`, private
`enum WeekStart` :1473) while AzCalendar's weeks are Monday-first (`examples/azul-calendar/src/week.rs:42`) and AzTasks
has a week-start setting Mon/Sun/Sat (`model.rs:491`, `backstage.rs:373`) it cannot pass on. **Visible bug**: AzCalendar's
date navigator (`chrome.rs:223-243`, `DatePicker::create(..).with_range(first, last)` in Date mode) lights a Mon-Sun week
that the Sunday-first grid splits over two rows. API item: `DatePicker::with_week_start(Weekday-like enum)` (export
`DatePickerWeekStart { Sunday, Monday, Saturday }`), and `ToDoBar::with_range` / `with_week_start` forwarding it
(CAL3 report: the To-Do bar's calendar cannot light the range). S.

B26. **Thread/job glue x3 + `file_url` x2.** The same "take-once init, run on a Drive, write back an outcome"
skeleton: `examples/azul-notes/src/jobs.rs:28-75` (`JobInit {drive, job}`, `job_thread`, `Done {outcome}`, `spawn`),
`examples/azul-tasks/src/jobs.rs:59-160` (same names, same shape, + a write queue), `examples/azul-appkit/src/ui.rs:
276-342` (`FileThreadInit`, `file_thread`, `spawn_file_jobs`, `FileReply`). `file_url(path)`:
`examples/azul-tasks/src/jobs.rs:101-118` vs `examples/azul-drive/src/browse.rs:477-500` (diverged: Drive swaps `\` only
on Windows, Tasks always - Drive's is right). Proposal: appkit gets a generic `spawn_job::<J, O>(info, drive, job,
run, on_done)`; `file_url` moves to `azul_storage` next to `sigv4::uri_encode` (TASKS report says the same). S each.

B27. **Tag normalisation + tag editor UI twice (three with Contacts' groups).** `examples/azul-notes/src/markdown.rs:189
clean_tag` == `examples/azul-tasks/src/model.rs:306 normalize_tag` (identical body); `add_tag`/`remove_tag`/`has_tag`
twins (notes model.rs:208-230, tasks model.rs:250-270). UI: chips + "Add tag" TextInput with Enter/comma handling in
`examples/azul-notes/src/ui.rs:1031-1060` and `examples/azul-tasks/src/detail.rs:520-541` (near line-for-line), groups in
`examples/azul-contacts/src/ui.rs:880-905`. -> the `TokenInput` widget (C3). S+M.

B28. **base64 x2 in this area (x5 in the tree).** `examples/azul-contacts/src/ui.rs:1967-1990 base64` (photo `data:`
URIs) vs `layout/src/callbacks.rs:7842 base64_encode` (pub, not exported), `layout/src/telemetry/crash_mail.rs:306`,
two decoders in e2e (project.rs:972, full.rs:21722). Contacts also cannot SHOW a photo (SMALLAPPS: "decode the data:
URI into an ImageRef ... initials today"). API item: `ImageRef::from_data_url(String) -> OptionImageRef` (decode +
`decode_image_bytes`) and `U8Vec::to_base64` / `from_base64`. S.

B29. **Copied "close guard" behaviour, three different answers.** AzMail holds a compose window's close and shows its own
"Save changes?" bar (`ui_compose.rs:746-770` veto via `flags.close_requested = false`, bar at :323); AzNotes saves then
closes (`jobs.rs:156-174`, same veto mechanism); AzCalendar's editor window DISCARDS an edited appointment on close
without asking (`editor_ui.rs:993-1002`); AzWriter has no close handler at all (unsaved document lost). The
`MessageBox` widget with exactly the "Save / Don't Save / Cancel" preset exists (`layout/src/widgets/standard_dialogs.rs:
451-480`) and no app in this area uses it. Proposal: AzCalendar + AzWriter adopt AzMail's veto with `MessageBox`; a small
appkit helper `hold_close_and_ask(info, title)` (or a `WindowCloseGuard` in widgets) removes the three copies. S-M.

B30. **Widget test helpers copied per module**: 67 small finders (`node_labelled`, `row_labelled`, `texts`, ...) across
widget test modules (address_bar.rs:823, todo_bar.rs:880, wizard_layout.rs:1016, tree_view.rs:2782, + message_list,
reading_pane, info_bar, module_switcher, ...). `roving::test_support` (roving.rs:262) is already the shared test module.
Move the finders there. S. (MAILWIDGETS report noted it.)

## C. Reuse candidates (app code that should be shared)

C1. **`examples/azul-pim` - a plain-Rust shared crate for the PIM apps** (the way azul-storage / azul-appkit are shared;
domain logic does not belong in api.json). Modules, each from the copy named: `dates` (B5: days_in_month, ymd_clamped,
add_months, names, ordinal; B7: `RelativeDay` + labels), `recurrence` (B6: AzCalendar's rrule.rs + AzTasks' completion
mode), `content_line` (B15), `mail_address` (B14), `search` (B16: words, `#tags`, diacritic `fold` from Contacts'
book.rs), `task` (B12: AzTasks' model.rs Task + file format + keys, so Calendar's and Mail's To-Do bars share it),
`tags` (B27), `ids` (B1, until `Uuid::random` lands). Adopters: Mail, Notes, Tasks, Calendar, Contacts (+ Meet,
Drive). Effort M (mostly moves with their tests). Risk low (pure code, tests move along).

C2. **`RichTextEditor` widget** - see A2 (layout/src/widgets/rich_text_editor.rs). The single largest item. L.

C3. **`TokenInput` widget** (layout/src/widgets/token_input.rs): chips + a text field, Enter / comma / blur commits, Backspace
in an empty field removes the last chip, optional suggestions popover. API: `TokenInput::create(tokens: StringVec)`,
`with_placeholder`, `with_separators(String)`, `with_validator(cb(String)->bool)` (red chip on failure),
`with_suggestions(StringVec)`, `with_on_change(RefAny, cb(StringVec))`. Adopters: AzMail To/Cc/Bcc (today TextInputs +
`split_addresses`), AzCalendar attendees (fixes B14's comma bug), AzNotes tags, AzTasks tags (TASKS report: "a TokenInput
for tags (chips + a line now)"), AzContacts groups. M.

C4. **`Toolbar` widget with toggle buttons** (layout/src/widgets/toolbar.rs): `ToolbarItem::{Button{icon, label, toggled,
on_click}, Separator, Group}`; `aria-pressed`, roving focus (the `roving` module). Notes builds one from `Button`s with
`ButtonType::Primary` as "pressed" (`examples/azul-notes/src/ui.rs:849-893`, and it never shows B/I/U pressed);
`RibbonButton.toggled` (ribbon.rs:2702) already does this inside the Ribbon; `Button` has no pressed state (DIALOGS also
asked for `Button::disabled`). The shared editor's toolbar (A2) is the first user. S-M.

C5. **`RecurrenceEditor` widget** - AzCalendar's repeat segments (`editor_ui.rs` + `editor.rs:338-540` rule_of / choices / segments / variants /
labels) and AzTasks' preset + "Custom..." editor (`detail.rs:72-120` presets, `:392-470` the editor, `:860-900` its callbacks; TASKS' planned `RecurrenceEditor`) over C1's `Recurrence`. M.

C6. **AzMail's child-window pattern for AzCalendar**: CAL3 says "one editor at a time (the layout callback cannot tell two
editor windows apart)", but AzMail already solves exactly that with `window_state.layout_callback.ctx =
RefAny(ComposeKey(id))` + `LayoutCallbackInfo::get_ctx()` (`examples/azul-mail/src/ui_compose.rs:246-254, 299-305`).
AzCalendar's `open_form` (`editor_ui.rs:117-144`) can lift its one-editor limit the same way. S. (Report claim corrected.)

C7. **`TaskRow`** stays app-local (TASKS D4) but `ToDoBar` (widget) should take a richer task (due, flag, priority) once C1's
task model is shared, so AzMail's and AzCalendar's To-Do bars show AzTasks' tasks.

## D. api.json exposure (things the apps re-implement or work around; name only, autofix adds them)

| # | Item (Type.method, module) | Why / who works around it | Evidence |
|---|---|---|---|
| D1 | `DocumentTextEdit.formats: TextFormatSpanVec` (or a `FocusEventFilter::TypingStyleChanged`) - module css/selection | the typing style (Ctrl+B at a caret) AND an inline formatted paste reach the app as plain text: `paste_inline_formatted` writes styled runs to the overlay only (`layout/src/window.rs:4569-4620`), `unsynced_text_edits` flattens them (:3061-3075). Notes mirrors the typing style by hand (editor.rs:505-536) but loses pasted bold too; Mail and Writer lose both. Engine change, M. | A3.2 |
| D2 | `TextAreaState.get_text() -> String` (widgets) | exists in Rust (text_area.rs:367), 5 apps copy its body | B8 |
| D3 | `BoxOrStaticString.as_str()` (css) or `Dom.get_text_content() -> String` / `NodeType.get_text() -> OptionString` (dom) | `box_str` unsafe copies x3 | B9 |
| D4 | `NodeData.get_attribute(name: String) -> OptionString` and `NodeData.get_attributes() -> AttributeTypeVec` (dom) | AzMail stores a link's href in a CLASS `azmail-href:` (editor.rs:35,153,324) and loses pasted links' hrefs; AzNotes' paste reader ignores `<a>` (editor.rs:452); `Dom.set_attributes` exists, no getter | A1 |
| D5 | `Pdf.from_dom_in_callback(self: ref, callback_info: ref CallbackInfo, ..)` - today `callback_info: CallbackInfo` by value | three apps wrote `reborrow_info`; `*info` (Copy) suffices | B9 |
| D6 | `Uuid.random() -> String`, `Uuid.is_valid(text) -> bool` (uuid) | three random-id copies; AzNotes uses the deterministic `v4` for file ids (data loss) | B1 |
| D7 | `MimeTypeHint.from_extension(ext)` / `from_file_name(name)` (xml or file), table extended with documents | 3 tables | B22 |
| D8 | `format_bytes(u64) -> String` as e.g. `FileMetadata.format_size` (file) | 4 formatters, 3 outputs | B23 |
| D9 | `Avatar.create_from_name(name: String)` (widgets); `ReadingPane.with_people` taking names | 3 initials helpers | B24 |
| D10 | `DatePicker.with_week_start(DatePickerWeekStart)` + exported enum; `ToDoBar.with_range` / `with_week_start` (widgets) | Calendar's navigator splits a Mon-Sun week over two rows; Tasks' week-start setting cannot reach the picker | B25 |
| D11 | `GlobalHotkey.matches(KeyboardState) -> bool` (app) or a window-local `KeyBinding` sharing `GlobalHotkey::parse` | shortcut tables written twice per app | B18 |
| D12 | `ImageRef.from_data_url(String) -> OptionImageRef`; `U8Vec.to_base64()` / `U8Vec.from_base64(String)` (image / vec) | Contacts' own base64, no photo preview | B28 |
| D13 | `Xml.sanitize(policy: HtmlSanitizePolicy) -> Xml` next to `Xml.create_from_html` (xml) | AzMail's 1428-line sanitizer duplicates the lenient parser; the engine paste has a second policy | B20 |
| D14 | audio: `encode_wav(samples: F32Vec, rate: u32, channels: u16) -> U8Vec`, `decode_wav(U8Vec)` | 3 WAV writers | B19 |
| D15 | `ListSelection` (ids + anchor, `apply(id, order, shift, ctrl)`), `MessageListSelection` kept as alias (widgets) | AzTasks re-implements the model because the exported one is index-based | B17 |
| D16 | `ShellThemeAccent.colors(mode) -> ...` (shells) - known SHOW item; in this area AzNotes draws its check boxes and the history "+" lines in a hard-coded BLUE `look.accent` (`look.rs:38` `#2f6fde`, used editor.rs:235, ui.rs:2230) inside a LEAF (green) theme scope (ui.rs:115). Use `var(--az-accent)` (theme_scope.rs:150) as AzTasks does (list.rs:64) - no API needed for that fix. | B-level, S |
| D17 | `system:error` / `system:danger` colour token (css) | the same error red `#b3261e` / `#ff8a80` is hand-copied into 5 files: azul-calendar lib.rs, azul-mail ui_main.rs + ui_account.rs, azul-notes look.rs:42, azul-tasks list.rs:57 (`system:text`, `system:secondary-text`, `system:separator`, `system:link` exist; no error colour) | S |

## E. Semantics / placement

E1. **`MessageList` is now the generic grouped item list** (AzNotes' note list with `MessageListMark::Pin`; AzMail; next
AzTasks/AzDrive could use it). Name, `MessageRow`, `MessageListMark::Flag` default and `with_on_flag` keep the mail
vocabulary; consider `ItemList` + type aliases (layout/src/widgets/message_list.rs). S, low priority.

E2. **AzMail's editor model lives in `compose.rs`** (`Run`, `BlockKind`, `Block`, `MailDoc`, `to_plain`, `to_html`,
compose.rs:245-460) next to MIME, drafts and address parsing; the Dom<->model code is in `editor.rs`. With A2 the model
goes to the widget; until then `MailDoc` belongs in `editor.rs` (or `mail_doc.rs`). S.

E3. **AzMail `send.rs` vs `sending.rs`**: `sending.rs` (205 l) is only the settings-form model over `send::SendSettings`;
the near-identical names invite edits in the wrong file. Rename `send_settings_form.rs` or fold into `ui_account.rs`. S.

E4. **AzCalendar mints TASK ids with `event::new_event_id`** (`tasks.rs:43`) and validates them with `event::is_event_id`
(:75, :113), and keeps `is_email` in `event.rs:241` - helpers in the wrong module (C1 `ids` / `mail_address`). S.

E5. **AzNotes keeps tag cleaning in `markdown.rs`** (`clean_tag`, `clean_tags`, :189-205) while the model (`model.rs`)
calls it; tags are a model concern (C1 `tags`). S.

E6. **AzWriter `document::dom_to_markdown` (document.rs:540-620) is test-only**: production saves through
`ir::to_markdown` (`snapshot_for_save` lib.rs:333-348). A second, unused Markdown writer - delete with its tests or make it
the round-trip oracle explicitly. S.

E7. **Data root names** (`AzMail`, `AzNotes`, `AzCalendar` vs `Azlin` for Tasks/Contacts) contradict the Azlin storage
design (one per-user root, a folder per app) - see B11. M.

E8. **CAL3 report claim to correct**: "one editor window at a time - the layout callback cannot tell two editor windows
apart" - false since AzMail's `LayoutCallback.ctx` / `LayoutCallbackInfo::get_ctx` pattern (C6).

## F. Other next-wave items found on the way

F1. **AzNotes drops structural edits it cannot mirror and reverts them** (`editor.rs:676-686`: `eprintln!("a structural
edit the model cannot mirror was dropped")` then `RefreshDom`): a `ReplaceChildren` below the host (paste inside a
quote/list item), `WrapRange`, `InsertChildren`/`RemoveChildren` are undone visually without telling the user. With A2
the widget either mirrors every op or applies the engine's `apply_to_dom` to a scratch Dom and re-reads the blocks
(AzMail's approach). M.

F2. **Structural undo**: AzNotes acks splits/merges without an inverse (no undo of Enter/Backspace-merge), AzWriter keeps
its own second stack (A3.6), AzMail hands the inverse to the engine. One rule for the shared editor: inverse always
handed to the engine (`mark_document_edit_applied_with_inverse`), no app stacks. M.

F3. **AzMail reopened drafts lose formatting** (MAIL2 sec. 7: the body comes back from text/plain). Fixed by A2's
`from_html` (or now: parse the draft's HTML part with `Xml::create_from_html` + `host_to_doc`). S-M.

F4. **AzWriter has no close guard** (unsaved document lost on window close) and **AzCalendar's editor discards an edited
appointment on close** (B29). S each.

F5. **AzTasks search runs on Enter only and an empty query matches nothing** (views.rs:232-247) - inconsistent with the
other apps (B16). S.

F6. **AzMail Sent duplicates (unverified)**: a relay that files sent mail server-side (Gmail) gives the local copy
(LOCAL_UID_FLOOR uid) AND the server's after the next sync (A3.9). Dedupe by Message-ID when adopting. S.

F7. **AzNotes `toggle_format` engine mapping maps `Format::Code` to `None`** (editor.rs:733-741, `Format::Code => None` :738): Ctrl+E at a caret sets
the app's typing style but the engine does not paint it, and `sync_text` asks for a rebuild only for `code`
(editor.rs:573) - so inline code typed at a caret appears unstyled until the rebuild. Works, but it is the D1 gap
again (the engine's `TextFormat` has no Code variant). Add `TextFormat::Code` with D1. S.

## B (continued)

B31. **Test `TempDir` x7.** `examples/azul-calendar/src/test_dir.rs` (whole file), `examples/azul-mail/src/testutil.rs:9`,
`examples/azul-tasks/src/store.rs:314`, `examples/azul-drive/src/fileops.rs:825`, `examples/azul-storage/src/tests/mod.rs:21`,
`examples/azul-appkit/src/files.rs:140 TestDir`, plus inline temp folders in azul-notes store.rs:459 and azul-writer
document.rs:862 (fixed name `azwriter_round_trip` - two parallel test runs collide). Proposal: azul-storage exports a
`testing::TempDir` behind a `test-util` feature; apps use it as a dev-dependency. S.

B32. **TLS / MIME twins around AzMail (SEND report, verified).** `examples/azul-mail/src/imap_client.rs:161-188 tls_stream`
builds the same rustls config (RustCrypto provider + webpki roots + an extra CA) as `micromail::tls::client_config_with_roots`
(micromail-0.2.0 tls.rs:37, now from crates.io) - diverged: IMAP reads only the FIRST certificate of the extra-CA PEM,
micromail all of them. `layout/src/telemetry/crash_mail.rs:279 build_mime_body` + `:306 base64_encode` are twins of
micromail's `MessageBuilder`; layout still pins `micromail = "0.1.0"` (layout/Cargo.toml:143), so the crash-mail
migration SEND described (and its four RED sink tests) is still open. S each.

B33. **Theme / mode switch UI rebuilt in every app, inconsistently, mostly not persisted.** AzMail ribbon View
(`ui_main.rs:614-617`, Flat/Flora/Light/Dark, no System, `set_theme`/`set_mode` at :498-501, not saved); AzCalendar ribbon
VIEW (`chrome.rs:201-204`) AND its Options page (`chrome.rs:700-703`) - four buttons each, no System, not saved
(`settings.rs` has no theme key); AzTasks backstage Segmented (`backstage.rs:206-214`, with System) AND ribbon
(`chrome.rs:372-378`, with System), not saved (TASKS report); AzNotes settings Segmented (`ui.rs:1857-1867`, "Follow the
system"), saved in `aznotes-settings.txt`; AzContacts via appkit's Appearance section (`appkit ui.rs:592 settings_page`),
saved. Proposal: `RibbonGroup::create_look(theme, mode, on_change)` (widgets/ribbon.rs) and a ready Appearance
`ShellSettingsSection` (shells/settings_layout.rs) reporting a `(theme, mode)` change; persistence through
`AppSettings` (B11). S-M. Benefits: all PIM apps + Writer, Sheets, Show.

## Status of the items the brief named

| Item | Status | Where |
|---|---|---|
| NOTES / AzWriter "editor twins" (+ AzMail) | confirmed, compared line by line; AzNotes is the base | A1, A2 |
| AzMail Ctrl+B with no selection not reported | confirmed; same root cause also loses inline formatted PASTE in all three editors; plus Ctrl+B over a selection does nothing in Mail/Writer | A3.2, A3.3, D1 |
| AzMail unfetched `<img>` without size = 300x150 hole | confirmed; root in sizing.rs:609-624 for a sourced NullImage | A3.8 |
| Locally sent mails moved to stale/ on first Sent sync | ALREADY FIXED in code (a9e67753b + 6420d63ab, two tests); close after the suite runs | A3.9 |
| TASKS twins: random_seed, file_url, TempDir, tree-row test helper | all confirmed | B1, B26, B31, B30 |
| NOTES twins: E2E clients, box_str, reborrow_info, rfc3339_utc vs iso8601, ir.rs vs doc.rs | all confirmed (reborrow_info is not even needed: CallbackInfo is Copy) | B13, B9, B4, A1 |
| MAIL2 twins: box_str, mime_type_for vs guess_mime_from_url, LocalFolder vs Drive, no NodeData attribute getter | confirmed | B9, B22, B3, D4 |
| SEND twins: crash_mail MIME/base64, IMAP TLS config | confirmed, still open | B32 |
| CAL3: text_field/drop_down (merged in-app), "one editor window" limit, DatePicker range, To-Do bar no range | the limit is solvable today (C6); DatePicker week start bug found | C6, B25 |
| EDITOR twins (element_shell, is_bold, seed_empty_run, insert_text_with_line_breaks) | merged in the engine by EDITOR; E-XML-1 (lenient paste HTML) is now done by LENIENT (`paste_html.rs:111` uses `parse_html_nodes`) | - |

## PRIORITIZED next-wave task list (most value per effort first)

1. **[S, urgent, data loss] AzNotes note ids from `Uuid::v4`** (`azul-notes/src/jobs.rs:296`) -> a random id (copy AzTasks'
   `new_id` for now); then export `Uuid.random()` / `Uuid.is_valid()` (D6) and delete the three app copies (B1). Same fix
   in azul-photo lib.rs:167, azul-show commands.rs:50, azul-videocut lib.rs:2839/2858/2874. RED: AzCalendar's guard test
   shape (`event.rs:1000-1012`): 256 new note ids are valid v4 UUIDs and none starts with `00000000-0000` (the marker
   sequence every process starts with).
2. **[S] AzMail typing flattens formatting** (A3.1): RED `typing_into_a_formatted_paragraph_keeps_its_bold_and_link`; fix
   by diffing the block text into the block's inline children (AzNotes' `text_diff` + run splice) instead of replacing them.
3. **[S] Ctrl/Cmd+B/I/U over a selection in AzMail and AzWriter** (A3.3): a VirtualKeyDown arm calling the existing ribbon
   action + `prevent_default` (AzNotes' editor.rs:905-930 pattern). Also AzMail unwrap-on-second-press (A3.4).
4. **[M, engine] `DocumentTextEdit` carries the inserted runs' formats** (D1, with `TextFormat::Code`): fixes the typing
   style (ledger item "Ctrl+B with no selection not reported") AND inline formatted paste for all three editors; then delete
   AzNotes' `Typing` mirror. RED in layout/tests: type after Ctrl+B and paste `<b>x</b>`, assert the edit's spans.
5. **[S-M, engine] Unresolved markup `<img>` takes no space** (A3.8; sizing.rs:609-624), RED
   `an_unresolved_img_from_markup_takes_no_space`.
6. **[S] DatePicker week start** (D10/B25) + AzCalendar navigator Monday-first + `ToDoBar.with_range`.
7. **[S] AzCalendar attendees split inside quotes** (B14) - move AzMail's `split_addresses`/`bare_address` into the shared
   module first (C1 `mail_address`), one `is_email` rule (keep Mail's dot-free acceptance behind `strict: false`).
8. **[S] api.json small exports** (one autofix batch): `TextAreaState.get_text` (D2), `BoxOrStaticString.as_str` or
   `Dom.get_text_content` (D3), `NodeData.get_attribute` (D4), `Pdf.from_dom_in_callback` by ref (D5) + delete the three
   `reborrow_info`, `MimeTypeHint.from_extension/from_file_name` (D7), `format_bytes` (D8), `Avatar.create_from_name` (D9),
   `GlobalHotkey.matches` (D11), `ImageRef.from_data_url` (D12).
9. **[L] `RichTextEditor` widget** (A2), in commits: (a) AzNotes' `doc.rs` + tests -> `layout/src/widgets/rich_text/doc.rs`
   with AzWriter's inverse-returning ops; (b) Notes' Markdown writer + AzMail's HTML/plain writers + a `from_html` over
   `Xml::create_from_html`; (c) the widget (host render, sync, structural edits, keys, toolbar C4); (d) AzNotes adopts;
   (e) AzMail compose adopts (quote_depth; reopened drafts keep formatting, F3); (f) AzWriter adopts (keeps pagination,
   one undo stack, A3.6); (g) Tasks / Calendar / Contacts notes fields.
10. **[M] `examples/azul-pim` shared crate** (C1): dates, recurrence, content_line, mail_address, search+fold, tags, task
    model; then **[M] one task store** for AzTasks / AzCalendar / AzMail To-Do bars (B12).
11. **[M] One data root + settings + args via azul-appkit** (B10, B11, B33): Mail, Notes, Tasks, Calendar onto
    `data_root`/`app_key`/`AppSettings`/`AppArgs`; persist theme/mode everywhere.
12. **[M] AzMail sanitizer on `Xml::create_from_html`** (B20) + shared sanitize policy with `paste_html.rs` (D13, B21).
13. **[M] AzMail `LocalFolder` -> `azul_storage::LocalDrive`** (B3) + drop `rfc3339_utc` (B4); **[M] AzCalendar onto Drive
    + jobs** (B2); generic appkit `spawn_job` + `file_url` into azul-storage (B26).
14. **[S] AzCalendar: several editor windows via `LayoutCallback.ctx`** (C6) and a close guard; AzWriter close guard (B29,
    F4) - both with the existing `MessageBox` widget.
15. **[M] Widgets**: `TokenInput` (C3: Mail To/Cc/Bcc, Calendar attendees, Notes/Tasks tags, Contacts groups),
    `Toolbar` with toggles (C4), `ListSelection` by id (B17/D15), `RecurrenceEditor` (C5), theme/mode ribbon group (B33).
16. **[S] Cleanups**: AzWriter `if !applied {}` + unreachable `set_run_text` (A3.7) and test-only `dom_to_markdown` (E6);
    AzWriter style gallery index 4 + H4-6 as h3 (A3.5); AzNotes accent via `var(--az-accent)` (D16); `system:error`
    token (D17); `sending.rs` rename (E2/E3); `TempDir` test util (B31); widget test finders into `roving::test_support`
    (B30); WAV codec (B19); base64 (B28); IMAP TLS via micromail + crash-mail on micromail 0.2 (B32); AzTasks search
    semantics (F5).
17. **Ledger**: close "locally sent mails moved to stale/" once `cargo test -p AzMail --lib sync::` is green (A3.9).

## Seen but NOT verified

- Whether a list item's `DocumentTextEdit.text` includes its `::marker` text (`flatten_inline_content` of the overlay entry;
  EDITOR says the overlay holds the text without generated items, `window.rs:20040-20045`) - matters for AzMail `li` sync.
- The knock-on effect of A3.1 on the NEXT structural edit in AzMail (model and laid-out DOM disagree on child indices, so
  `apply_to_dom` may fail and the edit is never acked) - derived from reading, not run.
- AzWriter: whether the engine's Ctrl+Z of a structural edit arrives as a new `DocumentEdit` the app then also pushes on
  its own stack (A3.6 interplay) - not traced.
- AzMail Sent duplicates for relays that file sent mail server-side (F6).
- That `Xml::create_from_html` + a tree walk reproduces AzMail's sanitizer test expectations (B20) - the 40+ tests not run.
- Two tilt-aware pen-ink rasterizers: `examples/azul-review/src/ink.rs:7-150` vs `examples/azul-paint/src/lib.rs:215-420`
  (dab/ellipse splats with tilt elongation) - a possible shared `InkCanvas`/brush module; not diffed.
- Whether `MessageBox` (standard_dialogs.rs) can be shown inside a child window (AzMail compose, AzCalendar editor) - not
  checked.
- Mail's `To-Do bar` tasks and AzCalendar's `tasks.rs` vs AzTasks file format: compatibility of a merged reader not
  designed in detail (B12).
- Nothing was compiled or run (house rule); every finding is from reading code at 53c978b33.
