# Plugging the phone companion into AzReview

For the AzReview agent. Everything AzReview needs is in the crate `azul-review-link`; this document says where each piece goes and in
what order. The runnable version of it is `examples/host_wiring.rs` (`cargo run -p azul-review-link --example host_wiring`): it
pairs a phone, drags a page, presses a button and lets an agent finish, with a toy `Review` standing where AzReview stands. When
this text and that file disagree, the file is right (it fails when the API changes).

Nothing in the crate touches AzReview's files, and AzReview needs no other crate for the companion.

## 1. What AzReview gets and what it owes

The phone is a touch pad for the review: it pans, zooms and fits the page the pen is on, steps pages, lists the workspace, opens a
file, and presses four quick buttons that stand for the user's recurring instructions. The Mac side of that conversation is one
object, `CompanionHost`, a state machine with no thread and no UI of its own. AzReview

- owns it (one per running AzReview) and polls it with a clock (section 3);
- tells it what the Mac knows: the pen and the surface under it, the picture of the page, the rows of a folder, the buttons, the
  settings, an agent that finished (section 5);
- acts on what it reports: a viewport command, a file to open, a button that was pressed, a phone that wants to be paired (section 4);
- keeps what must survive a restart: the Mac's own key and the trust records of the paired phones (section 8).

The host takes care of the rest: who may talk (a key that is not paired is refused before it can say anything), the pairing, every
cap, ordering and loss of gestures, pings and timeouts, how often the pen state may leave.

## 2. Dependency and construction

```toml
azul-review-link = { path = "../azul-review-link", features = ["azul"] }   # the feature brings the iroh link
```

The link mode of `azul-dll` (link-dynamic or link-static) is AzReview's own choice, as for every other app.

```rust
use azul_review_link::iroh_link::{IrohLink, IrohOptions};

// the Mac's key must be the same at every start: a phone knows its Mac by it. First start: None, then keep what link.secret_key() says.
// `advertised` is what the Mac says about itself on the local network (below): the phone finds the Mac by it.
let advertised = Some(advert::encode(&Advert { project: advert::project_hash(&repo_identity), name: mac_name.clone(), user: String::new(), role: Some(Role::Host) }));
let link = IrohLink::bind(&IrohOptions { secret_key: settings.companion_key, advertised, ..IrohOptions::default() })?;
if settings.companion_key.is_none() {
    settings.companion_key = link.secret_key();   // [u8; 32]: save it with the other settings, readable by the user only
}
let mut host = CompanionHost::new(link, HostConfig { mac_name, app_version, max_phones: 4 }, Box::new(OsEntropy));
host.load_paired(settings.paired_phones.clone());     // the trust records (section 8)
host.set_settings(now, settings.companion_settings);  // HostSettings, section 7
host.set_buttons(now, &settings.quick_buttons);       // ButtonSet: four labels and four instructions
host.set_screens(vec![ScreenInfo { id: ScreenId::MAIN, owner: host.node_id().to_string(), controller: host.node_id().to_string(), role: Role::Review }]);
```

`IrohLink::bind` fails when the library has no iroh engine or no socket can be opened: AzReview then shows the companion as off with
the text of the error (it never blocks AzReview from starting). Relay servers are off: the companion is for a desk, not for the
internet; the engine's local-network discovery is on (the engine announces the Mac's key on the local network, and a phone that
dials the key finds the Mac at its current address).

`IrohOptions::advertised` is the short text the endpoint announces on the local network (`advert.rs` encodes the project and the display
name; at most 245 bytes; never a secret): a phone that is paired with this Mac sees it appear there and dials it by its key at once,
also after the Mac's address changed. An app that needs more of the engine's config (a port) takes it from `IrohLink::config(&options)`,
adds to it and binds it with `IrohLink::bind_config(config)`.

`MockLink` (from `mock`) replaces `IrohLink` in AzReview's own tests (section 12).

## 3. The poll loop

