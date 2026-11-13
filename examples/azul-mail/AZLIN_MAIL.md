# Mail on the Azlin drive

Azlin never runs a mail server. A mailbox is a folder of files in the user's own bucket (the
Azlin drive): every message is one `.eml` object under `mail/`, and what the user did to it
(read, flagged, labelled) is a few empty objects next to it. AzMail reads and writes them with
plain S3 calls (SigV4, path style); the token server only hands out the drive's credentials.
Everything is browsable as files in AzDrive.

Incoming mail is not AzMail's job: later the customer's own Cloudflare Email Worker writes each
message into `mail/Inbox/` (or `mail/Spam/`) by the naming rule below (section 9). Sending stays
on this computer (section 8).

## 1. Layout

```text
mail/<Folder>/<name>.eml              one message: the exact RFC 5322 bytes, never rewritten
mail/<Folder>/<Sub>/<name>.eml        a folder in a folder (an S3 "folder" is a key prefix)
mail/.state/<id>/seen                 empty object: the message is read
mail/.state/<id>/flagged              empty object: flagged for follow-up
mail/.state/<id>/answered             empty object: replied to (read; AzMail does not set it yet)
mail/.state/<id>/label/<label>        empty object: a label (category), the name percent-encoded
mail/.state/<id>/deleted              empty object: marked deleted by another program, not expunged (hidden)
mail/.index/                          reserved for a folder summary cache (not written, see 4.4)
```

