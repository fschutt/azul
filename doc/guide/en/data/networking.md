---
slug: data/networking
title: Networking
language: en
canonical_slug: data/networking
audience: external
maturity: stub
guide_order: 211
topic_only: false
short_desc: HTTP from a callback
prerequisites: [data/background-tasks]
tracked_files:
  - core/src/task.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - HttpRequestConfig
  - HttpClient
  - ThreadPool
  - HttpResponse
  - Thread
  - ThreadSendMsg
  - ThreadReceiveMsg
  - WriteBackCallback
  - RefAny
  - U8Vec
  - http_get
  - download_bytes
  - is_url_reachable
  - IrohEndpoint
  - IrohConfig
  - IrohEvent
  - IrohLoadBalancer
---

# Networking

## Introduction

*Stub.* The framework ships a small blocking HTTP helper (`HttpRequestConfig`, `HttpResponse`) you can call from inside a [`Thread`](background-tasks.md). For raw sockets, async I/O, WebSockets, or anything else, do networking the same way you do any other blocking I/O: from a `Thread`.

## What's available

- `HttpRequestConfig`. A small blocking HTTP client. Configure timeouts, headers, max response size, and TLS verification, then call `http_get`, `download_bytes`, or `is_url_reachable`. The convenience constructors `http_get_default` and `download_bytes_default` skip configuration.
- `HttpResponse`. The result. Carries `status_code`, `body` (`U8Vec`), `headers`, `content_type`, `content_length`. Use `is_success`, `is_redirect`, `is_client_error`, `is_server_error`, `body_as_string` to inspect it.

