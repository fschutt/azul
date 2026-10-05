# KEYS9 progress - AzKeys (password manager)

Branch `wt/keys9` from `e537ddbe2`. Worktree `.claude/worktrees/agent-ad62e1cc06f76a821`.
Report: `scripts/KEYS9_<date>.md` (when done). Never compile (house rules); rustfmt only as a parse check.

## DONE (plain model, all with tests)
- 5776c612c progress file
- e31b6b9de RED: crate examples/azul-keys (Cargo.toml, lib.rs, main.rs), vault.rs (model + tests),
  crypto.rs / totp.rs / import.rs stubs + tests
- 6ae7022e3 GREEN crypto.rs (Argon2id + XChaCha20-Poly1305 envelope, key check value)
- f59738a49 GREEN totp.rs (HOTP/TOTP, base32, otpauth)
- 945e7e0c6 GREEN import.rs (CSV exports by header, Bitwarden JSON, merge)
- a3fce79b1 RED generator.rs / words.rs (1024 words) / clipboard.rs / lock.rs / audit.rs
- 7755d4af0 GREEN generator.rs; 580a990fa GREEN clipboard.rs + lock.rs; f50d18b3a GREEN audit.rs
- a18b6e625 RED store.rs (file keys, Work/Done/Failure, run(), read_listing) + azul-pim test-util dev-dep
- 46a58d5f7 GREEN store.rs; c91f0a2a0 sample.rs (+ tests, SAMPLE_PASSWORD "sample")
- 007d2b35e ids.rs; ed43239d6 RED + 5adc26c13 GREEN session.rs (Session, OpenVault, Form, Reading, Reveal,
  ImportView: the plain window model; UI callbacks should only call these)
- 1e6b65eb1 app.rs (SPEC/ABOUT/SHORTCUTS/APP_CATEGORIES, Settings, DeviceUnlock, KeysApp state, forms)
- 161a2ebae jobs.rs (vault thread spawn/on_done/handle, save/lock/finish_lock, copy_secret/set_clipboard,
  start_timer/on_tick, keyring + biometric (enable/disable/unlock_with_device, on_keyring_result,
  on_biometric_result), on_window_created, on_close_requested, on_activity). NOT yet in lib.rs (needs ui.rs).
- 3127cacbd ui.rs part 1 (layout, unlock, create); 1aa0372bb ui.rs part 2 (vault PimShell, nav, list, toolbar,
  status, on_key, settings sections, read_import_file/on_import_read, close guard)
- 6ed5bf219 / dd42887e1 / 1ebd7aff1 ui_item.rs (item view, edit form, generator, import preview, audit)
- 19f2dc90b lib.rs start() + `pub mod jobs; ui; ui_item;` - THE APP IS COMPLETE IN CODE (uncompiled)
- 651b4d772 wrong password empties the field; 4e9ac65d4 scripts/azkeys_e2e.py; d5c9fa8aa TODO wording
- report scripts/KEYS9_2026_10_03.md
- 9cbc73f1a registered: root Cargo.toml member, workspace_test_members.txt, rust.yml step (NEXT item 7 done)

## STATE: DONE in code - report scripts/KEYS9_2026_10_03.md committed. Remaining (Monday / the parent):
compile + fix, run `cargo test -p AzKeys --lib`, run scripts/azkeys_e2e.py through the capped runner.

## (old) NEXT (exact) - items 1-7 are DONE; continue at 8
- 8a. self-review pass over ui.rs / ui_item.rs / jobs.rs for compile errors (imports unused/missing, borrows).
- 8b. scripts/azkeys_e2e.py (model on scripts/shells_e2e.py): --data-dir tmp --sample; wait AZKEYS_LISTED/CREATED;
  lock (toolbar-lock), unlock with "sample" (focus + text_input __azkeys_unlock-password, click unlock-button),
  wrong password message, search "codehost", click row, copy password (AZKEYS_COPIED), new login form + save,
  generator panel, audit panel; assert node texts.
- 8c. report scripts/KEYS9_2026_10_03.md (or the finish date).
1. (done) GREEN `examples/azul-keys/src/store.rs`: fill `read_listing` (parse each `(key, bytes)` with
   `Envelope::parse`, problems "`<key>`: <VaultError>", sort by folded name) and `run` (List via
   `azul_appkit::files::list_all(drive, VAULTS)` + get each `.azkv`; Create: KdfParams::fresh() when None,
   `Envelope::create(&vault.id, &vault.name, ..)`, put `file_key`; Unlock: get -> parse -> unwrap_key ->
   open_data -> Vault::from_json, then vault.id/name = envelope's; UnlockWithKey: same with the key; Save:
   `drive.copy(key, backup_key(id))` (ignore error) then `envelope.reseal` + put; ChangePassword: `rewrap` + put;
   Put). Failures via `Failure::of` / `Failure::io`; a missing file = message "the vault file is gone".
   Needs `azul-appkit` (already a dependency; plain module `files` has `list_all`).
2. (done) sample.rs: `sample_vault(now) -> Vault` (~40-128 fictional items: CodeHost (example) dev@example.org with
   TOTP secret GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ, Mail (Box example), Bank (card) 4242, identity Anna Berg,
   SSH keys, notes, some weak / reused / old passwords for the audit); `SAMPLE_PASSWORD = "sample"`; a test.
3. (done) ids.rs: `__azkeys_` const AzString names (macro like examples/azul-contacts/src/ids.rs).
4. lib.rs: SPEC / ABOUT (app_folder "keys") / SHORTCUTS / APP_CATEGORIES, `KeysApp` state, Settings
   (idle_minutes default 5, clear_seconds default 30, `device_unlock.<vault-id>` = biometric|prompt|keyring),
   `start()` like AzContacts' ui::start (AppArgs::from_env, kit::create_kit, kit::window_options).
