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

## DONE

## IN PROGRESS
- micromail 0.2.0 on branch azul-send-2026-10-01

## NEXT
- examples/azul-mail/src/send.rs, bin azmail-send, scripts, tests, report

## Open questions
