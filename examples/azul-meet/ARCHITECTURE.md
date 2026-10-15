# AzMeet: how the pieces fit

AzMeet is built so that the only server it needs knows as little as possible. The people in a
meeting talk to each other directly over iroh (QUIC between their devices). A small meetings
service lets them find each other and remembers which rooms exist and who belongs to them. An
iroh relay carries a connection only when two devices cannot reach each other directly. Whoever
records a meeting keeps the recording and its transcript in their own storage.

```text
          meetings service                          iroh relay
   rooms, members, keys, call tickets       (only when no direct path forms)
            ^          ^                          ^            ^
   signed   |          |   signed                 |  encrypted |
   requests |          |   requests               |  packets   |
            |          |                          |            |
        device A <==================================> device B
                 media, presence (chat next: see "Next steps")
                 directly, or passed on by other participants
```

The cryptography (device keys, invite secrets, room keys, what each request carries) is written
down in `CRYPTO.md`. This file is about who carries what.

## The meetings service

The meetings service is a small HTTP API (the `meet` service; its code lives outside this
repository, and AzMeet's tests run against an in-memory copy that keeps its rules, `chatroom.rs`).
Settings > Meetings names its address ("Meetings service"); the shared Azlin config names it as
`endpoints.meet`.

It is meant to be dumb: it stores records it cannot read and checks signatures it can check, and
nothing else. It never holds a key that opens a message, a name or a room key, so a copy of its
database tells an attacker that rooms exist, when, and which device keys are in them, but not
what anyone said or who they are by name.

What it must keep, because no device can stand in for it:

- **Room existence**: a room's id, its short code, its public invite key (which proves that a
  device holds the link without showing the link), its kind (a meeting or a chat room) and its
  schedule (start and end).
- **Membership**: each member's signed public record (its X25519 key, its name sealed with the
  link's key), knocks and admissions, joins and leaves. A device that comes online later must be
  able to read who is in a room even when nobody else is online.
- **Rendezvous**: while a call runs, each participant's signed iroh ticket (how to reach its
  device). This is how participants find each other; the tickets expire on their own.
- **Room keys**: the sealed copies of each room key, one per member, as small ciphertext records.
  They stay on the service so that a member who was offline while the members changed still gets
  the new key when it comes back. The service cannot open them.

What it keeps today and will stop keeping: the chat, as ciphertext, numbered in the order it
arrived. The chat moves onto the peers (see "Next steps").

Why so little: every byte the service keeps is a byte that can leak, be subpoenaed or go down
with the service. Media never touches it, and once the chat leaves it too the service is a single
small database behind a handful of endpoints.

## What goes peer to peer

Everything a call carries goes between the devices over iroh, which encrypts every connection:

- **Media**: each participant's audio (Opus or 16-bit PCM, 20 ms packets), camera and screen
  (H.264 where an encoder works, JPEG otherwise), in the renditions the viewers ask for.
- **Presence inside a call**: mute and deafen, what each device can decode, its uplink and the
  tiles it shows (the capacity report every participant sends every two seconds and on every
  change). Who is in the call at all comes from the rendezvous tickets on the service.
- **Chat**: in a call, each message also goes to the peers directly (kind `8`, passed on by
  every peer that has not seen it), so it arrives without the service; the service still keeps
  and numbers the ciphertext until every client speaks the next wire version (see "Next steps").

A device only dials, and only keeps a connection to, an endpoint whose ticket a member's device
signed, so the service cannot slip a stranger into a call.

## The relay, and how peers route for each other

iroh first tries a direct path between two devices (including hole punching through NAT). When
none forms, the connection goes through a relay, which forwards QUIC packets it cannot read.
Settings > Meetings offers three relays ("iroh relay"):

- **Default**: n0's public relays, the ones iroh ships with.
- **Azlin**: Azlin's own relay at `https://connect.azlin.io`.
- **Custom**: a company's own relay, by its address, for networks that must not leave the company.

Every profile of the shared Azlin config uses n0's public relays until Azlin's relay is hosted;
Azlin's relay is then the trial and production profiles' default. Until then it is a choice in
the settings, by name or by address. A choice made in the settings is saved in `meet/settings.json` and counts
over the shared config (`endpoints.relay`: `default`, `azlin`, or an address); `--relay`
overrides both for one run. "No relay" is not offered: `--relay off` exists for tests, and a
meetings service on the same computer means a local test and no relay as well. Changing the relay
binds the device's iroh endpoint anew, so it can only be changed outside a meeting.

Small rooms are a full mesh: up to the mesh cap (4 today, 8 in the design) everyone sends to
everyone. Above it the best-connected participants (by the capacity reports) form a backbone.
Every other participant sends its media once, to its backbone parent, and receives everything
from it; backbone peers pass what they get on to their own leaves. Every device computes the
same plan from the same reports, so nobody coordinates it. A stream takes at most three hops,
and a forwarder passes on only the renditions someone below it shows. A participant on battery,
or one that opted out of forwarding, is ranked last for the backbone. The rules and the
wire format are in `routes.rs`.

## Recording and transcripts

Recording is not built yet; this is how it is meant to work (`ROADMAP.md` has the steps).

- Whoever records does all of it on their own device: they tell every peer that they record
  (each window then shows it), write the audio (each participant's stream is separate already,
  so the transcript needs no speaker detection) and the active speaker's video.
- The transcript is made on the recorder's computer with a local speech model, never a cloud
  service by default.
