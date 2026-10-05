//! The work that waits on the network, each on an azul `Thread` (never in a callback):
//!
//! - **refresh**: every feed asked in turn with its validators ([`crate::fetch::fetch`]); each
//!   answer is posted to the UI thread as it comes ([`RefreshEvent::Fetched`]), so the list
//!   fills feed by feed; [`RefreshEvent::Done`] at the end;
//! - **pictures**: the pictures of the open article (`Xml::scan_external_resources` found them,
//!   [`crate::reader::Article::images`]) downloaded and decoded off the UI thread
//!   ([`PictureEvent`]); the UI puts each into azul's image cache under its address, where the
//!   article's `<img src>` finds it;
//! - **find** (Add feed): [`crate::fetch::find_feeds`] ([`FindEvent`]).
//!
//! Requests go through azul-storage's `AzulTransport` (azul's `HttpRequestConfig`); the
//! User-Agent is [`crate::fetch::USER_AGENT`] (no personal data). A thread stops between two
//! requests when the window asks it to (`ThreadSendMsg::TerminateThread`).

use azul::{
    callbacks::WriteBackCallbackType,
    error::ResultRawImageDecodeImageError,
    image::RawImage,
    option::OptionThreadSendMsg,
    prelude::*,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender,
        ThreadWriteBackMsg,
    },
    vec::U8VecRef,
};
use azul_storage::{azul_transport::AzulTransport, HttpCall, Method, Transport};

use crate::fetch::{self, Candidate, Fetched};

/// Seconds one feed may take.
pub const FEED_TIMEOUT_SECS: u64 = 30;
/// Seconds one picture may take.
pub const PICTURE_TIMEOUT_SECS: u64 = 20;
/// The largest picture downloaded (bytes).
pub const MAX_PICTURE_BYTES: usize = 12 * 1024 * 1024;
/// The largest side a picture is kept at (pixels): larger ones are scaled down once.
pub const MAX_PICTURE_PX: u32 = 1600;
/// The most pictures one article loads.
pub const MAX_PICTURES: usize = 40;

/// One feed to refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshJob {
    pub id: String,
    pub url: String,
    pub etag: String,
    pub last_modified: String,
}

/// What the refresh thread reports.
#[derive(Debug, Clone)]
pub enum RefreshEvent {
    Fetched {
        id: String,
        fetched: Fetched,
        now: i64,
    },
    Done,
}

/// A picture of the open article, downloaded and decoded (`image: None` when it could not be:
/// `error` says why).
pub struct PictureEvent {
    pub url: String,
    pub image: Option<RawImage>,
    pub error: String,
}

/// What "Find" found for the address the user typed.
#[derive(Debug, Clone)]
pub struct FindEvent {
    pub input: String,
    pub result: Result<Vec<Candidate>, String>,
}

/// A message on its way to the UI thread (taken out once).
struct Message<T> {
    inner: Option<T>,
}

/// Takes the message of a thread's write-back (`None`: not one of type `T`, or taken already).
#[must_use]
pub fn take<T: 'static>(msg: &mut RefAny) -> Option<T> {
    msg.downcast_mut::<Message<T>>()
        .and_then(|mut m| m.inner.take())
}

/// Posts `event` to the UI thread; `false` when the window is gone.
fn post<T: 'static>(sender: &mut ThreadSender, on_event: WriteBackCallbackType, event: T) -> bool {
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_event,
        RefAny::new(Message { inner: Some(event) }),
    )))
}

/// Whether the window asked the thread to stop (reads every message waiting).
fn stop_asked(receiver: &mut ThreadReceiver) -> bool {
    let mut stop = false;
    while let OptionThreadSendMsg::Some(message) = receiver.recv() {
        if matches!(message, ThreadSendMsg::TerminateThread) {
            stop = true;
        }
    }
    stop
}

fn now() -> i64 {
    i64::try_from(azul_storage::time::now_unix()).unwrap_or(0)
}

// ==== Refresh ====

struct RefreshInit {
    jobs: Option<Vec<RefreshJob>>,
    on_event: WriteBackCallbackType,
}

/// Refreshes `jobs` on a Thread; `on_event(reply_to, msg, info)` gets each [`RefreshEvent`]
/// (take it with [`take`]). Nothing happens for no jobs.
pub fn spawn_refresh(
    info: &mut CallbackInfo,
    jobs: Vec<RefreshJob>,
    reply_to: RefAny,
    on_event: WriteBackCallbackType,
) {
    if jobs.is_empty() {
        return;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(RefreshInit {
                jobs: Some(jobs),
                on_event,
            }),
            reply_to,
            refresh_thread,
        ),
    );
}

