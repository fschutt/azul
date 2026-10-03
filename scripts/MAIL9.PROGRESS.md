# MAIL9 progress - AzMail: authenticated submission through a crate; remote images and fonts

Branch `wt/mail9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MAIL9". Rules: scripts/waves/house_rules.md.
Never compile; commit after every unit; report scripts/MAIL9_2026_10_03.md.

## Decisions (made unattended)
- CRATE: `lettre` 0.11 (blocking `SmtpConnection`, features `smtp-transport`, `rustls-no-provider`,
  `webpki-roots`, no default features), not Stalwart's `mail-send` 0.6: mail-send is async-only (a tokio
  runtime per send), verifies only through the OS platform verifier or not at all (no way to trust a test CA
  without turning verification off), and would bring a second MIME builder; lettre is blocking (fits the azul
  Thread), takes webpki roots + an extra root certificate, does implicit TLS and STARTTLS, AUTH PLAIN / LOGIN /
  XOAUTH2, and sends raw bytes - so micromail's builder + DKIM stay the one message generator. New crates:
  lettre + email_address only (base64 0.23, socket2 0.6, url, idna, nom 8, percent-encoding, rustls 0.23,
  webpki-roots 1.0 are already in Cargo.lock). Crypto: the RustCrypto provider AzMail's IMAP uses, installed
  as the process default once (`rustls-no-provider` asks for it).
- micromail untouched: Direct and the unauthenticated `Smtp` relay route still go through it.
- ROUTE: new `SendRoute::Submission` (`{"kind":"submission"}`) = the account's own SMTP server from
  account.json (`smtp`), signed in with the account's user name and secret (IMAP app password / OAuth token).
  Default for an account whose address has a provider preset when sending.json does not exist yet
  (`SendSettings::load` reads account.json then); the wizard proposes it for such addresses.
- Security by port (RFC 8314): 465 implicit TLS, anything else STARTTLS required; no TLS (`tls: off`) only
  to this computer (a test server). Never a password in the clear to another computer.
- Sign-in choice: the one chooser `auth::choose` (IMAP's), with SMTP's EHLO AUTH list as caps; for SMTP an
  OAuth token needs XOAUTH2 offered (`auth::choose_submission`).
- A mail that waits for the user (no password in memory yet, sign-in refused, a setting that cannot work)
  stays QUEUED and due at once (the attempt is not counted, no backoff): the next Send / Receive - which
  reading the keyring or saving Account Settings starts - sends it. Unreachable server: queued with backoff.
- Test SMTP server: the existing stdlib sink scripts/azmail_smtp_sink.py grows AUTH (PLAIN / LOGIN /
  XOAUTH2) and implicit TLS instead of a second server on aiosmtpd (not installed here; one server per
  concern).
- Remote content: the pre-pass is `Xml::scan_external_resources` on the parsed mail (images, fonts,
  stylesheets; http(s) only); the fetch list is the scan's images the sanitizer shows (no tracking pixel),
  capped; fetched on an azul Thread through azul's own HTTP client (`HttpRequestConfig::download_bytes`
  blocks the worker, not the UI); `cid:` pictures from the mail's own parts always shown.

## DONE
- (none yet)

## IN PROGRESS
- reading the code, progress file

## NEXT
- RED: route choice + auth mechanism selection tests (send.rs, auth.rs, sending.rs, account.rs)

## Open questions
- Fonts: azul has no runtime "register a font under a family name" API (FontManager::register_named_font
  exists, but no CallbackInfo entry, and the per-regenerate fc_cache swap drops memory faces). See report.