5. jobs.rs: vault Thread (init {root, Option<Work>}, `store::run(&LocalDrive::new(root), work)`, WriteBack
   `Done`), one job at a time (+ save_again), 1 s Timer (TOTP, clipboard clear via set_clipboard_content empty,
   auto-lock -> `vault.wipe()`), keyring ops (one in flight, like AzDrive lib.rs `KeyringOp`), biometric
   (`request_biometric_auth`, WindowEventFilter::BiometricResult / KeyringResult), CloseRequested (hold while
   saving), activity touch on window MouseDown / VirtualKeyDown.
6a. NEXT: `examples/azul-keys/src/ui_item.rs` - ui.rs calls: `reading_pane(s: &KeysApp, session: &Session, app: &RefAny)
   -> Dom`, `regenerate(session: &mut Session)`, extern "C" `on_edit`, `on_edit_save`, `on_escape`, `on_delete`
   (all `(RefAny, CallbackInfo) -> Update`). Content: item view (fields with copy / reveal buttons via
   jobs::copy_secret, TOTP code + ProgressBar `TODO(WIDGETS9B): Gauge`, tags Chips, history, Edit / Delete with
   confirm), edit form (kind Segmented for a new item, title, username, password + strength + Generate, totp,
   website, tags input + chips, card fields, custom fields add/remove, notes TextArea, Save / Cancel, discard
   confirm), generator panel (Segmented mode, Slider length/words, CheckBoxes, output, strength, Refresh / Copy /
   Use password (into the open form)), import preview (path TextInput, Choose... FileDialog::open_file ->
   ui::read_import_file, summary, skipped list, Import -> import::merge + jobs::save), audit view (summary
   cards as buttons -> Filter, rows, Export report -> Work::Put store::audit_key(date)).
6b. then lib.rs: `pub mod jobs; pub mod ui; pub mod ui_item;` + start() (see below); remove the "Mod+F Search"
   shortcut from app.rs SHORTCUTS (not implemented; array 13 -> 12).
6. ui.rs (DONE): must define `pub fn read_import_file(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny,
   path: &Path)` (jobs.rs calls it) and `pub extern "C" fn layout(RefAny, LayoutCallbackInfo) -> Dom`; body callbacks:
   VirtualKeyDown (kit::handle_key first), MouseDown -> jobs::on_activity, KeyringResult -> jobs::on_keyring_result,
   BiometricResult -> jobs::on_biometric_result, CloseRequested -> jobs::on_close_requested. Then lib.rs:
   `mod jobs; pub mod ui;` + `pub fn start()` (AppArgs::from_env(&app::SPEC), kit::create_kit(SPEC, ABOUT, &SHORTCUTS,
   &APP_CATEGORIES, args), Settings::from_values, KeysApp {..}, kit::window_options(&kit, ui::layout, (1180.0, 760.0),
   (720.0, 480.0), jobs::on_window_created), App::create(..).run(window)); println!("AZKEYS_DATA {}").
   Old plan for ui.rs: unlock screen (vault DropDown, TextInput::create_password, Unlock, "Use Touch ID", create-vault form,
   Attempts message), PimShell (nav: All / Favourites / kinds / One-time codes / tags; list with Avatar + search;
   reading pane: item detail with copy / reveal / TOTP code + ProgressBar ring `TODO(WIDGETS9B): Gauge`; edit form;
   generator panel (Segmented mode, Slider length, CheckBoxes, ProgressBar strength); import screen (FileDialog ->
   appkit spawn_outside_read -> import::import_file -> preview -> merge); audit screen), toolbar, status bar
   (lock / clipboard countdowns), settings sections (Security, Vault: change master password, device unlock),
   CloseGuard for an edited item. Avoid DOM rebuilds every second while a form is being edited.
7. (done) Register: root Cargo.toml members (append after examples/azul-dashboard), scripts/workspace_test_members.txt
   (`tested: AzKeys`), .github/workflows/rust.yml step after "Run AzDashboard tests".
8. scripts/azkeys_e2e.py (debug server, model on scripts/shells_e2e.py), report.

## Decisions
- Crypto: argon2 0.5.3 (NEW, + blake2 0.10) + chacha20poly1305 0.10.1 (in lock) + zeroize + getrandom 0.2 + sha2;
  NOT age (big new tree, scrypt-only passphrases, recipients model fits backups not a keyring-wrapped key).
- Vault file `keys/vaults/<uuid>.azkv`: JSON envelope, vault key (random) wrapped by the Argon2id password key;
  keyring stores the vault key (base64) for biometric unlock; `check` = SHA-256(label||key)[..16] tells
  WrongKey from Corrupt.
- base32 written in totp.rs (20 lines, RFC 4648 vectors) rather than a new crate; base64 = azul_pim::data_uri.
- CSV via the csv crate (AzSheets has it); AzContacts' hand-written csv::parse is a twin to report.
- Clipboard: azul reads the clipboard only during a paste, so the guard clears unconditionally at the end of the
  countdown (keeps only a digest); a "concealed" pasteboard flag (macOS org.nspasteboard.ConcealedType) is an
  engine gap to report.
- Wrong password: after 3 wrong, wait 30 s doubling to 5 min (lock.rs Attempts).

## Least sure to compile (so far)
- argon2 feature names `alloc`, `zeroize` (0.5.3); `argon2::Params::new(m, t, p, Some(32))`, `hash_password_into`.
- chacha20poly1305 0.10 `aead::{Aead, Payload}` with feature `alloc`; `Key::from_slice`, `XNonce::from_slice`.

- House rule (coordinator, 2026-10-03): never send personal data to an outside service; AzKeys makes no network
  calls (no breach check, no favicon fetch) - keep it so.

## Open questions
- (none)
