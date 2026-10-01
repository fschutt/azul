# SEND progress (branch wt/send, base 39092feee)

## Decisions
- The one SMTP client is micromail (the user's crate). Its fixes and the new raw-message API land in
  ~/Development/micromail on branch `azul-send-2026-10-01` (from origin/master 4584146 = the published
  0.1.0), as micromail 0.2.0 (never pushed). AzMail consumes it through a TEMPORARY
  `[patch.crates-io] micromail = { path = "../micromail" }` in the root Cargo.toml until 0.2.0 is
  published; 0.2.0 does not match layout's `micromail = "0.1.0"`, so the crash-mail build (and the dll)
  keeps the registry 0.1.0 tonight and cannot be broken by this branch.
- The MIME builder lives in micromail too (one generator for AzMail and, later, crash mail).
- DKIM: micromail's `dkim` feature signs with mini-mail-auth 0.1.0 (crates.io; rsa/sha2/base64 already
  in azul's tree).
- New AzMail code (send.rs) came with its tests in one commit (no stub-RED split: a new module, not a
  behaviour change); the micromail bug fixes are RED first (10b1a2b, then 7ad0c3f).

## DONE
- micromail 10b1a2b test(smtp): four wire-level REDs against a local sink
- micromail 7ad0c3f feat!: 0.2.0 (send_raw, verified STARTTLS, MessageBuilder, real DKIM)
- 86b60af66 chore(send): progress file
- 311a1dad0 feat(azmail): send.rs + azmail-send + the temporary micromail patch + supply chain
- 72642e297 test(azmail): scripts/azmail_smtp_sink.py + scripts/azmail_send_test.py
- 67d0d347a chore(send): progress
- mini-mail-auth (branch azul-send-2026-10-01): 0f0592c + 62e06de REDs, 0e73dd3 fix (body
  canonicalized by cb, simple keeps its last CRLF; 0.1.1)

## IN PROGRESS
- report scripts/SEND_2026_10_01.md (next step: write it, commit it with this file)

## NEXT
- (parent) build, run the suites listed in the report, publish micromail 0.2.0, drop the patch

## Open questions
- Sync renumbering: MAIL1's sync moves mail/sent aside when the server's UIDVALIDITY differs from the
  local state (0 for a Sent folder AzMail created); see the report, follow-up for MAIL2.
