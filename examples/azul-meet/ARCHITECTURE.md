# AzMeet: how the pieces fit

AzMeet is built so that the only server it needs knows as little as possible. The people in a
meeting talk to each other directly over iroh (QUIC between their devices). A small meetings
service lets them find each other and remembers which rooms exist and who belongs to them. An
iroh relay carries a connection only when two devices cannot reach each other directly. Whoever
records a meeting keeps the recording and its transcript in their own storage.

```text
          meetings service                          iroh relay
   rooms, members, schedule, tickets        (only when no direct path forms)
            ^          ^                          ^            ^
   signed   |          |   signed                 |  encrypted |
   requests |          |   requests               |  packets   |
            |          |                          |            |
        device A <==================================> device B
                 media, presence, (chat: see "Missing")
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

What it keeps today but does not have to (see "Missing"):

- the sealed copies of the room keys, one per member;
- the chat, as ciphertext, numbered in the order it arrived.

Why so little: every byte the service keeps is a byte that can leak, be subpoenaed or go down
with the service. Media never touches it, and the plan is for the chat to leave it too, so that
the service can be a single small database behind a handful of endpoints.

## What goes peer to peer

Everything a call carries goes between the devices over iroh, which encrypts every connection:

- **Media**: each participant's audio (Opus or 16-bit PCM, 20 ms packets), camera and screen
  (H.264 where an encoder works, JPEG otherwise), in the renditions the viewers ask for.
- **Presence inside a call**: mute and deafen, what each device can decode, its uplink and the
  tiles it shows (the capacity report every participant sends every two seconds and on every
  change). Who is in the call at all comes from the rendezvous tickets on the service.
- **Chat**: not yet. A room's chat goes through the meetings service as ciphertext today; moving
  it onto the peers is the first item under "Missing".

A device only dials, and only keeps a connection to, an endpoint whose ticket a member's device
signed, so the service cannot slip a stranger into a call.

## The relay, and how peers route for each other

iroh first tries a direct path between two devices (including hole punching through NAT). When
none forms, the connection goes through a relay, which forwards QUIC packets it cannot read.
Settings > Meetings offers three relays ("iroh relay"):

- **Default**: n0's public relays, the ones iroh ships with.
- **Azlin**: Azlin's own relay at `https://connect.azlin.io`.
- **Custom**: a company's own relay, by its address, for networks that must not leave the company.

The choice is saved in `meet/settings.json`; the shared Azlin config can name one as
`endpoints.relay` (`default`, `azlin`, or an address), and `--relay` overrides both for one run.
With nothing named, a meetings service on the same computer means a local test and no relay;
any other means the default relays. Changing the relay binds the device's iroh endpoint anew,
so it can only be changed outside a meeting.

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

## Missing

- **Chat over the peers.** The chat should travel like the media, so the meetings service no
  longer stores messages. The change, in order:
  1. A reliable iroh message kind for a chat record (`8`, next to the media kinds) carrying exactly
     the signed, sealed record the service gets today (`{room, id, key_id, sender, body, sig}`),
     and one for a room-key record with its sealed copies (`9`). Receivers check them as
     `chatroom.rs` checks what the service returns: the sender's record, the signature, the key's
     member list.
  2. `ChatRoom::send` hands back the record to broadcast instead of queuing a request, and a new
     `ChatRoom::take_record` takes one from a peer through the same checks as a synced message.
  3. Records travel the call's routing trees: a receiver keeps a bounded set of the record ids it
     has seen, drops a repeat, and passes a new one on through `relay_targets` inside the relay
     envelope (`routes::KIND_RELAY`) on a chat track of its own, as an audio frame is passed on,
     but on the reliable path.
  4. Order by `(time, id)` instead of the service's sequence numbers.
  5. History: on connecting, two peers exchange the ids of the newest records they hold for each
     shared room, and each sends the other what it lacks (capped), so a device catches up from
     whoever is online.
  6. Chat rooms between calls: the room view announces a chat-only ticket on the rendezvous (as a
     call does) and connects to the members online; with nobody online, a message waits in the
     sender's outbox and goes out on the next connection.
  7. The wire version moves (`azmeet/5`), and once every client speaks it the service drops its
     message endpoints and table.
  This needs a compiled, tested build at every step (the message checks, the forwarding and the
  catch-up are each a place where a mistake loses or duplicates messages), so it is not done yet.
- **Room keys over the peers.** With the chat on the peers, the sealed room-key copies can travel
  the same way, or stay on the service as small ciphertext records so that a member who was
  offline during a rotation still gets its copy. Undecided.
- **Messages to nobody.** Peer-to-peer chat cannot deliver to members who are all offline; a
  message then waits until someone is online. Whether some always-on device (or the recorder's
  storage) should hold chat history for offline members is undecided.
- **Removing a member, a consistent roster**: see `CRYPTO.md`, "Limits and what is not done".
- **Recording, transcript, summary**: not built (see above).
- **Production addresses**: the shared config's production profile names no meetings service
  yet, and the default relay is n0's in every profile; whether Azlin's relay becomes the default
  for Azlin customers is undecided.
