# MAIL1_AZMAIL progress

Branch `wt/mail1-azmail`, base `a7e18f4df`. Report: `scripts/MAIL1_AZMAIL_2026_09_30.md` (at the end).

## DONE

- 5bd9e7278 RED: `examples/azul-mail` (package AzMail, lib `azmail` + bin `AzMail`, link-dynamic,
  workspace member). Pure logic stated as tests over `todo!()` bodies: account.rs, auth.rs,
  mutf7.rs, folders.rs, store.rs, message.rs, html.rs, sync.rs (FakeServer).

- 15cbcc005 GREEN: the bodies.

## IN PROGRESS

- imap_client.rs (imap 3.0.0-alpha.15 over AzMail's own rustls stream) + lib.rs UI + sync
  Thread + keyring flow + azul-drawn title row (NoTitle) (this commit).

## NEXT

1. RED/GREEN: tracking pixels leave no placeholder, negative margins dropped (exploration 1.4).
3. Python IMAP test server (RED unittest first) + sample mail.
4. sync_e2e.py (headless, debug server).
5. dependency-justifications.toml + supply-chain exemptions for the new crates.
6. Report.

## Open questions

- The exploration (0132bf47d) recommends ammonia (html5ever) for the sanitizer; html.rs is a
  zero-dependency allow-list sanitizer with the same policy (ammonia 4.2.0 is inside the 14-day
  cooldown today, and the legacy rewrite needs a DOM pre-pass ammonia does not offer). The swap
  point is `html::sanitize`; the tests state the policy.
