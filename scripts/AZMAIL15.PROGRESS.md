# AZMAIL15 progress

Task: AzMail "Azlin" account kind - mail as .eml objects under `mail/` in the user's Azlin drive
(S3), signed in through the token server; seed script, mock stack, `--phase azlin` e2e; audit
AzMail for hard-coded endpoints. Worktree branch `worktree-agent-a53601a4878811705`
(fast-forwarded to 5d78255a4). No compiling here (house rule): the lead builds and runs tests.

## DONE
- (nothing committed yet)

## IN PROGRESS
- examples/azul-mail/AZLIN_MAIL.md (the layout, naming, convergence, 50 MB)

## NEXT
1. azlin.rs: session, token server client (trait CloudAccount), naming, markers, endpoints
2. azlin_sync.rs: bucket -> local cache sync, push of local-only mail, flags, move / delete /
   draft upload, on-demand fetch of big messages
3. account kind "azlin" (account.json v2), the wizard's Azlin drive page, Send/Receive glue
4. ribbon: Archive / Delete / Junk / Move for Azlin accounts; drafts upload
5. scripts: azlin_mock_stack.py (Python token server + AzDrive's stdlib S3),
   azmail_seed_azlin.py, azlin_token_conformance.py, azmail_e2e.py --phase azlin
6. hard-coding audit of AzMail

## Open questions
- AZCLOUD15 owns the `endpoints` section of azul-appkit's azlin_config.rs; AzMail reads it
  through ONE function (azlin::endpoints_from_config) to be re-pointed at the typed accessor at
  integration. HARDCODE15 (another agent) audits hard-coding across azul + azul-apps.
- azul-apps bug (read-only here): azlin-client's TokenServer::refresh posts to
  /v1/drives/{id}/refresh; the token server serves /v1/drives/{id}/credentials.
