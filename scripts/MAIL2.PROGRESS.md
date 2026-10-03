# MAIL2 progress (AzMail: Outlook-2010 window, compose window, send, E2E)

Branch `wt/mail2` from `39092feee`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a9d812f830332c658`. Nothing is compiled here
(house rule); the parent compiles. Brief: scratchpad `MAIL2_go.md`, rules `wave4_common.md`.

## DONE (commit hashes)

- `552fb1ee9` plan + progress file
- `8ed90db3a` / `103ea3069` RED / GREEN: `TreeViewNode::badge` (the folder tree's unread count)
- `44a941729` / `f80b56184` RED / GREEN: listing.rs, compose.rs, sending.rs, the message view's
  thread headers, sync adopts a local-only folder (UIDVALIDITY 0), `LocalFolder::delete`

- `1216806c2` / `63da1f890` RED / refactor: drafts and the Sending section go through SEND's
  `send::build_message`, `send::file_message`, `SendSettings::load/save` (twins removed; one
  `sending.json`). SEND API used beyond the brief's interface: `build_message`, `file_message`,
  `LOCAL_UID_FLOOR`, `SendSettings::{load, save}`, `SendRoute`, `TlsPolicy`.

## IN PROGRESS

- App UI (NEXT step 3), in pieces, each committed. Done: a (`a7add8cae`/`94d8ea0ee`), b
  (`968973d2a`), c (`ffff895d3`, `f3d582151`, `d1c39f4bd`), d part 1 (`ae989a728`: state +
  start + skeleton files), d part 2 (`f0a2f7160`, `0573fe057`: keyring, Send/Receive thread with
  outbox retry, IO thread). Extra: account name RED/GREEN (`87b20c2a0`/`50d2bc6e8`); local Sent
  mail across the first sync and a renumbering RED/GREEN (`c406c9905`/`b8d087b3d`, the gap SEND's
  report names). f ui_account.rs (`b8faed876`). E-IMG engine RED/GREEN (`052d8dc9c`/`8f5bbf164`:
  an `<img src>` from markup shows the image cached under its src). html `sanitize_with` pictures
  RED/GREEN (`a55f964ad`/`83b0fab68`). e ui_main.rs parts 1-3 (`8af876dd7`, `6d9953a98`,
  `f87188002`), open sanitizes (`30b1a144f`). g ui_compose.rs parts 1-3 (`ddfebfbab`,
  `28b190ea2`, `0d1811053`). h sample.rs (`6a5a034c0`).
- Step 4 engine, done so far: headless child windows laid out + pumped RED/GREEN
  (`1d0bed92d`/`6f5e2cf8f`: `HeadlessWindow::pump_once(is_root)`, `pump_children`,
  `children` field); RefreshDomAllWindows from timers/threads fans out RED/GREEN
  (`ad1e4078b`/`86d6846e3`).
- Debug server routing RED/GREEN (`7c027543e`/`6720b7c24`), runtime windows get the debug
  timer (`aa05c82dd`: headless, macOS x2, Windows, X11, Wayland).
- Step 5: `scripts/azmail_e2e.py` (`750c6a05b`); MAIL1's sync E2E on the wizard (`74aad2950`).
- After the E2E: a reopened draft keeps Bcc + In-Reply-To RED/GREEN (`a4478ac0b`/`107a0daf3`);
  Forward and a reopened draft carry the attachments RED/GREEN (`7c6cf859a`/`78bdd1f22`);
  "Save changes?" on closing an edited compose window RED/GREEN (`994b2d355`/`e59adddd2`).
- Step 6: report `scripts/MAIL2_2026_10_01.md` (`cddea66c8`, `c5810654d`, `82f107382`,
  `a1f145739`).

## STATE: DONE. Nothing in progress. If resumed: only keep the report's commit list current.

## NEXT (in order)

1. Widget: `TreeViewNode::badge` (the folder tree's unread count), RED test then GREEN; flat + flora
   looks appended at the end of the theme files.
2. App model, RED (`todo!()` bodies + tests) then GREEN: `listing.rs` (date groups, message rows,
   unread counts, folder tree order), `compose.rs` (reply / reply all / forward, address parsing,
   quote blocks, OutgoingMail, text + HTML serializers of the editor model, the draft `.eml` and its
   index line), `sending.rs` (the "Sending" settings file next to account.json, no secret).
3. App UI: PimShell window (ribbon File/Home/Send-Receive/Folder/View, navigation pane with
   Favorites + account trees, message list grouped by date, reading pane on paper with the
   "download pictures" info bar, To-Do bar, status bar with sync), backstage (Info / Add account
   wizard / Account settings with "Sending" / About), compose window (second window: From,
   To / Cc / Bcc, Subject, toolbar, contenteditable body on the Path-2 Dom model, attachments, Save
   draft, Send on an azul Thread through `send::send_mail`).
4. Engine: headless child windows are laid out and pumped; the debug server reaches every window
   (`window_id` routing, a debug timer on windows created at runtime). RED first.
5. `scripts/azmail_e2e.py`: add account, sync, open, Reply window with the quote, type, Send to
   `scripts/azmail_smtp_sink.py` (SEND's), assert the sink's headers and the Sent folder.
6. Report `scripts/MAIL2_2026_10_01.md`.

## Decisions (unattended run)

- The window is `PimShell` (S4, an `OfficeShell` with navigation | list | reading + To-Do bar):
  the shell made for exactly this layout.
- The compose editor is azul's "Path 2": the app holds the body as a `Dom`, applies the engine's
  structural edits with `DocumentChangeset::apply_to_dom`, syncs typed text with
  `get_unsynced_text_edits`, and serializes its own model to text/plain + text/html on Send.
- `send.rs` is SEND's: AzMail codes only against the interface in the brief.

## Open questions

- (none yet)
