---
slug: networking
title: Networking
language: en
canonical_slug: networking
audience: external
maturity: stub
guide_order: 270
topic_only: false
short_desc: HTTP from a callback
prerequisites: [background-tasks]
tracked_files:
  - core/src/task.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Networking

## Introduction

*Stub.* The framework ships a small blocking HTTP helper (`HttpRequestConfig`, `HttpResponse`) you can call from inside a [`Thread`](background-tasks.md). For raw sockets, async I/O, WebSockets, or anything else, do networking the same way you do any other blocking I/O: from a `Thread`.

## What's available

- `HttpRequestConfig`. A small blocking HTTP client. Configure timeouts, headers, max response size, and TLS verification, then call `http_get`, `download_bytes`, or `is_url_reachable`. The convenience constructors `http_get_default` and `download_bytes_default` skip configuration.
- `HttpResponse`. The result. Carries `status_code`, `body` (`U8Vec`), `headers`, `content_type`, `content_length`. Use `is_success`, `is_redirect`, `is_client_error`, `is_server_error`, `body_as_string` to inspect it.

The framework is intentionally runtime-agnostic. There's no built-in raw-socket type and no async runtime integration. Heavy networking belongs in a worker thread.

## Calling HTTP from a thread

Wrap an `HttpRequestConfig` call in a `Thread` callback and post the result back via `ThreadReceiveMsg::WriteBack`:

```rust,ignore
extern "C" fn http_get(
    mut initial: RefAny,
    mut sender:  ThreadSender,
    mut recv:    ThreadReceiver,
) {
    let url = match initial.downcast_ref::<String>() {
        Some(s) => s.clone(),
        None    => return,
    };

    let cfg = HttpRequestConfig::create()
        .with_timeout(10)
        .with_user_agent("my-app/1.0");

    let result = cfg.http_get(url.as_str().into());

    if let OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) = recv.recv() {
        return;
    }

    let msg = match result {
        ResultHttpResponseHttpError::Ok(resp) => ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
            refany:   RefAny::new(resp),
            callback: WriteBackCallback { cb: apply_response, ctx: OptionRefAny::None },
        }),
        ResultHttpResponseHttpError::Err(e) => ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
            refany:   RefAny::new(e),
            callback: WriteBackCallback { cb: apply_error, ctx: OptionRefAny::None },
        }),
    };
    sender.send(msg);
}
```

`apply_response` and `apply_error` run on the main thread and mutate the application's `RefAny` model the usual way. See [background-tasks](background-tasks.md) for the full `WriteBackCallback` pattern.

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

## What this page doesn't cover

- TLS configuration beyond `disable_tls_cert_verification`. For custom TLS stacks, use `rustls`, `native-tls`, or a third-party HTTP client inside the worker.
- Mid-frame cancellation of in-flight DNS or TCP handshakes. `std::net` doesn't expose this. Use `socket2` or a third-party client if you need it.
- WebSockets, gRPC, HTTP/2. Any blocking client works inside a `Thread`.

## Coming Up Next

- [Background Tasks](background-tasks.md) — Running long jobs off the layout thread
- [File Dialogs](file-dialogs.md) — Native open/save dialogs and folder pickers
- [Clipboard](clipboard.md) — Reading and writing the system clipboard