- **Folders.** `Inbox`, `Sent`, `Drafts`, `Archive`, `Spam` and `Trash` are the well-known
  names, written with this spelling and read in any case (a folder called `Junk`, `Junk E-mail`,
  `Sent Items`, `Deleted Items`, ... takes the role when the well-known one is missing: the same
  rules as an IMAP server's folders, `folders.rs`). Every other name is a folder of the user's.
  Spam is literally the folder `mail/Spam/`.
- **A folder exists while it holds an object** (S3 has no empty folders). AzMail shows the six
  well-known folders always and creates them in the bucket by putting the first message in.
- **Names that start with `.`** are AzMail's bookkeeping, never a folder of mail; AzMail refuses
  to create a folder whose name starts with `.`.
- `<id>` is a message's name without `.eml` (section 2).

## 2. Naming: stable and collision-free

```text
<stamp>-<hash>.eml        20261008T091500Z-3f2a9c1e5b7d4a60.eml
```

- `<stamp>`: when the message arrived in the drive, UTC, `YYYYMMDDTHHMMSSZ` (the SigV4 date
  format). The Worker's receive time for incoming mail; the time it was written for a sent mail
  or a draft; the Date header for imported mail.
- `<hash>`: the first 16 hex digits (64 bits) of SHA-256 over the exact bytes.

What this buys:

- **Stable.** A name never changes. A move keeps it (`mail/Inbox/X.eml` becomes
  `mail/Archive/X.eml`), so everything keyed by `<id>` (the state markers) follows the message
  without being touched.
- **Collision-free.** Two different messages get different hashes; with 64 bits a mailbox of a
  million messages has a collision chance of about 3 in 100 million, and the stamp in front
  makes it smaller still (both would have to arrive in the same second).
- **Idempotent.** The same bytes stored twice (a Worker retry, a second upload of the same sent
  mail after a crash) land on the same key: no duplicate.
- **Sorted.** Keys sort by arrival time, so a folder listing comes in time order and the
  newest mail is on the last listing page.
- A **draft** is rewritten whole on every save: new bytes, so a new name. AzMail puts the new
  object first and deletes the old one after; a crash between the two leaves two drafts, never
  none.
- A file someone put into a folder by hand (`invoice.eml` through AzDrive) is a message too: its
  `<id>` is its own name, its arrival time the object's LastModified.

## 3. What every action writes

Every user action is ONE idempotent object operation (or two, for a move):

| Action | Objects |
| --- | --- |
| read / unread | PUT / DELETE `mail/.state/<id>/seen` (empty) |
| flag / unflag | PUT / DELETE `mail/.state/<id>/flagged` |
| label / unlabel | PUT / DELETE `mail/.state/<id>/label/<label>` |
| mark deleted / undelete (a mail program through the Azlin Bridge, section 11) | PUT / DELETE `mail/.state/<id>/deleted`; AzMail hides a message while it has this marker |
| move (Archive, Junk, Move to) | CopyObject to `mail/<To>/<name>.eml`, then DeleteObject of the old key; the markers stay |
| delete | a move to `mail/Trash/`; deleting in Trash deletes the object, then its markers |
| save a draft | PUT `mail/Drafts/<new name>.eml`, then DELETE the draft it replaces |
| send | nothing in the bucket at once: the sent copy is filed in Sent and put into `mail/Sent/` by the next Send/Receive (section 8) |

## 4. The index, and how two devices converge

### 4.1 The bucket is the truth

The listing of `mail/<Folder>/` IS the folder's index (names in time order, sizes, dates); the
listing of `mail/.state/` is every message's state. No object is ever read, changed and written
back, so no device can undo another device's write by writing a stale copy - there is no shared
index object that could lose an update.

### 4.2 Concurrent actions

Two devices acting on the same marker at the same time: the bucket applies the two requests in
some order and the last one stays. That is the rule IMAP's `STORE` gives two clients of one
server. Other cases:

- Two devices move the same message to different folders at once (copy, copy, delete, delete):
  it ends up in both folders. A visible duplicate, never a loss.
- A move and a delete at once: whichever came last wins; a move whose copy finds the message
  gone reports it and changes nothing.
- A new message arriving while a device marks others read: different keys, no interaction.

### 4.3 How a device catches up

Send/Receive on a device:

1. **Push** what this device did offline: mail it filed itself (a sent mail, a draft saved
   without a connection) has no object yet and is put into its folder; read and flag marks
   made here and not yet written are written (the local `flags.json` holds them until then).
2. **Pull**: one listing per folder (one ListObjectsV2 page per 1000 messages) and one
   recursive listing of `mail/.state/`. New names are fetched, the flags come from the markers.
   A name that is gone from the listing leaves the folder only when the token server, asked
   then (`GET /v1/drives/{id}`: once per Send/Receive, and only when some folder has a name that
   is gone), says the drive was not restored, and the drive, asked about that very object (a
   HEAD), says it is not there: a listing that is short for any reason, or a look that fails,
   takes nothing out, and a token server that cannot be asked leaves the mail where it is (the
   window says so). What leaves is NOT deleted: its file moves to `removed/<folder>/<stamp>/` of
   the local copy (`<stamp>` is the moment of the pull; `index.jsonl` there holds the folder's
   lines of it) and stays 90 days, the span the drive keeps its own history; the window says, for
   each folder, how many messages were removed from the drive and where they are
   (`removed/<folder>/`, in the account's folder of AzMail's data, next to `mail/`). Only a sweep
   of `removed/` after those 90 days deletes it, and one that cannot read its list deletes nothing
   (and says so in the window).
   What is still in the drive is not "gone": a name that is missing from one folder's listing and
   listed in another folder was MOVED there by another device (or its folder was renamed), and its
   line follows it - with its file and the marks made here, under a UID of the new folder - before
   any folder is pulled: no hold, no download, no sentence. A message another program marked
   `\Deleted` and has not expunged is still in the drive too: its line stays (flagged, so the window
   shows no line that has it) until the mark goes or the message does.
   To put a held message back by hand, open its file under `removed/` in a mail program and move
   it into the folder; a window action that puts a whole pull back (it re-imports the lines of a
   `removed/<folder>/<stamp>/index.jsonl`, whose `remote` names the objects they had) is not built
   yet.
   The stamp is the clock of this computer, set against what else is known of the time (the one
   rule, `store::Clocks::hold_time`, that Delete in the Outbox follows too): a pull on a clock that
   is behind the last Send/Receive by more than a day, or ahead of the last Send/Receive and of the
   newest mail the drive lists by more than a year, holds nothing - the lines stay in their folder,
   the Send/Receive goes on (new mail comes, the later folders are synced, the sweep runs) and the
   window says why; a clock that was refused is not recorded as the time of the sync; one that
   is a few hours behind dates the hold at the last Send/Receive. The sweep deletes a hold only
   when BOTH the clock of this computer and the drive agree that it is 90 days old (the drive's
   reading is the newest `modified` it lists: it lived at least until then, so it never says a
   later time than it is): a clock that jumped ahead deletes nothing that a drive which has not
   lived that long does not agree to, and an unset clock or a drive that lists no mail deletes
   nothing. (The drive's `modified` of an encrypted drive is its writers' clock, this computer's
   included: an independent reading - the token server's `Date` - is the named rest.)
   Delete in the Outbox is the other writer of `removed/`: it holds the mail's text and its entry under
   `removed/outbox/<stamp>/` of the same folder, dated by the SAME rule (the clock of this computer set
   against the newest Send/Receive of the account), and removes nothing when that rule cannot date the
   hold - on a clock that is not set, a day behind or a year ahead, or when the account's record of the
   last Send/Receive cannot be read; the sweep above deletes what it holds after the 90 days too.

After a pull the local copy equals the bucket as listed (what the bucket no longer lists is held
under `removed/`, not in the folder); with no new writes, every device's next pull ends in the
same state - except after a RESTORE of the drive. An operator's restore (the drive rebuilt from
its lagging off-site copy) is announced by the token server with an epoch (`restore.rs`, the one
protocol of every drive). The session of the last sign-in carries the newest announcement it
saw, hours old, so Send/Receive asks the token server again before it lets go of anything the
drive no longer lists: a restore announced after that sign-in is in no snapshot. The
Send/Receive that meets an epoch newer than the one this computer took (`azlin-drive.json`,
written after the last folder was synced) deletes no mail here: what the restored drive lacks
came after the restore's moment and exists on this computer only, so it is put back into the
drive, and the window says so. "After the restore's moment" is decided by each index line's
`on_drive_since` (when this computer first saw its object in the drive) against the notice's
`as_of`: mail first seen before the moment and absent from the restored drive was deleted before
it, and is an ordinary delete (held); a line without the stamp (from before the field), or a
notice without a moment, goes back as it always did - the first Send/Receive after an upgrade is
that one conservative pass, and it stamps every line it sees in the drive. A message of which only
the header block is here cannot go back: the window says that apart. The restore a pass takes is
written into `azlin-drive.json` (`taking`) before the first message goes back, and the epoch
(`restore_epoch`) only after the last folder.
The user's own "Restore the drive as of ..." is no such announcement: on an encrypted drive, the
only kind AzMail opens, it is one ordinary commit that removes every file made since, and AzMail
follows it as the delete it looks like - the copies on this computer, possibly the only ones,
are held for 90 days under `removed/` (the drive's own history keeps them for the same span).

### 4.4 Cost, and what comes later

The state listing grows with the read messages: one listing page per 1000 markers. For very
large mailboxes a compaction comes later - a snapshot object of all markers in `mail/.index/`,
with only the markers newer than it listed on top - and a folder summary cache there so a new
device does not fetch every message's header block. Both are caches derived from the listing:
a lost or stale one costs a re-read, never data.

## 5. The local copy

An Azlin account keeps the same files as an IMAP account (`<AzMail folder>/<account>/mail/<key>/
<yyyy>/<mm>/<uid>.eml`, `index.jsonl`, `state.json`, `flags.json`), so the window, the search and
the reading pane are the IMAP account's. Each line of `index.jsonl` carries `remote`: the
message's key in the bucket. UIDs are given locally in arrival order. The window reads only the
local files; the bucket is read and written on azul Threads.

## 6. A 50 MB attachment

- **Listing** shows it with its size (the ListObjectsV2 entry); nothing is downloaded.
- **Send/Receive** fetches a message up to 4 MiB whole. A bigger one gets only its first 64 KiB
  (one ranged GET: the header block, for From, Subject and Date); the rest stays in the bucket.
- **Opening it** downloads it once, in ranged GETs of 8 MiB on a Thread (no single request comes
  near the HTTP client's timeout; the reading pane says it is downloading), into the local copy;
  later opens read the file.
- **Moving it** is a CopyObject in the bucket (S3 copies up to 5 GB in one call): the 50 MB never
  pass through the device.
- **Saving a draft with it** is one PutObject of the whole message (base64 makes 50 MB about
  68 MB). S3 takes up to 5 GB in one PUT, but one 68 MB request over a slow uplink can run into
  the client's timeout; a multipart upload (8 MiB parts) is the next step.

## 7. Signing in

- `account.json` (`"kind": "azlin"`, version 2): the name, the address, the token server's URL
  and the drive id. No secret.
- The keyring entry `AzMail/<account>/azlin`: the drive token and the current S3 credentials,
  one JSON object.
- Send/Receive: credentials valid for more than an hour are used as they are; otherwise
  `POST <token server>/v1/drives/<id>/credentials` with the drive token (`Authorization: Bearer`)
  answers new credentials AND a new drive token. The old token is dead from then on (reusing it
  makes the token server revoke this device), so the new one goes to the keyring at once,
  before the folders are synced.
- **One device, one token family.** AzMail must not share a drive token with AzDrive (or a
  second AzMail): each refresh would make the other's token a reused one. The wizard takes a
  drive id and a drive token minted for AzMail (the token server's members route), or creates a
  new drive (`POST /v1/drives`, development token servers only).
- **The token server's URL** comes from the shared Azlin config (`~/.azlin/config.json`, or the
  file `$AZLIN_CONFIG` names; its `endpoints` section), then `$AZLIN_TOKEN_URL`, then
  `--azlin-token-url <URL>`; the account remembers the one it was created with. There is no
  built-in default. The S3 endpoint is the one the token server reports; `$AZLIN_S3_URL` /
  `--azlin-s3-url` override it (a test's local S3, a block reached through another host name).
- An `http://` token server is accepted on this computer only (the drive token and the
  credentials would cross the network in the clear), as an unencrypted IMAP server is.

## 8. Sending

Unchanged: an Azlin account sends from this computer - straight to the recipients' mail servers
or through an SMTP relay - with the Outbox for what cannot go yet. The sent copy is filed in Sent
on this computer and the next Send/Receive puts it into `mail/Sent/`. Azlin itself never sends
or relays mail.

## 9. The seam for incoming mail

The customer's Cloudflare Email Worker (`examples/azlin-mail-worker`) files each message with the
customer's bucket credentials. An Azlin drive is always encrypted: the Worker seals the message to
the drive's drop key (AZD1) and puts it at `.azlin/drop/<random>`, and a device of the drive opens
it and files it into the encrypted `mail/` folders. A Worker without the drop key **fails
closed**: it files nothing and the delivery fails, so the sending server tries again - it never
writes the mail into an encrypted drive's bucket in the clear. Only a bucket that is not an Azlin
drive, and only when the Worker's configuration says `AZLIN_PLAIN_DRIVE = "yes"`, gets the plain
seam: name = stamp of now + `-` + the first 16 hex digits of SHA-256 of the bytes; PutObject
`mail/Inbox/<name>.eml` (or `mail/Spam/` on its own spam verdict). Nothing else: there is no
index to update (the listing is the index) and no state to write (unread is no marker).

## 10. Testing it locally

- `scripts/azlin_mock_stack.py`: a Python token server (the `/v1/drives` routes AzMail uses,
  rotation and reuse detection included) in front of AzDrive's stdlib S3 test server
  (`examples/azul-drive/scripts/s3_server.py`), on free ports or `--token-port 8081 --s3-port
  9000`.
- `scripts/azlin_token_conformance.py --mock` / `--token-url <URL>`: the same HTTP checks
  against the mock and against the real token server (`azctl dev up --processes`), so the two
  cannot drift.
- `scripts/azmail_seed_azlin.py [--token-url URL] [--s3-url URL] [--big-mb N] [--out FILE]`:
  signs up a test drive and puts realistic, made-up mail into it (plain text, HTML with an
  inline picture, an attachment, a calendar invitation into `mail/Inbox/`, one into
  `mail/Spam/`, with `--big-mb` one with a big attachment).
- `scripts/azmail_e2e.py --phase azlin [--azlin-stack mock|local]`: AzMail headless against the
  mock (default) or the running stack - add the Azlin account, see Inbox and Junk E-mail, open
  a message (its read marker appears), open the big one (downloaded then), archive one (and find
  it moved in the bucket), save a draft (and find it in `mail/Drafts/`), no drive token in any
  file or output, the seeded token dead after the sign-in.
- `scripts/azlin_client.py`: the scripts' one client of the token server and of a drive's
  bucket.
- The Rust side's tests: `azlin.rs` (names, markers, the session, the token server client over
  a fake transport, the endpoints; now in examples/azul-mail-core) and `azlin_sync.rs`
  (Send/Receive and every action, with a folder on disk as the drive).

## 11. Other mail programs: the Azlin Bridge

Apple Mail, Outlook and Thunderbird reach the same mailbox through the Azlin Bridge
(examples/azul-bridge): IMAP and SMTP submission on 127.0.0.1 of the user's own computer. It
reads and writes exactly this layout (through azul-mail-core's `azlin` module):

- Mailboxes are the folders under `mail/` with the roles of section 1 (`INBOX`, and `\Sent`,
  `\Drafts`, `\Archive`, `\Junk`, `\Trash` on the well-known ones), names in modified UTF-7.
- `\Seen`, `\Flagged` and `\Answered` are the markers of section 3, so a mail read in Apple Mail
  is read in AzMail. A keyword (`$Forwarded`, `Work`, ...) is a label marker
  (`label/<keyword>`), so AzMail's categories and the programs' keywords are one thing.
  `\Draft` is no marker: every message of the Drafts folder has it, no other one does.
- `\Deleted` is the marker `mail/.state/<id>/deleted` (added for the bridge; older AzMails
  ignore it, AzMail now hides a message while it is there). EXPUNGE deletes the object, then
  every marker of the message when no other folder holds the same name - else only its
  `deleted` marker, so the copy elsewhere is not hidden.
- APPEND, COPY and MOVE write objects named by section 2 (APPEND: the time it gives, else now);
  a copy shares its markers with the original (the same `<id>`).
- IMAP's UIDs are the bridge's own: a map per mailbox in its state folder (names numbered in
  name order when it first sees the mailbox, later names after them; UIDVALIDITY the map's
  creation time). Nothing of it is written into the bucket.
- A mail submitted over SMTP goes out through AzMail's sending path (section 8) and its copy
  is put into `mail/Sent/` at once, read; the mail program's own APPEND of that copy to Sent
  is recognised by its Message-ID and not filed twice.