- `IrohEndpoint`. Peer-to-peer QUIC connections between apps, for media frames and messages. See [Peer-to-peer connections](#peer-to-peer-connections).

The framework is intentionally runtime-agnostic. There's no built-in raw-socket type and no async runtime integration. HTTP is request / resume shaped (below), so ordinary fetches need no worker thread; anything heavier still belongs in one.

## Making a request

Every `HttpRequestConfig` call is a *request*: it returns a `RequestId`
immediately and resumes the callback you pass in with the answer. On desktop
the transfer runs synchronously inside the call and the callback runs right
after the requesting callback returns; in the browser it is a `fetch()` and
the callback runs on a later task. Either way the callback never runs
re-entrantly inside the callback that issued the request, and the same code
works on both.

```rust,ignore
extern "C" fn on_fetch_clicked(data: RefAny, _info: CallbackInfo) -> Update {
    let cfg = HttpRequestConfig::create()
        .with_timeout(10)
        .with_user_agent("my-app/1.0");
    let _request = cfg.http_get("https://example.org/api".into(), data, on_response);
    Update::DoNothing
}

extern "C" fn on_response(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match answer.result {
        ResultHttpResponseHttpError::Ok(resp) => {
            // mutate the model through `data` the usual way
            let _ = (&mut data, resp);
            Update::RefreshDom
        }
        ResultHttpResponseHttpError::Err(e) => {
            eprintln!("request failed: {e:?}");
            Update::DoNothing
        }
    }
}
```

The result structs are `HttpGetResult` (for `http_get`, `http_post` and
`http_request`), `HttpBytesResult` (`download_bytes`) and
`HttpReachableResult` (`is_url_reachable`); each has a static
`downcast(result)` accessor. There is no default-config shortcut: build a
config with `HttpRequestConfig::create()` and call the method on it.

A request may be issued from a worker thread as well; its resume still runs
on the main thread. CORS applies in the browser and cannot be escaped: a
target that does not send `Access-Control-Allow-Origin` fails with
`HttpError::Other` naming CORS.

## Reusing connections

A request opens a connection, uses it once and closes it. Many requests to the
same server each pay for a new connection and TLS handshake. To keep
connections open between requests, create an `HttpClient` once (in your app
state, not per request) and attach a clone to each config:

```rust,ignore
// once, e.g. when building the app state
let client = HttpClient::create(
    HttpClientConfig::create().with_max_idle_connections_per_host(8),
);

// per request
let cfg = HttpRequestConfig::create().with_client(client.clone());
```

All clones share one pool, which closes its connections when the last clone is
dropped. With a client, TLS verification follows the client's
`HttpClientConfig`; the request's own timeout, headers and size limit still
apply. In the browser the client changes nothing: the browser pools
connections itself.

A pooled connection skips the TCP and TLS handshake, but not the DNS lookup:
the host is resolved before the pool is asked for a connection. When many
requests go to the same few hosts, let the client remember the answer:

```rust,ignore
let client = HttpClient::create(HttpClientConfig::create().with_dns_cache_secs(300));
```

The default is `0`, one lookup per request. A host that changes its address
is reached again once its cached answer expires.

A build made with the `http-cookies` Cargo feature (in no shipped set) gives
every `HttpClient` a cookie jar: a cookie that a server sets comes back with the
later requests that go through the same client, and its clones, to that
server. A request without a client opens its own connection and keeps no
cookie, and a build without the feature has no jar: an app that needs a session
sets its `Cookie` header itself.

The same idea applies to threads. `ThreadPool::create(n)` starts `n` workers,
and `pool.create_thread(...)` returns an ordinary `Thread` whose body runs on
one of them, so many short jobs don't each start an OS thread.

## Hardening HTTPS: which roots to trust

By default every TLS connection trusts the public web PKI, the roughly 150 root
authorities Mozilla ships. Any one of them can issue a certificate for any
name, so a single authority that issues one wrongly is enough to impersonate
your server. `TrustRoots` narrows that, and one setting covers every TLS
connection azul makes: HTTP requests and pooled clients, a database's backup
sync, the iroh relays.

Set it once for the whole app on `AppConfig`. Every config that names no set
of its own (the default, `TrustRoots::app_default()`) follows it:

```rust,ignore
// Only your own authority: a certificate from any other root fails the handshake.
let config = AppConfig::create()
    .with_trust_roots(TrustRoots::pinned(StringVec::from_vec(vec![MY_CA_PEM.into()])));

// The public roots minus the ones that lost the browsers' trust (the eMudhra /
// emSign case): until the compiled-in roots catch up, drop them yourself.
let config = AppConfig::create()
    .with_trust_roots(TrustRoots::public_roots_except(StringVec::from_vec(vec![
        EMSIGN_ROOT_G1_PEM.into(),
    ])));
```

A single request or client can still pin its own set
(`HttpRequestConfig::with_trust_roots`, `HttpClientConfig::with_trust_roots`,
`DbConfig::with_trust_roots`, `IrohConfig::with_trust_roots`); its distrusted
roots add to the app's. Entries are PEM text. A distrusted root is matched by
its public key, so a re-issued copy is dropped too. An entry that does not
parse fails every connection with a TLS error naming it; azul never falls back
to the public roots. For a TLS client of your own (an IMAP library, another
language's TLS stack), `TrustRoots::app_default().get_certificates_pem()`
returns the exact set azul trusts, one PEM certificate each.

A pinned set needs care when your server's authority changes: ship the new
root before the server switches to it. The system web view trusts the
operating system's store, and in the browser `fetch` uses the browser's roots:
neither a pinned set nor a distrusted root applies there.

## Modelling connection state

Use a plain enum on the application data side. A typical shape:

```rust,ignore
enum ConnectionStatus {
    Idle,
    Connecting { thread_id: ThreadId, started: Instant },
    Done       { response:  HttpResponse },
    Failed     { reason:    String },
}
```

Cancel by calling `event.remove_thread(thread_id)` from a click handler. The thread destructor sends `TerminateThread` and joins. If your worker checks `recv.recv()` between operations, cancellation is prompt.

## Using an async runtime

The framework doesn't host a runtime, but nothing prevents you from running one inside a `Thread`:

```rust,ignore
extern "C" fn tokio_worker(
    _initial:   RefAny,
    mut sender: ThreadSender,
    mut _recv:  ThreadReceiver,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        // futures-based code here; pump results through `sender`
    });
}
```

A current-thread runtime keeps everything on the worker. Use a multi-threaded runtime if you need a worker pool, but spawn it once and reuse. Runtimes are expensive to construct.

## Peer-to-peer connections

`IrohEndpoint` (module `iroh`) connects two apps directly over QUIC, with hole punching and an optional relay, using [iroh](https://iroh.computer). An endpoint is identified by its public key. A *ticket* is the dialing string that carries the key and the endpoint's current addresses; hand it to the other side as a link or QR code.

```rust,ignore
let endpoint = IrohEndpoint::bind(
    IrohConfig::create("my-app/1").with_relay_mode(IrohRelayMode::Disabled),
);

extern "C" fn pump(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let endpoint = endpoint_of(&mut data);
    while let Some(event) = endpoint.recv().into_option() {
        match event.kind {
            IrohEventKind::Ready => show_invite(&mut data, event.text),
            IrohEventKind::PeerConnected => remember_peer(&mut data, event.peer),
            IrohEventKind::Frame => draw_frame(&mut data, event.track, event.data),
            IrohEventKind::Message => handle_message(&mut data, event.data),
            IrohEventKind::PeerDisconnected | IrohEventKind::Error => report(&mut data, event.text),
        }
    }
    TimerCallbackReturn::continue_unchanged()
}
```

- `connect(ticket)` dials. The outcome arrives as a `PeerConnected` or `Error` event. `connect` also takes a bare endpoint id when a peer on the same network announces itself (see [Finding a peer on the local network](#finding-a-peer-on-the-local-network)).
- `send_frame(peer, track, data)` and `broadcast_frame(track, data)` send each frame on its own QUIC stream. A frame that has not left yet is replaced by the next frame of the same track, so a slow link lowers the frame rate instead of adding latency. The receiver likewise keeps only the newest frame of each track until `recv` takes it, so a stream where every piece counts (audio) repeats recent data in each frame. JPEG frames from `RawImage::encode_jpeg` are the simplest video format.
- `send_message(peer, data)` is reliable and ordered, for chat and control data.
- When the link cannot carry everything, higher priority goes first. Frames start at 0 and messages at 1; `set_track_priority(track, priority)` and `set_message_priority(priority)` change that for every peer. A call app that sends video as messages sets its audio track to 2, so video never starves audio.
- `peer_stats(peer)` reports whether the path is direct or relayed, the RTT, the congestion window, frame counters, and the outgoing message backlog (`messages_queued`, `message_bytes_queued`: accepted by `send_message`, not yet handed to the connection).
- `IrohConfig::with_relay_only(true)` binds no UDP socket, so there is never a direct path and nothing is hole-punched: every packet goes through the relay (`relay_mode` `Default` or `Custom`; with relays disabled `bind` fails). Use it on networks that block UDP, or to test a relay. A local relay for tests is iroh's own server in dev mode, `iroh-relay --dev` (plain HTTP on port 3340), reached with `with_relay_url("http://127.0.0.1:3340")`.
- Nothing arrives unless you poll `recv`, so drive it from a timer.
- For rooms, `IrohLoadBalancer` picks the peers that forward media for everyone (`backbone_size`, `select_backbone`), and `IrohTileRole::rendition_height` picks the resolution a video tile should request.
- A build made with the `iroh-portmapper` Cargo feature (in no shipped set) also asks the router for a port by UPnP, NAT-PMP and PCP, which iroh's portmapper does as soon as the feature is on. It changes the user's router, it starts with a multicast of its own (SSDP), and relays and hole punching already cover the internet, so the shipped libraries leave it out; build with it for a home network whose router blocks hole punching.

### Finding a peer on the local network

`IrohConfig::create` turns local-network discovery on. An endpoint then dials a peer on the same network by its endpoint id alone, `connect(id)` instead of a ticket, also after the peer's address changed, with no saved address and no relay, and it announces itself there. The announcement is multicast DNS on the local link (service `irohv1`, UDP port 5353) and is never routed. It carries the endpoint id, a public key, and the network addresses that another computer on the link can use: private and link-local IPv4 addresses and IPv6 addresses; not the relay and not a public IPv4 address. What a dial believes of an announcement is cut down first: no loopback, multicast, unspecified or port-0 address, at most 16 addresses per peer, no relay and no user data. A neighbour that announces false addresses cannot take a peer over, because the connection is authenticated by the key of the id that was dialled.

Turn it off with `IrohConfig::with_local_discovery(false)` for a server, or for an app that must not announce itself to the network it sits on. An endpoint that accepts nobody (`with_allowed_peers` with an empty list) only looks and does not announce, and `with_relay_only(true)` never uses the local network. `local_discovery_status()` says what it does: text that starts with `off`, `on`, or `unavailable: <reason>`. A network or an OS that refuses multicast leaves the endpoint working without it, through ticket addresses and relays; no `Error` event is raised for it.

An app that accepts only its paired devices (`with_allowed_peers`) opens a pairing window with `clear_allowed_peers()`, which accepts every peer again from the next connection, and closes it with `set_allowed_peers`.

**What an endpoint says about itself, and who is nearby.** `IrohConfig::with_advertised(text)` adds a short text to the announcement: a project id and a display name, whatever the instances of an app agree on (`azreview/1;p=ab12;n=Studio Mac`). It is at most 245 bytes of UTF-8 (a DNS TXT string after its `user-data=` key); control characters and bidirectional overrides are removed, a longer text is cut at a character boundary, and `local_discovery_status()` says `the advertised text was cut from 300 to 245 bytes`. Nothing is announced unless the app set a text, and the text goes on the local link only, as the addresses do: not to a relay, not to a ticket. `set_advertised(text)` changes it while the endpoint is bound; an empty text announces nothing again.

The other half is `nearby_peers()`: the endpoints that announced themselves on the network and still do, each with its endpoint id, the text it advertises and the addresses a dial can use (the same cut as above: at most 64 endpoints, 16 addresses each; `local_discovery_status()` counts the announcements that did not fit). The list is the state behind two events of `recv()`: `NearbyAppeared` (`text` is the endpoint id, `data` the advertised text as UTF-8) when an endpoint is new or what it announced changed, once however many times it changed since the app last asked, and `NearbyGone` (`text` is the endpoint id) for one that was reported and stopped announcing. Listing never dials: an app that wants to join a neighbour passes its endpoint id to `connect`. The list holds every iroh endpoint on the link that announces under `irohv1`, azul apps or not, so an app recognises its own instances by a prefix of the text it advertised. A neighbour's text is a string anybody can write: show it as text, and treat it as a name, never as an instruction.

| Platform | What the app or the user has to allow |
|---|---|
| Linux | A firewall must let UDP 5353 (multicast) and the endpoint's UDP port in. |
| macOS | A bundled app gets the Local Network prompt (macOS 15) the first time it announces or looks; its text is `NSLocalNetworkUsageDescription` in the bundle: `usage = ["NSLocalNetworkUsageDescription=..."]` in the crate's `[package.metadata.bundle]`. A command-line tool run from Terminal is allowed. A sandboxed app needs `com.apple.security.network.client` and `.server`. |
| Windows | The first start on a private network shows Windows Defender Firewall's prompt, once per program: allow private networks. |
| Android | The manifest template declares `CHANGE_WIFI_MULTICAST_STATE` (a normal permission); the engine holds a `WifiManager.MulticastLock` for as long as an endpoint with local discovery is bound, and releases it when the endpoint is dropped. |
| iOS | Another path, see below. |

**iOS asks the system's Bonjour.** iOS lets an app send or receive multicast only with the restricted `com.apple.developer.networking.multicast` entitlement (Apple's TN3179), which Apple grants a team on request and a personal (free) team cannot have. So the engine does not open multicast sockets on iOS: it browses `_irohv1._udp` through Bonjour, reads the SRV records of the instance named by the endpoint id and asks the system for the addresses of their hosts. That needs the Local Network permission and only the service type the app lists, no entitlement. The Info.plist needs `NSLocalNetworkUsageDescription` (the text of the prompt) and `NSBonjourServices` with `_irohv1._udp`: `AZ_IOS_LOCAL_NETWORK="why the app uses the local network"` in the environment of `scripts/build-ios.sh` writes both ([Mobile](../deploying/mobile.md#local-network-on-a-phone)). An iPhone is a client of the computer it controls, so it only looks and announces nothing. Besides the dial it keeps one browse of `_irohv1._udp` open while the endpoint lives: for every instance named by an endpoint id it reads the SRV, TXT and address records, so `nearby_peers()` lists the Macs by the text they advertise there too, and the first browse shows the Local Network prompt when the endpoint is bound. A Mac announces records for the system's browse to read; whether the system's cache keeps the address records of that announcement long enough to read them after the browse is confirmed only on a real phone, and until then the ticket's saved addresses stay the way back to a computer whose address did not change. A Bonjour failure (permission denied, Info.plist keys missing) is a message in the dial's error, not a failure of the endpoint.

*Changed:* `IrohConfig` gained `local_discovery` (on), `with_local_discovery`, `advertised` and `with_advertised`, its fields are ordered by decreasing alignment, `IrohEndpoint` gained `clear_allowed_peers`, `local_discovery_status`, `set_advertised` and `nearby_peers`, and `IrohEventKind` gained `NearbyAppeared` and `NearbyGone`.

The engine needs the dll's `iroh` feature, which `build-dll` enables; `PlatformCapability::iroh()` reports whether it is compiled in. In the browser the handle exists but does not bind yet. `examples/azul-meet` uses it for meetings: video and 20 ms audio packets as frames, mute state as messages (see [Realtime Media](../system/realtime-media.md#azmeet-meetings-audio-leaving)).

## What this page doesn't cover

- TLS configuration beyond the trusted roots and `disable_tls_cert_verification` (client certificates, protocol versions). For those, use `rustls` or a third-party HTTP client inside the worker; `TrustRoots::get_certificates_pem` gives it the app's roots.
- Mid-frame cancellation of in-flight DNS or TCP handshakes. `std::net` doesn't expose this. Use `socket2` or a third-party client if you need it.
- WebSockets, gRPC, HTTP/2. Any blocking client works inside a `Thread`.