- The recording, the transcript and a summary go into the meeting's folder in the recorder's own
  Azlin storage (`meet/<room id>/`), sealed with the device's key before they leave it. The
  recorder uploads them; the meetings service stores none of it. Other participants get at most
  an access link to read that folder.

## Chat history when nobody else is online

There is no always-on peer. A message written while no other member is online waits in the
sender's outbox and goes out on the next connection. For a recorded meeting the chat is part of
what the recorder keeps in their storage, next to the recording and the transcript.

## Direct calls

A direct call reaches one other computer by its iroh address, with no meetings service at any
point. It is not practical for meetings - there is no room, no link and no history - but it shows
that two AzMeets can find each other with nothing in between, so it stays small and tucked away,
like AzMail's direct sending: a "Direct call..." button in Settings > Meetings with a plain note on
what it means (`direct.rs` is the protocol, `direct_call.rs` wires it to the endpoint and the
window).

- **Your address.** The screen shows this computer's iroh address as text to copy and as a QR
  code, in one of two forms, said plainly next to the switch: "With direct addresses" (the
  default: the full ticket, faster, but it shows your IP addresses to whoever has it) or "Id
  only" (the bare endpoint id, found through the configured relay; it shows no IP address). A
  ticket too long for a QR code is there as text only.
- **Calling.** A field for the other person's address and a Call button (Cancel while it rings).
  AzMeet dials it over iroh with the configured relay (any of the three; with `--relay off`,
  direct addresses only) and, once connected, says hello: its name and its own ticket. A call
  nobody takes within 45 seconds is given up.
- **Being called.** An incoming direct call is never accepted by itself: a connection from an id
  nobody expects waits for its hello (ten seconds at most), then AzMeet shows a prompt over
  every screen with the caller's name, Accept and Decline, and four safety words, picked from a
  list of 256 words by the first four bytes of `SHA-256("azmeet/v1/direct-call" || the smaller
  id || the larger id)`. Both people read them to each other: the same words on both screens
  mean they reached each other. While AzMeet is in a meeting of the meetings service, a direct
  call is answered "busy". Calls arrive only while AzMeet runs.
- **Keys.** The iroh connection is already end-to-end encrypted and authenticated by both
  endpoint ids, so the media needs no room key and nothing from a server. Chat in a direct call
  waits for chat over the peers and then uses the same records.
- **More than two.** Whoever takes a third person into a call hands them the roster (everyone
  else's id, name and ticket) and tells the others to expect them; the newcomer dials each, and
  a connection from an expected id joins without a prompt.
- **Recent calls.** The screen lists the last ten direct calls (who, when, which way) with Call
  again; they are kept in this device's settings (`meet/settings.json`, `direct_calls`), never
  sent anywhere.

On the wire every direct-call message is a reliable iroh message of kind `9` (next to the media
kinds; `8` is kept for chat records): the kind byte, a version byte, then JSON - `hello {name,
ticket}`, `accept {name, ticket}`, `decline {busy}`, `roster {peers}`, `expect {peer}`. A taken
call puts the room session into the call stage with no room behind it: the call screen, the
routing and the media run per connection as they do in a meeting, and nothing is announced,
polled or left on a meetings service.

## Next steps

Decided, built one at a time after an integration build, because each step needs compiled tests:

1. **Chat over the peers.** The chat travels like the media, so the meetings service no longer
   stores messages (room keys stay on it, see above). The record and the checks do not change:
   only who carries the record does. In order, with what is built marked:
   1. *(built)* A reliable iroh message of kind `8`, next to the media kinds: the kind byte, a
      version byte, then the JSON of exactly the signed, sealed record the service gets
      (`{room, id, key_id, sender, body, sig}`, `chatroom::ChatRecord`). A receiver checks it as
      it checks what the service returns: the signature over the room, the id, the key and the
      body; the sender a member it knows who held that key; the seal. A record that fails is
      counted unreadable and dropped; one whose key has not arrived yet waits, as a synced
      message does.
   2. *(built)* `ChatRoom::peer_records` hands out this device's messages sealed for the peers
      (each once, as soon as a key for the current members is held), and `ChatRoom::take_record`
      takes one from a peer through the same checks as a synced message. Taking it says whether
      it was new here.
   3. *(built, simpler than the routing trees)* A device sends its records to every peer it is
      connected to; a peer that takes a record new to it passes it on to every other peer it is
      connected to, and drops a repeat (the room's set of message ids it has seen). Each record
      crosses each connection at most twice, whatever the routing plan, so a member who reaches
      the others only through a backbone peer still gets it. Records are small; the routing trees
      stay for the media.
   4. While the service still numbers messages, both carry each message: a record from a peer is
      listed at once, unnumbered, after the numbered ones, counted unread until the chat is read;
      the service's copy (same id) numbers it later and is not listed twice. Ordering by `(time,
      id)` replaces the service's numbers when the service stops carrying messages (step 7).
   5. History: on connecting, two peers exchange the ids of the newest records they hold for
      each shared room, and each sends the other what it lacks (capped), so a device catches up
      from whoever is online.
   6. Chat rooms between calls: the room view announces a chat-only ticket on the rendezvous (as
      a call does) and connects to the members online; with nobody online, a message waits in the
      sender's outbox (see above).
   7. The wire version moves (`azmeet/5`), messages are ordered by `(time, id)`, and once every
      client speaks it the service drops its message endpoints and table.
2. **Recording, transcript, summary** (see "Recording and transcripts").

## Open

- **Removing a member, a consistent roster**: see `CRYPTO.md`, "Limits and what is not done".
- **A meetings service for production**: the shared config's production profile names none yet.
