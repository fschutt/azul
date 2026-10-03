# MAIL1_AZMAIL progress

Branch `wt/mail1-azmail`, base `a7e18f4df`. Report: `scripts/MAIL1_AZMAIL_2026_09_30.md`.

## DONE

- 5bd9e7278 RED: `examples/azul-mail` (package AzMail, lib `azmail` + bin `AzMail`, link-dynamic,
  workspace member). Pure logic stated as tests over `todo!()` bodies.
- 15cbcc005 GREEN: the bodies.
- 613f9d491 imap_client.rs + lib.rs UI + sync Thread + keyring flow + azul-drawn title row (NoTitle).
- 47ab7f44c / cb767f967 RED / GREEN: tracking pixels, negative margins (exploration 1.4).
- a59d917e9 RED: test_imap_server.py (16 tests over a stub).
- edd4890e7 GREEN: imap_server.py (16/16 pass) + sample_mail/ + sync_e2e.py.
- 6f8cc1f6d / e0de9f33d RED / GREEN: a button goes with its label.
- 6f9b9cf56 supply chain: justifications, cargo-vet exemptions, hashify build-script policy.
- fde871e37 test fixture: FakeServer built with `with_folder`.
- Report `scripts/MAIL1_AZMAIL_2026_09_30.md` (the commit after fde871e37).

## IN PROGRESS

(none)

## NEXT (for whoever resumes)

1. Parent: `cargo test -p AzMail --lib`, then `python3 examples/azul-mail/scripts/sync_e2e.py`; commit Cargo.lock.
2. Swap `LocalFolder` for DRIVE1's `Drive` (store.rs; see the report).
3. SMTP send (lettre), compose (mail-builder), OAuth flow.

## Open questions

- The exploration (0132bf47d) recommends ammonia (html5ever) for the sanitizer; html.rs is a
  zero-dependency allow-list sanitizer with the same policy (ammonia 4.2.0 is inside the 14-day
  cooldown today, and the legacy rewrite needs a DOM pre-pass ammonia does not offer). The swap
  point is `html::sanitize`; the tests state the policy.
- Where an imported IMAP account sits in a user's S3 bucket (`users/<uid>/mail/` is the Azlin mailbox).
