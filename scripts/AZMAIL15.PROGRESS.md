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
- 7fcaa56bc account kind "azlin" (account.json v2), AccountForm, MailArgs switches
- e9deeec7e UI: wizard Azlin page, Send/Receive (refresh + rotated token to keyring), Archive /
  Junk / Delete / Move, marks pushed, big message fetched on open, drafts uploaded

- 070a36791 scripts: azlin_mock_stack.py, azlin_client.py, azlin_token_conformance.py (--mock
  passes 14/14 here)
- c8a0ade5b scripts/azmail_seed_azlin.py (checked against the mock)
- b24dfcde6 scripts/azmail_e2e.py --phase azlin (stack + seed steps dry-run here; AzMail itself
  not run: no compiling)
- a26a8a190 audit fix: --dns-servers / AZMAIL_DNS_SERVERS for the DKIM / DMARC / SPF check
- 1a8588274 pushed / uploaded mail keeps \Seen in the drive (markers)
- e583f3c8a docs; bf97061ec / 0eceed0ef e2e isolation (AZLIN_DATA, AZLIN_CONFIG=off in every
  phase, sync_e2e.py too); ead001ae8 conformance takes long-lived keys

## IN PROGRESS
- (none: handed to the lead)

## NEXT (for the lead)
- compile + `cargo test -p AzMail` (azlin, azlin_sync, account, args, dkim tests are new)
- `python3 scripts/azmail_e2e.py --phase azlin` (mock), then `--azlin-stack local` with
  `azctl dev up --processes`; `python3 scripts/azlin_token_conformance.py --token-url
  http://127.0.0.1:8081`

## Open questions
- AZCLOUD15 owns the `endpoints` section of azul-appkit's azlin_config.rs; AzMail reads it
  through ONE function (azlin::endpoints_from_config) to be re-pointed at the typed accessor at
  integration. HARDCODE15 (another agent) audits hard-coding across azul + azul-apps.
- azul-apps bug (read-only here): azlin-client's TokenServer::refresh posts to
  /v1/drives/{id}/refresh; the token server serves /v1/drives/{id}/credentials.
