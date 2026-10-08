# AzMeet: keys, encryption and what the meeting server sees

AzMeet's rooms (meetings and chat rooms) are end-to-end encrypted. The `meet` Worker (azul-apps
`cf-workers/meet`) mints rooms, keeps the members' public records, the sealed copies of the room
keys and the chat history as ciphertext, and hands out the iroh tickets that start a call. It never
holds a key that opens any of it. The iroh relay forwards QUIC packets it cannot read. This file is
the design; `src/crypto.rs` implements the primitives, `src/chatroom.rs` the rules, and the Worker
checks the signatures it can check (`cf-workers/meet/src/auth.js`).

## 1. Threat model

| Party | Trusted with | Not trusted with |
|---|---|---|
| The `meet` Worker and its database (Turso / sqld), and whoever runs or breaks into it | availability: it may drop, delay or withhold anything | reading messages or names, adding a member, swapping a member's keys or iroh ticket, posting as a member |
| The iroh relay (`--relay`) | forwarding packets | reading or changing them (iroh's QUIC is TLS 1.3 between the two endpoints' keys) |
| The network | nothing | everything above |
| A person with the room's **link** | joining the room (as in every meeting app: the link is the invitation) | reading what was written before they joined |
| A person with only the room's **code** | asking to join ("knocking"); a member must let them in | anything else |
| A member's device | everything of the epochs it was a member in | messages written after it left |

Out of scope: a compromised member device (it reads what its member reads), traffic analysis
beyond what section 9 lists, and the user's own files (section 8).

## 2. Notation

- `H` is SHA-256; `HKDF(ikm, salt, info)` is HKDF-SHA256 (RFC 5869) giving 32 bytes.
- `sign(sk, m)` is Ed25519 (RFC 8032) over the UTF-8 bytes of the canonical string `m`;
  `verify` uses `verify_strict` (no malleable signatures).
- `seal(k, n, p, a)` is XChaCha20-Poly1305 with key `k`, 24-byte nonce `n`, associated data `a`;
  every nonce is 24 random bytes from the OS (`getrandom`).
- `hex` is lower-case hex; `b64` is standard base64 with padding (RFC 4648 section 4).
- A **canonical string** is lines joined by `\n`: the first line is a label `azmeet/v1/<what>`, the
  others are hex, base64, base32 ids, decimal integers or a URL path - never a newline. Every
  signature and every associated data in this file is one; nothing is JSON-canonicalised.
- The **id alphabet** is lower-case Crockford base32, `0123456789abcdefghjkmnpqrstvwxyz`.

## 3. The device identity

