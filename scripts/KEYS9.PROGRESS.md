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
- 9cbc73f1a registered: root Cargo.toml member, workspace_test_members.txt, rust.yml step (NEXT item 7 done)

## NEXT (exact) - items 1, 2, 3 and 7 are DONE (+ session.rs); continue at 4
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
6. ui.rs: unlock screen (vault DropDown, TextInput::create_password, Unlock, "Use Touch ID", create-vault form,
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

## Open questions
- (none)
