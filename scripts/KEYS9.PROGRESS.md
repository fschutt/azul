# KEYS9 progress - AzKeys (password manager)

Branch `wt/keys9` from `e537ddbe2`. Worktree `.claude/worktrees/agent-ad62e1cc06f76a821`.
Report: `scripts/KEYS9_2026_10_03.md` (when done).

## DONE
- 5776c612c progress file
- e31b6b9de RED: crate examples/azul-keys (Cargo.toml, lib.rs, main.rs), vault.rs (model, implemented + tests),
  crypto.rs / totp.rs / import.rs stubs + tests
- 6ae7022e3 GREEN crypto.rs (Argon2id + XChaCha20-Poly1305 envelope, key check value)
- f59738a49 GREEN totp.rs (HOTP/TOTP, base32, otpauth)
- 945e7e0c6 GREEN import.rs (CSV exports by header, Bitwarden JSON, merge)

## IN PROGRESS
- next plain modules: generator.rs (password / passphrase / PIN + strength), clipboard.rs (ClipboardGuard),
  lock.rs (auto-lock + wrong-password back-off), audit.rs (weak / reused / old / no 2FA), store.rs (keys,
  vault listing), sample.rs

## NEXT
- RED+GREEN for generator / clipboard / lock / audit
- store.rs + jobs.rs (azul Thread: list vaults, unlock (Argon2id off the UI thread), create, save, change password)
- ids.rs (const AzString `__azkeys_` names), args (appkit AppArgs, screens), ui.rs (unlock screen, PimShell vault,
  item detail + edit form, generator panel, import screen, settings sections), keyring / biometric unlock
- register: root Cargo.toml members, scripts/workspace_test_members.txt, .github/workflows/rust.yml step
- scripts/azkeys_e2e.py; report scripts/KEYS9_2026_10_03.md

## Decisions
- Crypto: argon2 0.5.3 (NEW, + blake2 0.10) + chacha20poly1305 0.10.1 (in lock) + zeroize + getrandom 0.2 + sha2;
  NOT age (big new tree, scrypt-only passphrases, recipients model fits backups not a keyring-wrapped key).
- Vault file `keys/vaults/<uuid>.azkv`: JSON envelope, vault key (random) wrapped by the Argon2id password key;
  keyring stores the vault key (base64) for biometric unlock; `check` = SHA-256(label||key)[..16] tells
  WrongKey from Corrupt.
- base32 written in totp.rs (20 lines, RFC 4648 vectors) rather than a new crate; base64 = azul_pim::data_uri.
- CSV via the csv crate (AzSheets has it); AzContacts' hand-written csv::parse is a twin to report.

## Open questions
- (none)
