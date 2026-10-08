# AZMAIL15 progress

Task: AzMail "Azlin" account kind - mail as .eml objects under `mail/` in the user's Azlin drive
(S3), signed in through the token server; seed script, mock stack, `--phase azlin` e2e; audit
AzMail for hard-coded endpoints. Worktree branch `worktree-agent-a53601a4878811705`
(fast-forwarded to 5d78255a4). No compiling here (house rule): the lead builds and runs tests.

## DONE
- 84c184e5c docs: examples/azul-mail/AZLIN_MAIL.md (layout, naming, convergence, 50 MB)
- 0e8efd2df azlin.rs: names, markers, AzlinSession, CloudAccount + TokenServer, Endpoints (tests)
- 48f959d2c azlin_sync.rs: sync_account, move / delete / upload / push_marks / fetch (tests);
  IndexEntry.remote, FolderReport.pushed/removed

## IN PROGRESS
- account kind "azlin" (account.json v2) + wizard + Send/Receive glue

## NEXT
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
