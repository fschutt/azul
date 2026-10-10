# azlin-mail-worker

A Cloudflare Email Worker that files your domain's incoming mail into your Azlin drive. It runs
on **your** Cloudflare account (Email Routing sends each message to it); Azlin never runs a mail
server and never sees your mail. AzMail and AzDrive read the messages from the drive.

- **A plain drive:** each message's exact bytes go to `mail/Inbox/<stamp>-<hash>.eml` (or
  `mail/Spam/` when an upstream filter flagged it), the names AzMail uses
  (`examples/azul-mail/AZLIN_MAIL.md`). The same message delivered twice lands on the same name.
- **An encrypted drive:** each message is sealed to the drive's *drop key* before it leaves the
  Worker (AZD1: X25519, HKDF-SHA256, AES-256-GCM through Web Crypto; `src/azd1.js`) and stored
  at `.azlin/drop/<random>`. Only the drive's devices hold the drop key's secret; one of them
  opens the drop, files the message into the encrypted mail folders and deletes the drop.

The Worker logs nothing of a message. When the bucket does not answer, the delivery fails and
the sending server tries again later.

## Install

1. `npx wrangler deploy` in this folder (edit `wrangler.toml` first: the drive's endpoint,
   bucket and id).
2. `npx wrangler secret put S3_ACCESS_KEY_ID` and `... S3_SECRET_ACCESS_KEY`: the drive's
   credentials for incoming mail.
3. Cloudflare dashboard -> your domain -> Email -> Email Routing -> Routing rules: send the
   addresses you want (or the catch-all) to the Worker `azlin-mail-worker`.
4. For an encrypted drive, turn on incoming mail in the app: it makes the drive's drop key and
   sets `AZLIN_DROP_PUBLIC_KEY` on this Worker with **your own** Cloudflare API token (a token
   with "Workers Scripts: Edit" on your account; it goes from your computer to Cloudflare only).
   Without it the Worker writes plain messages.

## Test

```sh
npm test                                   # node --test: the AZD1 vector, SigV4, the handler
python3 ../../scripts/azlin_drop.py --vector   # the same vector in a standard-library reference
```

`test/azd1-vector.json` is the vector the Worker, the Python reference and azul-storage's Rust
side (`crypto::drop`) all check.
