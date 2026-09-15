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

The same idea applies to threads. `ThreadPool::create(n)` starts `n` workers,
and `pool.create_thread(...)` returns an ordinary `Thread` whose body runs on
one of them, so many short jobs don't each start an OS thread.

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

- `connect(ticket)` dials. The outcome arrives as a `PeerConnected` or `Error` event.
- `send_frame(peer, track, data)` and `broadcast_frame(track, data)` send each frame on its own QUIC stream. A frame that has not left yet is replaced by the next frame of the same track, so a slow link lowers the frame rate instead of adding latency. JPEG frames from `RawImage::encode_jpeg` are the simplest video format.
- `send_message(peer, data)` is reliable and ordered, for chat and control data.
- `peer_stats(peer)` reports whether the path is direct or relayed, the RTT, the congestion window and frame counters.
- Nothing arrives unless you poll `recv`, so drive it from a timer.
- For rooms, `IrohLoadBalancer` picks the peers that forward media for everyone (`backbone_size`, `select_backbone`), and `IrohTileRole::rendition_height` picks the resolution a video tile should request.

The engine needs the dll's `iroh` feature, which `build-dll` enables; `PlatformCapability::iroh()` reports whether it is compiled in. In the browser the handle exists but does not bind yet. `examples/azul-meet` opens two windows that exchange camera and screen frames this way.

## What this page doesn't cover

- TLS configuration beyond `disable_tls_cert_verification`. For custom TLS stacks, use `rustls`, `native-tls`, or a third-party HTTP client inside the worker.
- Mid-frame cancellation of in-flight DNS or TCP handshakes. `std::net` doesn't expose this. Use `socket2` or a third-party client if you need it.
- WebSockets, gRPC, HTTP/2. Any blocking client works inside a `Thread`.
