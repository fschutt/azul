# AzMeet Transport Design — WebRTC vs WebTransport vs iroh

**A design report for a peer-to-peer video conferencing tool that must run both as a native Rust desktop app and in web browsers.**

Date: 2026-07-07 · Status: research + recommendation · Author: research synthesis for the AzMeet maintainer

> Scope note: this report covers the **transport and media-plane architecture only**. It does not touch AzMeet application code, `api.json`, or the azul engine. All non-obvious claims are cited inline; see the Sources section for the full URL list. Items I could not verify against a primary source are explicitly flagged **[uncertain]**.

---

## 1. Executive summary

**Can iroh be the transport for AzMeet on the web? Yes — with one hard caveat and one architectural consequence.**

- **The caveat (the no-UDP question):** browsers cannot open raw UDP sockets, and iroh's fast path is QUIC-over-UDP hole-punching. So a **browser peer can never hole-punch** — its iroh traffic is **always relayed** through an iroh relay server (still end-to-end encrypted; the relay can't read it). Native↔native peers still get true direct P2P; native↔browser and browser↔browser are relay-bound. This is not a limitation you can engineer away today — it is stated plainly in iroh's own docs. ([iroh WASM/browser docs](https://docs.iroh.computer/deployment/wasm-browser-support))

- **The good news:** iroh **already compiles to WebAssembly and runs in the browser** (shipped in iroh 0.33, Feb 2025; maintained through the v1.0 release, June 2026), and browsers reach native iroh nodes over **WebSocket relays** (mandatory relay wire protocol since iroh 0.91, Aug 2025). So a browser AzMeet client can join an iroh network **today**, without you building a bridge — it just rides a relay. ([iroh 0.33 blog](https://www.iroh.computer/blog/iroh-0-33-0-browsers-and-discovery-and-0-RTT-oh-my), [iroh 0.91 "last relay break"](https://www.iroh.computer/blog/iroh-0-91-0-the-last-relay-break))

- **The decisive find:** n0 (the iroh team) ships **`iroh-live`** — an early-preview, full real-time **audio/video** pipeline over iroh, using **Media-over-QUIC (MoQ)** as the media layer, H.264/AV1/Opus codecs with hardware acceleration, multi-party rooms, **and optional relay bridging to browsers via WebTransport**. This is almost exactly the AzMeet architecture, built by the iroh authors themselves. It is a *tech preview* (basic A/V sync, no relay auth, incomplete Windows support, churning API), not a product — but it proves the path is real and shows the intended shape. ([iroh-live repo](https://github.com/n0-computer/iroh-live))

**Recommendation:** the maintainer's iroh lean is **viable and well-aligned with where iroh is going** — but AzMeet should adopt it as a **pluggable transport trait**, not a hard commitment, and be clear-eyed that **iroh moves bytes; it does not give you a media engine.** Unlike WebRTC, iroh/QUIC/MoQ hands you **no codecs, no echo cancellation, no adaptive congestion control tuned for conferencing** — you assemble those. AEC in pure Rust just became realistic in 2026 (`sonora`, `aec3`), but conferencing-grade congestion control over QUIC/MoQ is still an open problem per current sources. ([webrtcHacks: WebRTC vs MoQ](https://webrtchacks.com/webrtc-vs-moq-by-use-case/), [sonora](https://github.com/dignifiedquire/sonora))

**Bottom line by scenario:**

| If AzMeet's priority is… | Pick |
|---|---|
| Ship a working cross-browser call **fast**, get AEC/codecs/congestion-control for free | **WebRTC everywhere** (str0m or LiveKit SFU) |
| **Native-first**, best-in-class direct P2P for desktop, browsers as second-class relay clients | **iroh native P2P + iroh/WebTransport browser bridge** (the iroh-live shape) |
| Bet on the **future unified stack**, willing to own the media plane | **MoQ over QUIC (native) + WebTransport (browser)** with WebCodecs + Rust codecs |
| Hedge | **Pluggable transport trait**: iroh native + WebRTC/WebTransport web behind one interface |

---

## 2. The three candidates at a glance

### 2.1 WebRTC — the incumbent

WebRTC is the only transport that is **peer-to-peer, in the browser, with a built-in media engine** — all three at once. That combination is why it still wins meetings.

- **Topologies:** 1:1 P2P (direct, no server); **full mesh** (every peer sends to every peer — practical ceiling ~4–5 participants, "struggles beyond ~10–12"); **SFU** (server forwards each stream once — the dominant group-call architecture, 5→100+ participants); **MCU** (server mixes/transcodes — lowest client cost, highest server cost). "P2P at scale" is a half-truth: past ~5 people you need an SFU. ([Ant Media topology](https://antmedia.io/webrtc-network-topology/), [BlogGeek SFU](https://bloggeek.me/webrtcglossary/sfu/))
- **NAT traversal:** ICE gathers candidates (host / STUN-reflexive / TURN-relay); **STUN** discovers your public IP:port; **TURN** relays media when direct fails (symmetric NAT / CGNAT / restrictive firewalls). You **must run TURN** (e.g. coturn) and a signaling channel, or a substantial fraction — commonly cited ~15–30% behind restrictive NAT — of calls fail. ([MDN protocols](https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Protocols))
- **DataChannels vs media tracks:** media tracks (getUserMedia → SRTP) carry A/V and get auto encode/decode/jitter; DataChannels (SCTP-over-DTLS, reliable-or-not) carry chat/control. A video app uses both on one `PeerConnection`. ([MDN data channels](https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Using_data_channels))
- **Native Rust support (checked crates.io 2026-07):**
  - **`str0m` 0.21** — Sans-I/O (no runtime, no threads, "an enormous state machine"), runtime-agnostic; **run in production as a server SFU by Lookback**. Best architectural fit for Rust. **But: no media capture/codecs** — you supply camera + VP8/H.264/Opus yourself; the P2P/client path is less hardened than the SFU path. ([str0m](https://github.com/algesten/str0m))
  - **`webrtc` (webrtc-rs) 0.17.x stable** — a Tokio port of Pion, closest to the browser API; self-described **"early stage,"** mid-rewrite to Sans-I/O (0.20-rc). Most complete pure-Rust `RTCPeerConnection`. ([webrtc-rs](https://github.com/webrtc-rs/webrtc))
  - **libwebrtc bindings** — Google's C++ stack (what Chrome ships), via e.g. **LiveKit `rust-sdks`/`webrtc-sys`**. Most feature-complete and browser-grade, at the cost of a heavy C++ build + FFI. **[uncertain]** exact current maturity/version not verified this pass. ([LiveKit rust-sdks](https://github.com/livekit/rust-sdks))
- **Browser support:** effectively universal for `RTCPeerConnection` + media across Chrome/Edge/Firefox/Safari — but **not uniform**: Safari/iOS is H.264-first and conservative on VP9/AV1; iOS forces all browsers onto WebKit. Negotiate **H.264 (and/or VP8) as the baseline** for cross-browser reach. ([Ant Media browser support](https://antmedia.io/webrtc-browser-support/))
- **Strengths:** works in the browser *and* native Rust; batteries-included media engine (AEC/NS/AGC/jitter/adaptive bitrate for free); mature; SFU-in-Rust is a proven pattern.
- **Weaknesses:** the SDP/ICE/DTLS/SCTP/SRTP machinery is heavy; mesh doesn't scale; you must operate TURN + signaling; the native Rust libraries are libraries, not turnkey clients (str0m gives you no codecs; webrtc-rs is early).

### 2.2 WebTransport — HTTP/3 (QUIC) in the browser

WebTransport is a **client↔server** low-latency API over **HTTP/3 / QUIC**, exposing both **reliable ordered streams** (WebSocket-like) and **unreliable unordered datagrams** (UDP-like, drop-old-frames semantics ideal for media). ([W3C explainer](https://github.com/w3c/webtransport/blob/main/explainer.md))

- **The hard limit (verified, primary source):** the W3C explainer states verbatim — **"WebTransport is strictly client-server. P2P use cases should continue to use WebRTC."** There is **no browser-to-browser mode** and no active proposal to add one; it always needs an HTTP/3 server endpoint. So WebTransport can only ever be the **client↔media-server leg** of a conferencing design, never direct peer media. ([W3C explainer](https://github.com/w3c/webtransport/blob/main/explainer.md))
- **Congestion control / HoL:** all traffic is **congestion-controlled and encrypted by QUIC** (CUBIC by default, BBR available — that's an implementation property of the QUIC stack, not mandated by the spec **[implementation-level]**). QUIC's independent streams mean **one lost packet doesn't stall the others** — the TCP/WebSocket head-of-line problem is gone. ([WebKit Safari 26.4](https://webkit.org/blog/17862/webkit-features-for-safari-26-4/))
- **Browser support (late 2025 / 2026) — now Baseline:**
  - Chrome 97+ · Edge 98+ · Firefox 114+.
  - **Safari / iOS: NOT supported through 26.3; shipped in Safari 26.4 (release notes dated March 24, 2026).** This is the change that made WebTransport **Baseline** (all major engines) in 2026. **Caveat:** the older-Safari/iOS installed base still has zero support for some time. ([caniuse WebTransport](https://caniuse.com/webtransport), [WebKit 26.4](https://webkit.org/blog/17862/webkit-features-for-safari-26-4/), [webrtc.ventures](https://webrtc.ventures/2026/04/webtransport-is-now-baseline-what-it-means-for-real-time-media/))
- **Rust server support:** `wtransport` (simplest standalone server, ~v0.7, "functional but not fully production-ready"); `web-transport-quinn` (on Quinn, the maintained one — supersedes deprecated `webtransport-quinn`); `h3-webtransport` (multiplex on an existing h3 server). All are Quinn/h3-family. ([wtransport](https://github.com/BiagioFesta/wtransport), [web-transport-quinn](https://docs.rs/web-transport-quinn/latest/web_transport_quinn/))
- **Role for AzMeet:** WebTransport is **not a WebRTC replacement for P2P** — it's the **browser's on-ramp to a server/relay**. It is the foundation MoQ builds on, and it's how iroh's future direct-browser story (and iroh-live's browser bridge today) reaches the web.

### 2.3 iroh — QUIC (Quinn/noq) P2P with relays and hole-punching

iroh dials **public keys, not IPs** (`NodeId` = ed25519 key); it finds and maintains the fastest path automatically, hole-punching to a direct QUIC/UDP connection where possible and falling back to encrypted relays otherwise. ([iroh GitHub](https://github.com/n0-computer/iroh))

- **Maturity:** **iroh 1.0 shipped June 15, 2026** (first stable after 65 releases); current v1.0.2 (July 6, 2026). Production users cited by n0: **Rave (video to 600k concurrent connections per relay)**, Nous Research (distributed training), Paycode (payments). ([iroh v1 blog](https://www.iroh.computer/blog/v1))
- **QUIC engine:** originally built on **Quinn**; as of **iroh v0.96 (March 2026) it hard-forked Quinn into `noq`**, adding **Multipath QUIC** (relay path and direct path are now first-class QUIC paths with independent congestion state), QUIC Address Discovery (replaces STUN), and standards-track QUIC NAT traversal. So "iroh is built on Quinn" is now historically true; current iroh runs on noq. ([noq announcement](https://www.iroh.computer/blog/noq-announcement))
- **NAT traversal (Tailscale/DERP heritage):** both peers meet at a shared relay; the relay observes public IP:ports; peers exchange addresses and attempt simultaneous outbound; on success the relay path falls away. The relay is a **blind, stateless forwarder** — it can't decrypt traffic. ~90% of environments achieve direct P2P **[the 90% figure is from a secondary analysis, not n0 docs]**. ([iroh holepunching docs](https://docs.iroh.computer/concepts/holepunching))
- **Discovery:** DNS/Pkarr (n0 runs `dns.iroh.link`), Pkarr-over-Mainline-DHT, and mDNS for LAN. `iroh-gossip` (pub/sub overlay) is a natural fit for **room membership / signaling**, and it too compiles to the browser. ([iroh DNS blog](https://www.iroh.computer/blog/iroh-dns))
- **Real-time video:** **`iroh-live`** — n0's own early-preview A/V pipeline over iroh + MoQ (H.264/AV1/Opus, HW accel, 1:1 and multi-party rooms, WebTransport browser bridge). Tech-preview caveats: basic A/V sync, no relay auth, incomplete Windows, churning API. ([iroh-live](https://github.com/n0-computer/iroh-live))

---

## 3. The browser / no-UDP question, answered explicitly

**Q: How does iroh reach a browser peer that has no UDP?**

**A: It doesn't hole-punch to the browser — it relays. And that already works today.** Concretely, verified against iroh's own docs and blog:

1. **iroh compiles to WASM and runs in the browser.** Official: *"Iroh can be compiled to WebAssembly for use in browsers!"* (target `wasm32-unknown-unknown`, via `wasm-bindgen`; requires `default-features = false`). Shipped in iroh **0.33.0 (Feb 25, 2025)** for `iroh` core + `iroh-gossip`; the WASM tracking issue #2799 is marked Done at the v1.0 milestone. ([iroh WASM docs](https://docs.iroh.computer/deployment/wasm-browser-support), [iroh 0.33 blog](https://www.iroh.computer/blog/iroh-0-33-0-browsers-and-discovery-and-0-RTT-oh-my), [issue #2799](https://github.com/n0-computer/iroh/issues/2799))

2. **A browser cannot send UDP from the sandbox, so ALL browser connections flow through a relay.** Official: *"All connections from browsers to somewhere else need to flow via a relay server… even though traffic from browsers is always relayed, it can't be decrypted by the relay."* Browser nodes therefore **cannot use hole-punching**. This is the fundamental, unavoidable limitation. ([iroh WASM docs](https://docs.iroh.computer/deployment/wasm-browser-support))

3. **The relay is browser-reachable because it speaks WebSocket.** As of **iroh 0.91.0 (Aug 1, 2025)** the relay wire protocol was standardized to **WebSocket-only** (raw-TCP relay dropped) — the "last relay break" — specifically so browsers can initiate relay connections. (Browsers pay extra handshake round-trips because they lack the TLS Keying Material Exporter API.) ([iroh 0.91 blog](https://www.iroh.computer/blog/iroh-0-91-0-the-last-relay-break))

4. **"iroh over WebTransport" for DIRECT browser connections is NOT shipped — it's planned/exploratory.** WebTransport today requires DNS names + valid TLS certs, which blocks browser-initiated direct peer links; n0 lists `serverCertificateHashes`-style WebTransport and WebRTC as *future* possibilities to someday give browsers a **direct** path. Until then, browser = relayed. ([iroh & the Web](https://www.iroh.computer/blog/iroh-and-the-web), [iroh WASM docs](https://docs.iroh.computer/deployment/wasm-browser-support))

5. **Is native↔browser P2P possible today?** **Connectivity: yes** — a browser iroh node can talk to a native iroh node right now (via relay). **Direct/hole-punched P2P with a browser endpoint: no** — always relayed. So for AzMeet, plan for browser participants to be **relay-bound**, and budget relay bandwidth/latency accordingly. ([iroh & the Web](https://www.iroh.computer/blog/iroh-and-the-web), [iroh WASM docs](https://docs.iroh.computer/deployment/wasm-browser-support))

**Contrast:** WebRTC solves the same browser problem differently — it gives the browser a *direct* P2P media path (ICE hole-punching in the browser itself), falling back to TURN relay. iroh gives the browser a *relayed* path only. For **native↔native**, iroh's direct P2P is excellent; for **anything involving a browser**, both stacks end up relaying a large share of traffic — WebRTC via TURN, iroh via its relays.

---

## 4. Comparison table

| Dimension | **WebRTC** | **WebTransport** | **iroh (QUIC/noq)** |
|---|---|---|---|
| **Native P2P (desktop↔desktop)** | Yes (direct, ICE) | No — client↔server only | **Yes — direct hole-punched QUIC** (its core strength) |
| **Browser support** | Universal (Chrome/FF/Safari); Safari H.264-first | Baseline 2026 (Safari since **26.4**, Mar 2026); older iOS none | **Yes via WASM**, but **relay-only** (no browser hole-punch) |
| **Needs a server/relay?** | TURN needed for ~15–30% of calls; SFU for >5 people; signaling always | **Always** (it *is* client↔server) | Relay needed for browsers + NAT fallback; native↔native can be serverless |
| **Media codecs provided?** | **Yes** — built-in engine (VP8/H.264 MTI, VP9/AV1 opportunistic) | **No** — transport only | **No** — transport only (iroh-live wires H.264/AV1/Opus itself) |
| **Echo cancellation / AEC / jitter** | **Yes**, built-in (AEC3/NS/AGC/jitter buffer) | No | No — bring your own (`sonora`/`aec3`, new in 2026) |
| **Congestion control** | **Yes**, conferencing-tuned (`gcc`, years of tuning) | QUIC's (CUBIC/BBR) — not conferencing-tuned | QUIC/noq multipath — **not yet conferencing-tuned** |
| **Rust maturity** | str0m 0.21 (prod SFU at Lookback); webrtc-rs 0.17 "early"; libwebrtc via LiveKit | wtransport / web-transport-quinn (usable, young) | **iroh 1.0 (Jun 2026), production users**; iroh-live preview |
| **NAT traversal** | ICE + STUN + TURN (mature, you operate it) | N/A (server-terminated) | **Built-in** hole-punch + relay fallback (Tailscale/DERP heritage) |

*MTI = mandatory-to-implement. VP8 and H.264 Constrained Baseline are WebRTC MTI per RFC 7742 — unchanged in 2025/2026. ([RFC 7742](https://datatracker.ietf.org/doc/html/rfc7742))*

---

## 5. Architecture options

Four concrete shapes for a native-Rust + web AzMeet, ordered from most conservative to most forward-looking.

### Option A — WebRTC everywhere (SFU)

```
   Native Rust client            Browser client
   (str0m / libwebrtc)           (RTCPeerConnection)
          │  WebRTC/SRTP                 │  WebRTC/SRTP
          └───────────────┬─────────────┘
                          ▼
                 ┌──────────────────┐
                 │   WebRTC SFU      │  (str0m SFU, or LiveKit-Go)
                 │  + TURN + signal  │
                 └──────────────────┘
```

- **How:** everyone — native and browser — speaks WebRTC to a central SFU. Native Rust joins via str0m or the LiveKit Rust SDK; browsers use `RTCPeerConnection`. Small calls can use mesh P2P; groups use the SFU.
- **Pros:** one protocol both sides; **AEC / codecs / jitter / adaptive bitrate for free** on every client; Safari/iOS works; SFU-in-Rust is proven (Lookback runs str0m). Lowest media-plane risk.
- **Cons:** you operate SFU + TURN + signaling; the "P2P" story is mostly SFU; heavy SDP/ICE machinery; str0m gives no native codecs (you add camera capture + VP8/H.264/Opus) or you take libwebrtc's C++ dependency.
- **Effort:** **Medium.** Lowest-risk path to a shipping cross-browser call. Most of the effort is ops (SFU/TURN) + native capture/codecs.

### Option B — iroh native P2P + iroh/WebTransport browser bridge (the iroh-live shape) ★ recommended for the iroh lean

```
  Native Rust ◄──── direct hole-punched QUIC (iroh) ────► Native Rust
     │                                                        │
     │  (native↔native = TRUE P2P, no server)                 │
     └───────────────► iroh relay ◄──────────────────────────┘
                          ▲   ▲
       WebSocket relay ───┘   └─── WebTransport bridge
             │                            │
        Browser (WASM iroh,          Browser (WebTransport +
         relay-bound)                 WebCodecs, MoQ media)
```

- **How:** native peers run iroh and get **real direct P2P** for desktop↔desktop. Media rides **MoQ over QUIC** (independent streams per track — a dropped video packet never blocks audio). Browsers join **relay-bound**: either as WASM iroh nodes over WebSocket relay, or via a WebTransport bridge carrying MoQ + WebCodecs — exactly what **iroh-live** already prototypes. Use `iroh-gossip` for room membership/signaling.
- **Pros:** best-in-class **native P2P**; matches the maintainer's lean and n0's own direction; unifies native+browser on the QUIC family; NAT traversal is built-in; you can start from `iroh-live` as a reference.
- **Cons:** browsers are **second-class** (always relayed — no direct browser media). **You own the media plane**: codecs (rav1e/dav1d/openh264/libvpx), **AEC** (`sonora`/`aec3` — new, v0.1.x, unproven at scale), and — the real open gap — **conferencing-grade congestion control over QUIC/MoQ, which current sources say does not yet exist.** iroh-live itself is a tech preview (basic A/V sync, no relay auth, Windows gaps).
- **Effort:** **High.** Highest-ceiling, highest-risk. Realistic as native-first now, browser-parity later as iroh's direct-browser story matures.

### Option C — MoQ over QUIC (native) + WebTransport (browser), transport-agnostic

```
  Native Rust ──QUIC──►┐                    ┌◄──QUIC── Native Rust
                       │                    │
                       ▼                    ▼
                 ┌──────────────────────────────┐
                 │        MoQ relay(s)           │  (moq-dev/moq, Rust)
                 └──────────────────────────────┘
                       ▲                    ▲
        WebTransport ──┘                    └── WebTransport
             │                                    │
        Browser (WebCodecs)               Browser (WebCodecs)
```

- **How:** drop iroh; standardize on **Media-over-QUIC (MoQ)** — pub/sub over QUIC (native) and WebTransport (browser). Reference impl `moq-dev/moq` is Rust. Browsers use WebCodecs for encode/decode.
- **Pros:** single media protocol across native + browser; genuine 2025/2026 industry momentum (Cloudflare; 11 vendors demoed at NAB 2026; nanocosmos in production); clean architecture.
- **Cons:** MoQ's pub/sub relay model is built for **one-to-many distribution, not symmetric two-way calls**; **congestion/bandwidth control is immature** — webrtcHacks (Nov 2025) argues WebRTC stays for meetings precisely because MoQ "doesn't have any of that today." No P2P at all (always via relay). You still own AEC. Safari WebTransport only since 26.4.
- **Effort:** **High**, and **betting on immature tech** for the conferencing (symmetric) case. Great for one-to-many/broadcast; risky for meetings today.

### Option D — Pluggable transport trait (hedge) ★ recommended engineering posture

```
                ┌─────────────────────────────────────┐
                │   AzMeet core: rooms, media pipeline, │
                │   codecs (rav1e/openh264/libvpx),     │
                │   AEC (sonora), jitter, UI            │
                └─────────────────────────────────────┘
                              │  trait Transport
        ┌─────────────────────┼──────────────────────┐
        ▼                     ▼                        ▼
  IrohTransport        WebRtcTransport         WebTransportMoq
  (native P2P)         (browser + Safari,      (future unified,
                        AEC/CC for free)        experimental)
```

- **How:** define `trait Transport { connect, send_track, recv_track, datachannel… }`. Ship **iroh for native** (direct P2P) and **WebRTC for browsers** (mature, Safari-safe, media engine included) behind it, and keep a **MoQ/WebTransport** implementation as an experimental third backend to grow into.
- **Pros:** de-risks the iroh bet — you get iroh's native P2P **and** WebRTC's browser maturity without betting the product on either; lets browsers use WebRTC's built-in AEC/congestion control while native peers stay pure-Rust P2P; swap backends as iroh's direct-browser story lands.
- **Cons:** two (eventually three) transports to build and test; you own a **bridge/gateway** where an iroh-native peer and a WebRTC-browser peer meet (no turnkey iroh↔browser media bridge exists — you'd build it). Media/codec negotiation must be normalized across backends.
- **Effort:** **Medium-High.** The pragmatic long-term answer: matches transports to where each is strongest, and the trait boundary is cheap insurance given how fast this space moves.

---

## 6. Media / codec plane — "transport moves bytes; you still need a codec + echo cancellation"

This is the part that most often sinks non-WebRTC designs, so it is called out explicitly per option.

**Codecs.** Conferencing uses VP8 / H.264 (gen 1, the workhorses), VP9 (gen 2), AV1 (gen 3, with SVC). **VP8 and H.264 Constrained Baseline are WebRTC mandatory-to-implement (RFC 7742, unchanged 2026).** **AV1 is not yet production-ready for two-way calling across browsers** — real-time software encode is CPU-heavy, hardware encode is limited to newer high-end chips, and **Safari's WebRTC stack exposes no AV1 encode at all**; realistic broad AV1 SVC is ~2028+, with screen-sharing the furthest-ahead case. Negotiate **H.264/VP8 baseline**, AV1/VP9 opportunistically. ([RFC 7742](https://datatracker.ietf.org/doc/html/rfc7742), [Trembit AV1 2026](https://trembit.com/blog/av1-in-2026-why-the-next-gen-codec-still-isnt-dominant/))

**Who provides the media engine in each option:**

- **Browser, WebRTC path (Options A, D-browser):** the browser's **built-in libwebrtc engine** gives you encode/decode + **AEC, noise suppression, AGC, jitter buffer, adaptive bitrate** for free via getUserMedia + RTCPeerConnection. This is WebRTC's biggest hidden advantage. ([MDN WebRTC codecs](https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/WebRTC_codecs))
- **Browser, WebTransport/MoQ path (Options B-browser, C):** **WebCodecs** gives raw encode/decode (H.264/H.265/AV1/VP8/VP9; Opus/AAC audio) — but **deliberately excludes AEC/NS/AGC and the jitter buffer** **[inference from API scope — no source states the negative outright]**. Browser support: Chrome/Edge 94+, Firefox 130+ (no Android), **Safari full only in 26.0+ (2026)**. So you must supply AEC and buffering yourself, and Safari/Firefox-Android have gaps. ([caniuse WebCodecs](https://caniuse.com/webcodecs), [MDN WebCodecs](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API))
- **Native Rust, non-WebRTC path (Options B, C native):** codecs come from **`rav1e`** (AV1 encode), **`dav1d`/`rav1d`** (AV1 decode), **`openh264`** (H.264), **`libvpx`/`vpx-encode`** (VP8/VP9). **Echo cancellation** was historically *the* hard gap — but in 2026 it's newly tractable in pure Rust: **`sonora`** (a pure-Rust port of libwebrtc's audio-processing module M145 — AEC3 + NS + AGC, SIMD, first v0.1.0 ~Feb 2026, by n0/iroh contributor dignifiedquire) and **`aec3`/`aec3-rs`**. **Caveat:** these are v0.1.x and unbenchmarked in real calls — verify latency/robustness yourself. ([rav1e](https://docs.rs/rav1e), [openh264](https://crates.io/crates/openh264), [sonora](https://github.com/dignifiedquire/sonora))
- **The unsolved gap (Options B, C):** **conferencing-grade congestion / bandwidth control.** WebRTC's `gcc` "took years to tune"; MoQ/QUIC "doesn't have any of that today" for symmetric calls per webrtcHacks (Nov 2025). This — not codecs, not even AEC anymore — is the strongest reason to keep WebRTC in the mix for browser participants. ([webrtcHacks](https://webrtchacks.com/webrtc-vs-moq-by-use-case/))

---

## 6.5 Layout-driven resolution + source-specific codec (azul's structural advantage)

Standard conferencing clients treat the codec and the UI as separate systems that guess at
each other's needs. AzMeet doesn't have to: **every video tile is a DOM node with a computed
`LogicalRect`, so the layout solver already knows the exact pixel size each remote stream will
be drawn at.** That collapses several hard problems into one.

**Bandwidth scales with *displayed pixels*, not participant count.** This defeats the naive
"per-direction × N participants" cost model. You physically cannot fit 100×1080p on a screen,
so you never *request* it: a 10×10 grid means each tile is ~160×90 and is encoded/requested at
that resolution. Total received pixels is bounded by the local screen, so **received bandwidth
is roughly constant regardless of room size** — it just gets divided into more, smaller tiles.
A 2-person call doesn't double 1080p; each face is now half-screen, so each is requested at
lower resolution. **The layout engine is the rate controller** — and it re-adapts reactively
for free (window resize / someone joins → grid reflows → tiles shrink → lower resolution
requested), because that's just a normal relayout.

- **Mechanism.** The sender encodes, so the receiver's tile size flows back over a control
  message ("send me ≤ WxH @ F fps for stream S"). For **P2P mesh** (one receiver per sender
  pair) this receiver-request model is exact and cheap — one round-trip to adapt. For
  fan-out to many viewers you'd instead **simulcast** (encode 2–3 fixed resolutions) or use
  **SVC** (layered). **Caveat: hardware H.264 generally cannot do SVC** — with the HW-H.264
  plan the realistic options are *receiver-request* (P2P) or *simulcast* (SFU), not SVC. AV1/VP9
  offer real SVC but lack broad HW encode. **[HW-SVC limitation is a well-known constraint; verify per-chip.]**
- **Hysteresis.** Debounce resolution changes (a transient reflow shouldn't trigger a keyframe
  storm); switch resolution only on a settled tile size, and only *up* after a short dwell.

**Codec strategy forks by source — and screen-share + audio matter more than camera video.**

| Source | Motion profile | Codec mode | Resolution | Frame rate | Priority |
|---|---|---|---|---|---|
| **Audio** | — | Opus, per-packet independent | — | 50/s (20 ms) | **highest — protect last** |
| **Screen share** | near-static, small dirty regions | **interframe / P-frames** (diffs are tiny) | **true / native** (text must stay crisp) | low / event-driven (5–15 fps) | **high** |
| **Camera video** | unpredictable, whole-frame | **all-intra or short-GOP + LTR** | **follows tile size** (§ above) | 24–30 fps | **lowest — drop first** |

Screen content is mostly unchanged frame-to-frame, so P-frames (the encoder's motion estimation
finds ~zero delta on static regions) give near-lossless compression at true resolution — the
opposite of camera video, where all-intra buys loss-tolerance at a bitrate cost. Never all-intra
a screen share; never long-P-chain a lossy camera feed.

**Loss resilience without a keyframe-per-frame: Long-Term Reference (LTR) frames.** The
"timestamp the I-frames; a P-frame diffs against the I-frame from N seconds ago" idea is exactly
the **LTR** technique (H.264/H.265 long-term reference pictures; used by WebRTC for error
resilience; exposed by NVENC / VideoToolbox / MediaCodec). The design:

```
[keyframe job]  emit I-frame every N s, tagged with a monotonic LTR id + timestamp
[encoder]       P-frames reference the *current LTR id*, NOT the immediately-previous frame
[decoder]       "I hold LTR #k → any P-frame with ref=k decodes standalone"
```

- **Why it's better than a plain P-chain:** a single lost P-frame does **not** break the stream —
  the next P-frame still references the LTR, not the lost frame. A late joiner / recovering peer
  just waits for the next LTR (≤ N s distortion), then applies P-frames. Bounded, self-healing,
  no per-frame retransmit.
- **Bitrate:** sits between all-intra and full-IPPP — cheaper than all-intra, more resilient than
  a long chain.
- **Caveat:** a P-frame against a *fixed* LTR grows as the scene drifts from it (5 s of movement
  ⇒ big diff), so quality/bitrate **sags toward the end of each interval**. Make **N
  motion-adaptive** (shorten under high motion) or allow a short secondary P-chain off recent
  frames for the high-motion case. The two "jobs" (keyframe publisher + P-encoder) map cleanly
  onto two azul `Timer`s / worker threads.

**Priority is a transport-QoS rule, not just an encoder setting.** audio > screenshare > camera:
carry **audio on low-latency datagrams** (drop-tolerant, protected to the last), **screen share
on a prioritized stream**, and make **camera video the first thing shed under congestion**
(drop fps, then resolution, then the stream). On iroh/QUIC this is native stream priorities +
datagrams; it's also the single most important knob for perceived call quality.

---

## 7. Recommendation for the maintainer

1. **Yes, lean into iroh — for the native plane.** iroh 1.0 is stable, production-proven, and gives AzMeet exactly what WebRTC struggles at: **true direct desktop↔desktop P2P** with built-in hole-punching. Native↔native AzMeet on iroh is the right call.

2. **Accept that browsers are relay-bound on iroh, and don't fight it.** iroh in the browser works today (WASM + WebSocket relay) but **cannot hole-punch and cannot do direct browser media** — that's an iroh roadmap item, not a today feature. Budget relay bandwidth for every browser participant.

3. **Do not make iroh a hard dependency — put it behind a `trait Transport` (Option D).** Ship **iroh for native + WebRTC for browsers** first. WebRTC in the browser hands you AEC, jitter, and conferencing-tuned congestion control for free and covers Safari/iOS — closing the two areas where a pure iroh/MoQ browser client is weakest today. This is the lowest-regret path and preserves the iroh bet.

4. **Study `iroh-live` closely — it is your reference architecture.** It is the iroh team building the AzMeet shape (iroh + MoQ + WebTransport browser bridge). Track it, but treat it as a preview, not a dependency.

5. **Plan the media plane deliberately, early.** Whatever transport wins, on the native/non-WebRTC path you own codecs (rav1e/openh264/libvpx), AEC (`sonora`/`aec3` — promising but young), and congestion control (the real risk). Prototype **echo cancellation and adaptive bitrate first** — they are what will actually make or break call quality, not the transport.

6. **Watch MoQ (Option C) but don't bet meetings on it yet.** Strong industry momentum, clean native+browser story, but immature congestion control for symmetric two-way calls as of late 2025. Revisit in 6–12 months.

7. **Make layout-driven resolution the headline differentiator (§6.5).** Because every stream is a DOM tile with a known pixel rect, the layout solver *is* the rate controller: received bandwidth is bounded by screen pixels, not participant count, and adapts on reflow for free. Pair it with source-specific codecs (screen = P-frames/true-res; camera = all-intra/short-GOP + **LTR** for loss resilience) and the QoS priority audio > screenshare > camera. This is the part no browser stack gets for free — it falls out of azul's architecture — and it, more than the transport choice, is what makes AzMeet scale to large rooms.

**One-line answer to the dilemma:** *iroh can absolutely be AzMeet's native transport and can even reach browsers today (via relay) — but it is not, by itself, a browser conferencing stack. Ship iroh natively behind a transport trait, bridge browsers with WebRTC (now) and iroh/WebTransport+MoQ (as it matures), and invest as much in the codec/AEC/congestion-control media plane as in the transport itself.*

---

## 8. Sources

**iroh (browser, architecture, video):**
- iroh WASM / browser support docs — https://docs.iroh.computer/deployment/wasm-browser-support
- iroh 0.33 "browsers, discovery & 0-RTT" — https://www.iroh.computer/blog/iroh-0-33-0-browsers-and-discovery-and-0-RTT-oh-my
- iroh 0.32 browser alpha — https://www.iroh.computer/blog/iroh-0-32-0-browser-alpha-qad-and-n0-future
- iroh 0.91 "the last relay break" (WebSocket-only relays) — https://www.iroh.computer/blog/iroh-0-91-0-the-last-relay-break
- "Iroh & the Web" roadmap — https://www.iroh.computer/blog/iroh-and-the-web
- iroh v1.0 announcement — https://www.iroh.computer/blog/v1
- noq (Quinn hard-fork) announcement — https://www.iroh.computer/blog/noq-announcement
- iroh on QUIC-multipath — https://www.iroh.computer/blog/iroh-on-QUIC-multipath
- iroh holepunching concepts — https://docs.iroh.computer/concepts/holepunching
- iroh DNS/Pkarr discovery — https://www.iroh.computer/blog/iroh-dns
- iroh GitHub — https://github.com/n0-computer/iroh
- WASM tracking issue #2799 — https://github.com/n0-computer/iroh/issues/2799
- **iroh-live** (A/V over iroh + MoQ + WebTransport bridge) — https://github.com/n0-computer/iroh-live
- noq repo — https://github.com/n0-computer/noq

**WebRTC:**
- MDN WebRTC protocols (ICE/STUN/TURN) — https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Protocols
- MDN using data channels — https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Using_data_channels
- MDN WebRTC codecs — https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/WebRTC_codecs
- Ant Media — network topology (mesh/SFU/MCU) — https://antmedia.io/webrtc-network-topology/
- Ant Media — browser support 2026 — https://antmedia.io/webrtc-browser-support/
- BlogGeek — SFU — https://bloggeek.me/webrtcglossary/sfu/
- str0m — https://github.com/algesten/str0m
- webrtc-rs — https://github.com/webrtc-rs/webrtc · https://webrtc.rs/
- webrtc-rs/sfu — https://github.com/webrtc-rs/sfu
- LiveKit Rust SDKs — https://github.com/livekit/rust-sdks
- RFC 7742 (WebRTC video codecs / MTI) — https://datatracker.ietf.org/doc/html/rfc7742

**WebTransport:**
- W3C WebTransport explainer ("strictly client-server") — https://github.com/w3c/webtransport/blob/main/explainer.md
- Chrome for Developers WebTransport — https://developer.chrome.com/docs/capabilities/web-apis/webtransport
- caniuse WebTransport — https://caniuse.com/webtransport
- WebKit — Safari 26.4 features (WebTransport ships) — https://webkit.org/blog/17862/webkit-features-for-safari-26-4/
- webrtc.ventures — WebTransport is now Baseline — https://webrtc.ventures/2026/04/webtransport-is-now-baseline-what-it-means-for-real-time-media/
- wtransport — https://github.com/BiagioFesta/wtransport
- web-transport-quinn — https://docs.rs/web-transport-quinn/latest/web_transport_quinn/

**Media-over-QUIC (MoQ) & codecs / AEC:**
- IETF MoQ WG — https://datatracker.ietf.org/group/moq/about/
- moq-transport draft — https://moq-wg.github.io/moq-transport/draft-ietf-moq-transport.html
- moq-dev/moq (Rust reference impl) — https://github.com/moq-dev/moq · https://doc.moq.dev/
- Cloudflare on MoQ — https://blog.cloudflare.com/moq/
- nanocosmos on MoQ (production, browser reach) — https://www.nanocosmos.net/blog/media-over-quic-moq/
- webrtcHacks — WebRTC vs MoQ by use case (Nov 2025, congestion-control gap) — https://webrtchacks.com/webrtc-vs-moq-by-use-case/
- Trembit — AV1 in 2026 (why not dominant) — https://trembit.com/blog/av1-in-2026-why-the-next-gen-codec-still-isnt-dominant/
- caniuse WebCodecs — https://caniuse.com/webcodecs
- MDN WebCodecs — https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API
- rav1e (AV1 encode) — https://docs.rs/rav1e
- openh264 crate — https://crates.io/crates/openh264
- **sonora** (pure-Rust libwebrtc AEC3, 2026) — https://github.com/dignifiedquire/sonora
- aec3 crate — https://crates.io/crates/aec3

**Uncertainty flags (see inline [uncertain]/[inference] notes):** current libwebrtc-Rust-binding maturity not verified this pass; the ~90% iroh direct-connect figure and relay wire-protocol details come from secondary analyses, not n0 primary docs; "WebCodecs/MoQ exclude AEC" is inference from API/protocol scope; `sonora`/`aec3` are v0.1.x and unbenchmarked in real calls; the `sonora`↔iroh association is circumstantial (shared contributor, not an official tie).
