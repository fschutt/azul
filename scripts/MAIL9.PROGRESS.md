# MAIL9 progress - AzMail: authenticated submission through a crate; remote images and fonts

Branch `wt/mail9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MAIL9". Rules: scripts/waves/house_rules.md.
Never compile; commit after every unit; report scripts/MAIL9_2026_10_03.md.

## ROUTE PRIORITY CHANGED BY USER DECISION (2026-10-03, via the coordinator)
"client-side DKIM ... I'd really like to have this first. later on we can relay, goal is to keep the server as
dumb as possible". Order now:
1. FIRST: DIRECT delivery with client-side DKIM end to end (micromail's direct MX path, untouched): key
   generation in the client (private key in the OS keyring), the exact DNS TXT record
   `<selector>._domainkey.<domain>` + DMARC / SPF notes, every outgoing mail signed, the signature verified by
   an independent verifier, understandable failures (outbound port 25 blocked, a receiver's 5xx for home
   addresses - Spamhaus PBL 5.7.1 -, missing PTR) recorded in the per-domain policy list for the later relay.
2. SECONDARY: authenticated submission through a crate - an OPTIONAL route, NOT the default.
3. The remote-content pre-pass as briefed.

## Decisions (made unattended)
- DKIM key: RSA 2048 through micromail's `generate_rsa_key_pem` (feature `signing`, micromail itself untouched);
  the DNS record is AzMail's own: `p=` is the SubjectPublicKeyInfo (what OpenDKIM's `d2i_PUBKEY` and every
  provider publish), not micromail's `format_dkim_dns_record`'s PKCS#1 (listed as a micromail note in the
  report). Public half kept in sending.json (`dkim.public_key`), private half only in the keyring.
- Signing moves to the attempt: the outbox keeps the UNSIGNED message, each attempt signs it with the key in
  memory; a mail whose account signs but whose key is not in memory yet waits (queued, due at once).
- A signed mail is tried directly even to a domain the SHIPPED defaults mark as needing a relay (gmail.com,
  yahoo.com - those defaults were about unsigned mail from home); a LEARNED refusal still holds it back.
- Independent DKIM verifier: python + the `openssl` command (RFC 6376 relaxed canonicalization written in the
  test, RSA-SHA256 checked by OpenSSL) in scripts/azmail_send_test.py; dkimpy when installed. No mail-auth
  dev-dependency (0.13 needs ed25519-dalek 3, rand 0.10, sha1/sha2 0.11, hashify, similar: 5+ new crates).
- Port 25 probe: when every exchanger of a domain could not even be connected to (no reply), AzMail probes
  well-known exchangers on port 25; all unreachable while DNS works = the connection blocks port 25: recorded
  in send_policy.json (`port25`), every direct mail waits for a relay, the reason says so.
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
- ROUTE (secondary, optional): new `SendRoute::Submission` (`{"kind":"submission"}`) = the account's own SMTP
  server from account.json (`smtp`), signed in with the account's user name and secret (IMAP app password /
  OAuth token). NOT the default (user decision): Direct stays the default.
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
- 000768b4e / f6c5143d8 progress file, route priority change recorded
- 2f998b457 RED dkim.rs tests; 488255954 GREEN dkim.rs (key, SPKI, record, zone line, notes, TXT check)
- d7cb18ec8 RED send.rs (signed vs defaults, unsigned outbox + sign at attempt, refusal causes, port 25);
  35ec74153 GREEN send.rs

## IN PROGRESS
- wiring DKIM into the app (Sending page: key creation, record, notes, DNS check; keyring store / read;
  the compose job and the Send / Receive retry hand the key over)

## NEXT
- send.rs module doc; ui_account.rs DKIM section; lib.rs keyring queue (IMAP secret + DKIM key);
  ui_compose OutgoingJob.dkim_key; sync thread retry dkim_key; azmail-send `--dkim-generate`,
  `--port25-probe`; E2E: python+openssl DKIM verifier in scripts/azmail_send_test.py (`dkim` full b=
  check, `dkim-generated`, `port25`)

## Open questions
- Fonts: azul has no runtime "register a font under a family name" API (FontManager::register_named_font
  exists, but no CallbackInfo entry, and the per-regenerate fc_cache swap drops memory faces). See report.
