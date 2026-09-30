# MAIL1_AZMAIL progress

Branch `wt/mail1-azmail`, base `a7e18f4df`. Report: `scripts/MAIL1_AZMAIL_2026_09_30.md` (at the end).

## DONE

(none yet)

## IN PROGRESS

- RED: `examples/azul-mail` (package AzMail, lib `azmail` + bin `AzMail`, link-dynamic, workspace
  member). Pure logic stated as tests over `todo!()` bodies: account.rs, auth.rs, mutf7.rs,
  folders.rs, store.rs, message.rs, html.rs, sync.rs (FakeServer).

## NEXT

1. GREEN: fill the bodies.
2. imap_client.rs (imap 3.0.0-alpha.15 over AzMail's own rustls stream) + lib.rs UI + sync Thread.
3. Python IMAP test server (RED unittest first) + sample mail.
4. sync_e2e.py (headless, debug server).
5. dependency-justifications.toml + supply-chain exemptions for the new crates.
6. Report.

## Open questions

- `scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md` (wt/x-mail-explore) was not written yet when
  this started; crate choices are this task's own (see the report).