extern "C" fn refresh_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((jobs, on_event)) = init.downcast_mut::<RefreshInit>().and_then(|mut i| {
        let jobs = i.jobs.take()?;
        Some((jobs, i.on_event))
    }) else {
        return;
    };
    let transport = AzulTransport::new(fetch::USER_AGENT).with_timeout(FEED_TIMEOUT_SECS);
    for job in jobs {
        if stop_asked(&mut receiver) {
            break;
        }
        let fetched = fetch::fetch(&transport, &job.url, &job.etag, &job.last_modified);
        let event = RefreshEvent::Fetched {
            id: job.id,
            fetched,
            now: now(),
        };
        if !post(&mut sender, on_event, event) {
            return;
        }
    }
    let _delivered = post(&mut sender, on_event, RefreshEvent::Done);
}

// ==== Pictures ====

struct PictureInit {
    urls: Option<Vec<String>>,
    on_event: WriteBackCallbackType,
}

/// Downloads and decodes `urls` (at most [`MAX_PICTURES`]) on a Thread; `on_event` gets a
/// [`PictureEvent`] per picture.
pub fn spawn_pictures(
    info: &mut CallbackInfo,
    urls: Vec<String>,
    reply_to: RefAny,
    on_event: WriteBackCallbackType,
) {
    if urls.is_empty() {
        return;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(PictureInit {
                urls: Some(urls.into_iter().take(MAX_PICTURES).collect()),
                on_event,
            }),
            reply_to,
            picture_thread,
        ),
    );
}

/// A picture no larger than [`MAX_PICTURE_PX`] on either side.
fn fit(raw: RawImage) -> RawImage {
    let max = MAX_PICTURE_PX as usize;
    if raw.width > max || raw.height > max {
        match raw.thumbnail(MAX_PICTURE_PX, MAX_PICTURE_PX).into_option() {
            Some(small) => small,
            None => raw,
        }
    } else {
        raw
    }
}

/// One picture: downloaded, decoded, fitted.
fn picture(transport: &dyn Transport, url: String) -> PictureEvent {
    let call = HttpCall {
        method: Method::Get,
        url: url.clone(),
        headers: vec![("Accept".to_string(), "image/*".to_string())],
        body: Vec::new(),
        content_type: String::new(),
    };
    match transport.send(&call) {
        Ok(reply) if reply.is_success() && reply.body.len() <= MAX_PICTURE_BYTES => {
            match RawImage::decode_image_bytes_any(U8VecRef::from(reply.body.as_slice())) {
                ResultRawImageDecodeImageError::Ok(raw) => PictureEvent {
                    url,
                    image: Some(fit(raw)),
                    error: String::new(),
                },
                ResultRawImageDecodeImageError::Err(e) => PictureEvent {
                    url,
                    image: None,
                    error: format!("not a picture azul can show: {e:?}"),
                },
            }
        }
        Ok(reply) => PictureEvent {
            url,
            image: None,
            error: format!("HTTP {} ({} bytes)", reply.status, reply.body.len()),
        },
        Err(error) => PictureEvent {
            url,
            image: None,
            error,
        },
    }
}

extern "C" fn picture_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((urls, on_event)) = init.downcast_mut::<PictureInit>().and_then(|mut i| {
        let urls = i.urls.take()?;
        Some((urls, i.on_event))
    }) else {
        return;
    };
    let transport = AzulTransport::new(fetch::USER_AGENT).with_timeout(PICTURE_TIMEOUT_SECS);
    for url in urls {
        if stop_asked(&mut receiver) {
            return;
        }
        if !post(&mut sender, on_event, picture(&transport, url)) {
            return;
        }
    }
}

// ==== Find ====

struct FindInit {
    input: Option<String>,
    on_event: WriteBackCallbackType,
}

/// Looks for the feeds of what the user typed on a Thread; `on_event` gets one [`FindEvent`].
pub fn spawn_find(
    info: &mut CallbackInfo,
    input: String,
    reply_to: RefAny,
    on_event: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(FindInit {
                input: Some(input),
                on_event,
            }),
            reply_to,
            find_thread,
        ),
    );
}

extern "C" fn find_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((input, on_event)) = init.downcast_mut::<FindInit>().and_then(|mut i| {
        let input = i.input.take()?;
        Some((input, i.on_event))
    }) else {
        return;
    };
    let transport = AzulTransport::new(fetch::USER_AGENT).with_timeout(FEED_TIMEOUT_SECS);
    let result = fetch::find_feeds(&transport, &input);
    let _delivered = post(&mut sender, on_event, FindEvent { input, result });
}
