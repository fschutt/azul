---
slug: networking
title: Networking
language: en
canonical_slug: networking
audience: external
maturity: stub
guide_order: 270
topic_only: false
prerequisites: [background-tasks]
tracked_files:
  - core/src/task.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T12:00:00Z
---

# Networking

> **Not yet functional.** Azul does not ship a built-in networking layer.
> The `AzTcp` / `AzUdp` types described below are planned but unimplemented.
> Today, do networking the same way you do any other blocking I/O: from
> inside a [`Thread`](background-tasks.md).

## Status

| Component | State |
|---|---|
| `AzTcp`, `AzUdp` socket types | not implemented |
| `ConnectionStatus` enum integrated with the event loop | not implemented |
| Async runtime integration | not planned — bring your own |
| `Thread`-based blocking I/O | works today (see [background-tasks](background-tasks.md)) |

The framework is intentionally runtime-agnostic. The eventual networking
API will be a thin FFI-safe wrapper over the OS socket APIs, surfacing
events through the same `WriteBackCallback` mechanism `Thread` uses.

## Today: blocking I/O inside a thread

The interim pattern is identical to any blocking task. Wrap `std::net`
calls in a `Thread` callback and post results back via
`ThreadReceiveMsg::WriteBack`:

```rust,ignore
extern "C" fn http_get(
    mut initial: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let url = match initial.downcast_ref::<String>() {
        Some(s) => s.clone(),
        None    => return,
    };

    // any blocking HTTP client works here — ureq, reqwest::blocking, etc.
    let body: Vec<u8> = match ureq::get(&url).call().and_then(|r| {
        let mut buf = Vec::new();
        r.into_reader().read_to_end(&mut buf).map_err(Into::into).map(|_| buf)
    }) {
        Ok(b)  => b,
        Err(e) => {
            sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
                refany:   RefAny::new(FetchError(format!("{e}"))),
                callback: WriteBackCallback { cb: apply_error, ctx: OptionRefAny::None },
            }));
            return;
        }
    };

    // cooperative cancellation
    if recv.recv().into_option() == Some(ThreadSendMsg::TerminateThread) { return; }

    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany:   RefAny::new(FetchOk(body)),
        callback: WriteBackCallback { cb: apply_body, ctx: OptionRefAny::None },
    }));
}
```

The `apply_body` and `apply_error` callbacks run on the main thread and
mutate the application's `RefAny` model in the usual way — see
[background-tasks](background-tasks.md) for the full pattern.

## Modelling connection state

Use a plain enum on the application data side. A typical shape:

```rust,ignore
enum ConnectionStatus {
    Idle,
    Connecting { thread_id: ThreadId, started: Instant },
    Open       { stream:    RefAny    /* hold the live socket */ },
    Closed     { reason:    String   },
}
```

Cancel by calling `event.remove_thread(thread_id)` from a click handler;
the `Thread::Drop` impl sends `TerminateThread` and joins. If your
worker checks `recv.recv()` between operations, cancellation is prompt.

## Using an async runtime

The framework does not host a runtime, but nothing prevents you from
running one inside a `Thread`:

```rust,ignore
extern "C" fn tokio_worker(
    initial: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
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

A current-thread runtime keeps everything on the worker. Use a
multi-threaded runtime if you need a worker pool — but spawn it once and
reuse, runtimes are expensive to construct.

## Planned API (not implemented)

The intended shape, once `AzTcp` lands:

```rust,ignore
let socket = AzTcp::connect("api.example.com:443")?;
let id     = ConnectionId::unique();
event.add_connection(id, socket, on_data, on_close);
```

- `on_data` runs on the main thread on each readable chunk; it returns
  `Update`, mirroring `WriteBackCallback`.
- `on_close` runs once when the connection ends — clean or not.
- `ConnectionStatus` is a frame-coherent snapshot the layout callback can
  read for status displays without locking.

This page will be promoted from `stub` to `wip` when the runtime side
lands. Until then, treat networking as "do it in a `Thread`."

## What this page does not cover

- TLS — out of scope for the framework. Use `rustls`, `native-tls`, or
  whatever HTTP client you prefer inside the worker.
- Mid-frame cancellation of in-flight DNS or TCP handshakes — `std::net`
  does not expose this. Use `socket2` or a third-party client if you need
  it.
- WebSockets, gRPC, or HTTP/2 — same answer: any blocking client works
  inside a `Thread`.
