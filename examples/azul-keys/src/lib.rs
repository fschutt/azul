//! AzKeys: the password manager of the Azlin apps (the plan:
//! azul-apps/planning/core/password-manager.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`crypto`]: the vault file - Argon2id derives the password key that seals a random vault
//!   key, XChaCha20-Poly1305 seals the vault's JSON with it;
//! - [`vault`]: the items (logins, cards, notes, identities, SSH keys), search, scopes, sections;
//! - [`totp`]: one-time codes (RFC 4226 / 6238), otpauth URLs, base32;
//! - [`import`]: CSV from browsers and password managers, Bitwarden JSON;
//! - [`generator`]: passwords, passphrases ([`words`]) and PINs, the strength estimate;
//! - [`clipboard`]: the clipboard guard (a copied secret is cleared after a while);
//! - [`lock`]: the idle lock and the wait after wrong master passwords;
//! - [`audit`]: weak, reused and old passwords, logins without a one-time code;
//! - [`store`]: the files in the data tree and the work on them (list, create, unlock, save).

pub mod audit;
pub mod clipboard;
pub mod crypto;
pub mod generator;
pub mod import;
pub mod lock;
pub mod store;
pub mod totp;
pub mod vault;
pub mod words;

/// Starts AzKeys (the window comes with the UI commits).
pub fn start() {}