Every installation of AzMeet is a **device** with one long-term secret, the 32-byte `seed`
(random, `getrandom`). It lives in the OS keyring (Keychain, Credential Manager, libsecret,
KeyStore - azul's `CallbackInfo::keyring_*`, the way AzMail keeps its passwords) under the name
`AzMeet/identity`, as `{"format":"azmeet-identity","version":1,"seed":"<b64>"}`. Everything else
is derived from it:

```text
sign_sk = Ed25519 key from HKDF(seed, "azmeet/v1/identity", "ed25519")
dh_sk   = X25519 key  from HKDF(seed, "azmeet/v1/identity", "x25519")
local   =               HKDF(seed, "azmeet/v1/identity", "local")
device  = hex(sign_pk)     (64 characters: the device's id everywhere)
dh      = hex(dh_pk)
```

- A run whose keyring answers `NotFound` makes a new seed and stores it. One whose keyring is
  unavailable (or refuses) runs with a seed that lives only until it quits, and says so.
- Headless runs (`AZ_BACKEND=headless`) get azul's in-memory keyring: a new device every start.
  `--identity-file <path>` (`AZMEET_IDENTITY_FILE`) keeps the same JSON in a file (created with
  mode 0600) instead of the keyring - for tests and unattended machines; it is never the default.
- `local` seals the secrets AzMeet keeps in its files (section 8).

### Safety codes

A device's **safety code** is 20 digits derived from its two public keys:

```text
d = H("azmeet/v1/safety\n" || sign_pk || dh_pk)            (raw key bytes after the label)
code = for i in 0..4: (big-endian 40-bit number d[5i .. 5i+5]) mod 100000, as 5 digits
       joined by spaces: "05881 39114 50072 66310"
```

The people list shows each member's code, the room view shows "Your safety code". Two people
compare the code one of them sees for the other with the code the other sees as their own (read
out on the call, or side by side), then mark the device **verified**; the mark is kept locally by
device id, so it holds in every room. A device whose keys change is a new device with a new code,
unverified. 20 digits are about 66 bits: forging a device with someone's code means about 2^66 key
generations.

## 4. Rooms, links and the invite secret

AzMeet mints a room itself: the room id (26 random characters of the id alphabet, 130 bits) and
the **invite secret** `s` (26 more, 130 bits). The room is registered with the Worker by
`POST /rooms {room, invite_key, kind, starts_at, ends_at}` (the Worker's "links made offline"
path, which AzCalendar uses too). The link is

```text
azlin://meet/<room>#<s>
```

The fragment never reaches the Worker: AzMeet never sends it, and a browser never sends a URL
fragment either. From `s`:

```text
invite_sk  = Ed25519 key from HKDF(s, "azmeet/v1/invite", "sign\n" + room)
invite_key = hex(invite_pk)                     (registered with the room; public)
link_key   =               HKDF(s, "azmeet/v1/invite", "names\n" + room)
proof(device, dh) = sign(invite_sk, "azmeet/v1/proof\n{room}\n{device}\n{dh}")
```

- A **proof** shows that a device holds the link. The Worker checks it against the registered
  `invite_key` before it lists the device as a member; every member checks it again.
- Joining with a link, AzMeet derives `invite_key` from `s` and refuses a room whose registered
  key differs ("the meeting server sent a room that does not match this link").
- `link_key` encrypts display names (section 5): every link holder reads them, the Worker does not.
- The short **code** (`xq4-8kd-2nm`) is the Worker's alias of the room id; it carries no secret.
  Joining with it is a **knock** (section 6). Joining by code trusts the Worker to send the person
  to the right room until they compare safety codes; joining by link does not.

## 5. Members

A device joins a room with `PUT /rooms/<room>/members/<device>`:

```json
{"dh": "<hex>", "sealed_name": "<b64>", "name": null, "ts": 1760000000000,
 "proof": "<hex>", "sig": "<hex>"}
```

```text
sealed_name = b64(nonce || seal(link_key, nonce, name, "azmeet/v1/name\n{room}\n{device}"))
sig         = sign(sign_sk, "azmeet/v1/member\n{room}\n{device}\n{dh}\n{sealed_name}\n{name}\n{ts}")
```

(`{sealed_name}` and `{name}` are empty where the field is null.) A member's record binds its X25519
key and its name to its device key; the Worker cannot change one without breaking the signature.
A member is **accepted** by the others when its record's `sig` verifies, its `proof` verifies against
the room's `invite_key`, and it is not a device they saw leave since (section 7).

## 6. Knocking and letting someone in

Without the invite secret a device can only knock: the same `PUT` with `name` in plain text (the
Worker keeps only a valid, short name), `sealed_name` and `proof` null. Members see "Cleo asks to
join" with Cleo's safety code and an **Admit** button:

```text
POST /rooms/<room>/members/<device>/admit   {"admission": "<hex>"}
admission = sign(admitter_sk, "azmeet/v1/admit\n{room}\n{device}\n{dh}")
```

The Worker lists the device as a member once an accepted member (one with a proof) admitted it;
the others accept the admission when the admitter is an accepted member with a proof. The admitter
makes a new room key at once (section 7), sealed to everyone including the newcomer; the newcomer's
copy carries the invite secret, so it now holds the link: it registers again with a proof and a
sealed name, and from then on it is a member like every other.

## 7. Room keys and rotation

A **room key** `k` is 32 random bytes. Its id is `key_id = hex(H("azmeet/v1/key-id\n" || k))[..32]`
(the first 16 bytes), so whoever opens a copy can check it got the key the sender committed to.
Keys are ordered by `(epoch, key_id)`; the epoch is one more than the highest the sender knows in
the room. A sender publishes a key with

```json
POST /rooms/<room>/keys
{"key_id": "<hex>", "epoch": 3, "members": ["<device>", ...], "sig": "<hex>",
 "envelopes": [{"recipient": "<device>", "sealed": "<b64>"}, ...]}
```

```text
sig = sign(sender_sk, "azmeet/v1/key\n{room}\n{key_id}\n{epoch}\n{members, sorted, joined by ','}")
For each recipient r with X25519 key dh_r (from r's accepted record):
  e_sk random; e_pk = X25519 public of e_sk;  shared = X25519(e_sk, dh_r)  (refused if all zero)
  wrap   = HKDF(shared, e_pk || dh_r, "azmeet/v1/seal\n{room}\n{key_id}\n{r}")
  sealed = b64(e_pk || nonce || seal(wrap, nonce, k || s,
                    "azmeet/v1/seal\n{room}\n{key_id}\n{epoch}\n{sender}\n{r}"))
```

`members` is exactly the set the key was sealed to: every holder knows who else holds it. A
recipient keeps a key when the record's sender is an accepted member, `sig` verifies, it is listed
in `members`, its copy opens, and `key_id` matches the opened key.

**Rotation rule** (every member applies it; no coordinator): let `current` be the accepted members,
and `newest` the held key with the highest `(epoch, key_id)`. Before sending a message, and right
after admitting a knock, a member whose `newest` is missing or whose `newest.members != current`
makes a new key sealed to `current`, and sends with it.

- **Someone leaves** (`DELETE /rooms/<room>/members/<device>`, signed by that device): the next
  message from anyone is under a new key the leaver has no copy of. Messages written after that
  are unreadable to it, even if it kept every byte the Worker ever sent.
- **Someone joins**: the next message is under a key that includes them; what was written before
  they joined stays unreadable to them. "A message is readable by the members at the time it was
  sent" - the same rule as Signal's groups.
- Two members rotating at once make two keys of one epoch; both are sealed to the same members, and
  every sender moves to the higher `key_id`. Nothing needs to agree first.
- A device that saw another leave keeps that departure (with the time of the leave it saw) and
  refuses a record of that device older than it: the Worker cannot quietly bring back a member
  that left, to the members that saw it go. It can still hide a departure from a member that never
  saw it - there is no consistent signed roster (an MLS-style group would add one; section 11).

## 8. Messages, and what stays on the device

```json
POST /rooms/<room>/messages
{"id": "<32 hex>", "key_id": "<hex>", "body": "<b64>", "sig": "<hex>"}
```

```text
plain = {"text": ..., "name": <sender's name>, "ts": <ms>} as JSON, padded with spaces to a
        multiple of 128 bytes (lengths show only in 128-byte steps)
body  = b64(nonce || seal(k, nonce, plain, "azmeet/v1/msg\n{room}\n{id}\n{key_id}\n{sender}"))
sig   = sign(sender_sk, "azmeet/v1/msg\n{room}\n{id}\n{key_id}\n{hex(H(body))}")
```

`H(body)` is over the base64 text as sent. The Worker checks `sig` against the poster (only a
listed member posts) and keeps the message; readers check it again, check that the sender is in
the key's `members`, and open it. A message with a key the reader does not hold yet waits for the
next sync; one that fails a check is dropped and counted ("1 message could not be read").

On the device:

- The keyring holds the seed (section 3); nothing else secret is stored in the clear.
- `meet/rooms.json` (the data tree, azul-appkit's data root) lists this device's rooms: id, code,
  kind, server, times, the last message read (unread counts), and the invite secret sealed with
  `local` (`seal(local, nonce, s, "azmeet/v1/local\n{room}")`). A different device (a new seed)
  cannot open them and asks for the link again.
- `meet/<room>/chat.jsonl` and `meeting.json` are the meeting's record in the user's own storage
  (the folder per meeting the storage ruling asks for): plain text, as a mail client keeps mail.
  They are the user's files, synced by the user's drive; whether the drive encrypts them is the
  drive's design, not this file's.

## 9. What the Worker's database shows

| Table | Columns | To an attacker with the database |
|---|---|---|
| `room` | id, code, `invite_key`, kind, times, expiry, `members_rev` | that a room exists, when, its public invite key |
| `member` | room, device, dh, `sealed_name`, knock `name`, proof, sig, admission, joined / left times | the device keys in each room, when they joined and left; a knocker's typed name |
| `room_key` | room, key_id, epoch, sender, members, sig | who rotated when, and which devices hold which epoch |
| `key_envelope` | room, key_id, recipient, sealed | ciphertext |
| `message` | room, seq, id, sender, key_id, body, sig, times | who posted when, sizes in 128-byte steps; the body is ciphertext |
| `peer` | room, node_id, ticket, device, sig | iroh endpoint ids and tickets (the endpoints' addresses and relay), who is in a call |
| `rate` | hashed client IP buckets | request counts, not addresses |

Not in it: any message text, any member's name, the invite secret or the link key, any room key.
The E2E script (`scripts/azmeet_e2e.py`, phase `crypto`) reads every table of the local database
and fails if a message text or a member's name shows up anywhere, also inside any base64.

## 10. Calls

iroh already encrypts every connection between two endpoints; the risk was the Worker handing
out someone else's ticket (a man in the middle). Announcements are signed now:

```text
POST /rooms/<room>/peers {node_id, ticket, name: "", device, sig}
sig = sign(sign_sk, "azmeet/v1/peer\n{room}\n{node_id}\n{ticket}")
```

A peer is dialed, and an incoming connection kept, only for a node id whose record a member's
device signed. A connection that arrives before its record waits (no media) until the next read
of the peers list confirms it, and is dropped if it is not confirmed within a few reads. Media of
rooms above the mesh cap passes through other members (`routes.rs`); they are members of the room.

## 11. Signed requests (what the Worker checks)

Every request that changes something carries

```text
x-azmeet-device: <device>
x-azmeet-ts:     <milliseconds since 1970>
x-azmeet-sig:    hex(sign(sign_sk, "azmeet/v1/request\n{METHOD}\n{path}\n{ts}\n{hex(H(body bytes))}"))
```

`{path}` starts at `/rooms` (a proxy may mount the Worker under another path). The Worker refuses a
timestamp more than five minutes off, a signature that does not verify, and a device that may not
do the request: only a device changes or deletes its own record and its own peer records, only a
listed member admits, posts keys, posts messages or announces itself in a call. A leave is refused
when it is older than the device's last join (a replayed leave cannot remove a member who came
back).

## 12. Limits and what is not done

- **Multi-device**: each device is a member of its own, with its own safety code; a person's
  laptop and phone are two members, each verified once. A new device reads from the moment it
  joins. Linking devices (cross-signing, handing over history keys) is not done.
- **No forward secrecy inside an epoch**: a room key is used until the members change; a device
  key stolen later opens every message of the epochs that device held (a per-message ratchet, as
  Megolm's, would narrow it).
- **No removal**: a member leaves on its own; nobody can remove another, and a person who keeps the
  link can join again (and is seen joining). Removing someone needs a new invite secret (a new
  link) and a signed ban list; both are open (`../../azul-apps/planning/core/meet.md`).
- **No consistent roster**: the Worker can withhold a departure from a member that never saw it
  (section 7), or withhold anything else - an availability attack, not a confidentiality one.
- **Knock names** are plain text: a knocker has no key the members could read it with.
- **The landing page** (`https://<worker>/rooms/<room>`) does not carry the fragment over into its
  "Open in AzMeet" link yet; opening it there is a knock.

## 13. Versions

Every label is `azmeet/v1/...`. A change to any construction above gets `v2` labels, and the
iroh protocol name moves with the wire (`azmeet/4` since the chat left the iroh connections for
the Worker).