Call `host.poll(now_ms)` from a timer on the UI thread, at least 30 times a second (60 is better: the gesture updates are applied at
that rate). `now_ms` is any monotonic clock in milliseconds. `poll` receives, sends, times out and returns `Vec<HostEvent>`; it never
blocks and takes a bounded number of events from the network at a time. Do not call the host from two threads.

`host.close()` when AzReview quits (the phones are told).

## 4. What each event asks of AzReview

| `HostEvent` | what AzReview does |
|---|---|
| `Ready { ticket }` | the endpoint knows its address. A QR code made before this has a stale one: remake it. |
| `PairingRequested { peer, name, code }` | a phone proved the one-time secret. Show a dialog: the phone's `name` and the code `format_code(code)` ("482 913"); the person compares it with the phone's screen. Yes: `host.accept_pairing(now, &peer)`. No: `host.reject_pairing(now, &peer)`. The host gives the person two minutes. |
| `Paired { record }` | save `host.paired()` (section 8). |
| `PeerConnected { peer, name, role }` / `PeerGone { peer, reason }` | refresh the list of phones (`host.phones()`: id, name, connected, round trip). The host tells a connecting phone the settings, the buttons, the picture and the pen at once. |
| `Viewport { peer, screen, op }` | move the view (section 6). |
| `ListDir { peer, request_id, path, offset }` | list the workspace folder `path` (relative, inside the workspace: the host has checked it) from row `offset`: `host.publish_rows(now, &peer, Rows { request_id, dir, offset, entries, more })`, at most `MAX_ROWS` (500) rows, `more` when there are rows after. |
| `OpenFile { peer, screen, request_id, path, line, .. }` | open the file on that screen (at `line`, or its first unreviewed line), then `host.reply(now, &peer, request_id, ReplyStatus::Done)` (`Failed` when it cannot). |
| `ButtonPressed { peer, screen, index, label, text, request_id }` | the instruction goes to an agent with the context at the pen (the marked region, else the line, else the page). The host has answered `Received`; send `Working`, then `Suggested` or `Done` or `Failed` with `host.reply(now, &peer, request_id, status)` as the conversation goes. |
| `ButtonsChanged { buttons }` | the phone edited the buttons (already cut to their caps; only a phone of the user's own devices may). Keep them in the settings; the host sends them to every phone. |
| `ContinueReviewing { peer, screen }` | go back to the return point of the screen and clear it (section 9). |
| `Diagnostic(text)` | something the user may want to read: a cap that was hit, a refused connection, a frame that was not understood. Put it in a log or a notice; never act on it. |

`peer` is always the phone's key (a lowercase hex string); `screen` is the `ScreenId` the command is for (today `ScreenId::MAIN`).

## 5. What AzReview publishes

All of these are cheap to call; the host paces, dedups and sends them to the phones that are here.

- `host.publish_pen(PenState)`: whenever the pen, the view, the page or the zoom changes. The host sends it at most 30 times a second,
  only when it changed, newest wins on a slow link, and numbers it itself (leave `seq` at 0). Units: widths of the surface (x across a
  sheet from 0 to 1, y down; sheet `i` starts at `i * (1 / aspect + gap)`; see `frame.rs`). `surface` is `Surface::Page(PageSurface)`
  over the pages or `Surface::Whiteboard(WhiteboardSurface)` over the board; `return_point` is `Some` while there is a place to go back to.
- `host.publish_thumbnail(Thumbnail)`: the picture of the surface (a page, or the board's overview) when it changes, with an `id` that
  the pen state names in `thumb_id`. At most 128 KiB, at most 1024 px a side. The phone asks again (`NeedThumbnail`) when a picture
  did not arrive; the host answers from the last one published.
- `host.publish_rows(now, &peer, Rows)`: the answer to `ListDir` (section 4).
- `host.reply(now, &peer, request_id, ReplyStatus)`: how far a request has got.
- `host.notice(now, Notice { level, text })`: a line for every phone (at most 512 bytes).
- `host.agent_finished(now, AgentFinished { conversation_id, file, line, kind })`: an agent finished. The phone shows where and buzzes
  by the settings (`Done` a success buzz, `Suggestion` a warning, `Failed` an error, `NoAction` none; each can be switched off).

## 6. The zoom model

A gesture arrives as a clean `Begin`, `Update`s and `End`, all with `id`, and its numbers are TOTALS since the `Begin` (a dropped
update loses nothing). `pan_x` and `pan_y` are how far the VIEWPORT has moved over the surface (positive: toward the right and the
bottom), in the surface's units, whichever way the user's "reverse pan" setting points: the phone has already applied that setting,
so the Mac never needs to know it. `zoom` is the factor so far (above 1 zooms in).

`viewport.rs` is the arithmetic, the same the phone uses to show the page moving under the finger, so the two agree to the pixel. Use
it as it is, or as the reference for AzReview's own zoom model:

```rust
let space = Space::of_pages(&page);              // or Space::of_board(aspect) over the whiteboard
// on Begin: base = the current view (a Rect in surface units). On every step of the gesture:
let about = resolve_focus(focus, base, pen_position);         // Focus::Pen is the pen (or the cursor), Center the middle
let view = apply_gesture_in(base, (pan_x, pan_y), zoom, about, space);   // zoomed about the point, then moved; held in the space
// on End: the same, and when the setting "snap to page top" is on: snap_to_page_top(view, &page)
// the other commands:
ViewportOp::Fit        => fit_width_in(view, space),
ViewportOp::PageStep(n) => step_page(view, &page, i32::from(n)),     // the page keys; n is held to +-8
ViewportOp::Reset      => reset_to_page_top(view, &page),
```

A `Rect` is `{ x, y, w, h }` of the part of the surface the Mac shows; the zoom is `1 / w` (1.0: the width of a sheet fills the
window). `MIN_ZOOM` 0.25 and `MAX_ZOOM` 16 are held by `clamp_view_in`. After every change publish the pen state again.

Commands are for a screen (`ScreenId`) and arrive only for screens the peer may command (`may_command`): a command for another
screen is refused by the host with a `Diagnostic`.

## 7. Settings AzReview shows (the phone obeys them; it keeps none of them)

`HostSettings { reverse_pan, snap_to_page_top, haptic_done, haptic_suggestion, haptic_failed }`; the phone gets it with every
connection and whenever `host.set_settings(now, settings)` changes it. Suggested rows (every string English and German, as everywhere
in AzReview):

| row | en | de |
|---|---|---|
| reverse pan | Reverse the pan direction of the phone | Wischrichtung des Telefons umkehren |
| snap | Snap to the top of a page after a drag | Nach dem Ziehen an den Seitenanfang springen |
| buzz done | Buzz when an agent is done | Vibrieren, wenn ein Agent fertig ist |
| buzz suggestion | Buzz when an agent has a suggestion | Vibrieren, wenn ein Agent einen Vorschlag hat |
| buzz failed | Buzz when an agent could not answer | Vibrieren, wenn ein Agent nicht antworten konnte |
| phones | Phones | Telefone |
| pair | Pair a phone | Telefon koppeln |
| forget | Forget this phone | Dieses Telefon vergessen |

The "reverse pan" row is a question that is still open with the user: the default is that the CONTENT follows the finger (a map on a
phone); the row flips it.

## 8. Pairing, and what to keep

1. "Pair a phone": `let offer = host.begin_pairing(now)` (`None` until the endpoint knows its address). Show `offer.to_text()` as a QR
   code AND as text the user can copy (the phone takes the text pasted into its pairing screen; its QR scanner is not built yet).
   The offer holds a one-time secret and is good for five minutes (`host.pairing_open(now)`); `host.cancel_pairing()` closes the window.
2. The phone dials the ticket in the offer and proves the secret. `PairingRequested` arrives (section 4): the dialog with the code.
3. Yes: `accept_pairing` (returns `Err(ConfirmError)` when nobody waits any more, another phone waits, or four are paired: say so).
   The secret is spent, `Paired { record }` arrives, the phone is served.
4. KEEP: the Mac's key (`IrohLink::secret_key`, section 2) and the trust records (`host.paired()`: `Vec<TrustRecord>` of peer key, name,
   kind, group) in AzReview's settings; load them with `host.load_paired(..)` at the next start. Nothing else is kept on the Mac.
5. "Forget this phone": `host.forget(now, &peer)`, then save `host.paired()`. The phone is disconnected and refused from then on; its
   own screen says so and offers to pair again.

A key that is not paired and arrives while no window is open is refused with a `Diagnostic`. A wrong proof three times closes the window.

## 9. Return point and "Continue reviewing"

When the user jumps from the review to an agent's conversation, AzReview saves where it was (first jump only) and puts it in the next
pen states as `PenState.return_point = Some(ReturnPoint { file, page_index, line, label })`; the phone shows "Back to <label>" and a
"Continue reviewing" button. `HostEvent::ContinueReviewing` means the user pressed it: go back to the saved place, clear it, publish the
pen state with `return_point: None`.

## 10. Screens, roles and who may command what

`host.set_screens(vec![ScreenInfo { id, owner, controller, role }])` lists the screens of this node (one is enough today:
`ScreenId::MAIN`, owner and controller are the node's own key, `role` is `Role::Review`). `may_command(record, screen, node)` is the one
pure rule: the user's own devices (`TrustGroup::MyDevices`, what a phone is paired as) may command every screen of the node, a
collaborator only the screen it controls, nobody a screen of another node. No session is shared and no screen is claimed by this
crate; the types are there so that other peers and screens fit later.

`advert.rs` is the text a node announces on the local network (`azreview/1;p=<project hash>;n=<name>;u=<user>;r=<role>`, at most 245
bytes, total in both directions), the one the engine's `IrohConfig::with_advertised` carries (section 2). The phone reads the
announcements through `IrohLink::discovery()` (the engine's `NearbyAppeared` / `NearbyGone` events and `nearby_peers()` behind the
`Discovery` seam of `discovery.rs`): a Mac of the key it was paired with that appears is dialed by that key at once. Besides that it
dials the addresses it saved, and the Mac's key as the last of them.

## 11. What the phone cannot do

A phone app that is suspended cannot hold a connection on the local network, and a sideloaded app with a free Apple account has no
push notifications: the Mac cannot wake the phone. The phone buzzes and shows the line only while its app is open. The phone dials the
addresses it saved; the engine's local-network discovery is on, so a Mac whose address changed can be found again by its key (not yet
tried on a real network); when that fails the phone says "the address changed" and offers to pair again. The phone never talks to anything but the Mac it was paired with (or the offer it was given).

## 12. Tests on AzReview's side

No network, no libazul: `MockNet` is an in-process network with latency, loss, reordering and a slow link, and `PhoneClient` runs on it
as a fake phone.

```rust
let net = MockNet::with_knobs(Knobs { latency_ms: 2, ..Knobs::default() });
let mut host = CompanionHost::new(net.endpoint("mac"), config, Box::new(SeededEntropy(7)));   // a MockLink
let mut phone = PhoneClient::new(net.endpoint("phone"), ClientConfig::default(), Box::new(MemoryStore::new()));
// begin_pairing / accept_pairing as in section 8, then drive both with a counter: host.poll(now); phone.poll(now); now += 1
// phone.gesture(..), phone.press_button(..), phone.list_dir(..) are what a person does; host events are what AzReview sees
```

`net.kill_connections(now, "mac")` and `net.set_knobs(Knobs { down: true, .. })` play a Mac that crashes or a network that is gone;
`net.change_address("mac")` a Mac whose address changed.

## 13. Limits (every one is held, none can crash AzReview)

A frame at most 256 KiB; a path 1024 bytes and inside the workspace; a name 96 bytes; a notice 512 bytes; 500 rows to a listing frame;
a thumbnail 128 KiB and 1024 px; four phones; eight button presses a second; a pan of at most 16 page widths and a zoom between 0.05
and 20 a gesture; the pen state 30 times a second and a gesture update 60. A peer that sends what is no frame is forgiven three times
and then dropped. Everything over a cap is cut or refused and reported as a `Diagnostic`, never a panic.
