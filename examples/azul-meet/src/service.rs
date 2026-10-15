//! The meetings service: its requests on an azul Thread (create, look up, announce, the peers list,
//! the health check, leave), their answers on the UI thread.

use super::*;

/// The callback a finished request resumes into, on the UI thread.
pub(crate) type ResumeFn = extern "C" fn(RefAny, CallbackInfo, RefAny) -> Update;

/// What AzMeet calls itself on the meeting server.
pub(crate) const USER_AGENT: &str = concat!("AzMeet/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verb {
    Get,
    Post,
    Put,
    Delete,
}

impl Verb {
    /// The verb of a request's method name.
    pub(crate) fn of(method: &str) -> Verb {
        match method {
            "POST" => Verb::Post,
            "PUT" => Verb::Put,
            "DELETE" => Verb::Delete,
            _ => Verb::Get,
        }
    }
}

/// One request to the meeting server.
pub(crate) struct HttpJob {
    pub(crate) verb: Verb,
    pub(crate) url: String,
    pub(crate) body: Vec<u8>,
    /// More headers: a signed request's (CRYPTO.md section 11).
    pub(crate) headers: Vec<(&'static str, String)>,
    pub(crate) on_result: ResumeFn,
    /// The room session the request was sent in (see `RoomSession::session`).
    pub(crate) session: u32,
    /// A room's chat request: the room, and what the request is (`on_chat_answer`).
    pub(crate) chat: Option<(String, chatroom::CallKind)>,
}

impl HttpJob {
    pub(crate) fn get(url: String, on_result: ResumeFn, session: u32) -> Self {
        HttpJob {
            verb: Verb::Get,
            url,
            body: Vec::new(),
            headers: Vec::new(),
            on_result,
            session,
            chat: None,
        }
    }

    /// `POST /rooms` with the room this side minted: its id, its invite key, its kind and times
    /// (the Worker's "links made offline" registration; CRYPTO.md section 4).
    pub(crate) fn create_room(room: &RoomSession, minting: &Minting) -> Self {
        let mut body = serde_json::json!({
            "room": minting.invite.room(),
            "invite_key": minting.invite.invite_key(),
            "kind": minting.kind.as_str(),
        });
        if let Some((starts, ends)) = &minting.times {
            body["starts_at"] = serde_json::json!(starts);
            body["ends_at"] = serde_json::json!(ends);
        }
        HttpJob {
            verb: Verb::Post,
            url: format!("{}/rooms", room.worker),
            body: body.to_string().into_bytes(),
            headers: Vec::new(),
            on_result: on_room_opened,
            session: room.session,
            chat: None,
        }
    }

    pub(crate) fn look_up(room: &RoomSession, key: &RoomKey) -> Self {
        HttpJob::get(
            format!("{}/rooms/{}?format=json", room.worker, key.as_str()),
            on_room_opened,
            room.session,
        )
    }

    /// This endpoint's ticket in the room, signed by this device (CRYPTO.md section 10): its
    /// name is the member record's, sealed, so none goes here.
    pub(crate) fn announce(room: &RoomSession, me: &crypto::Identity, ts: u64) -> Self {
        let sig = me.sign(&crypto::peer_input(&room.room_id, &room.node_id, &room.ticket));
        let body = serde_json::json!({
            "node_id": room.node_id,
            "ticket": room.ticket,
            "name": "",
            "device": me.device(),
            "sig": sig,
        })
        .to_string()
        .into_bytes();
        let path = format!("/rooms/{}/peers", room.room_id);
        let headers = Vec::from(me.request_headers("POST", &path, ts, &body));
        HttpJob {
            verb: Verb::Post,
            url: format!("{}{path}", room.worker),
            body,
            headers,
            on_result: on_announced,
            session: room.session,
            chat: None,
        }
    }

    pub(crate) fn poll(room: &RoomSession) -> Self {
        HttpJob::get(
            format!(
                "{}/rooms/{}/peers?except={}",
                room.worker, room.room_id, room.node_id
            ),
            on_peers,
            room.session,
        )
    }

    /// The waiting room's read of its meeting's peers list (this side is not announced there).
    pub(crate) fn waiting_peers(room: &RoomSession) -> Self {
        let meeting = room
            .waiting
            .as_ref()
            .map_or("", |found| found.room.as_str());
        HttpJob::get(
            format!(
                "{}/rooms/{}/peers?except={}",
                room.worker, meeting, room.node_id
            ),
            on_waiting_peers,
            room.session,
        )
    }

    /// Whether the meeting server answers (`GET /health`); the answer is matched to the check by
    /// `session`, which holds the check's number here.
    pub(crate) fn health(room: &RoomSession) -> Self {
        HttpJob::get(format!("{}/health", room.worker), on_health, room.checks)
    }

    /// Takes this endpoint off the room's list (Leave), signed by the device that announced it.
    pub(crate) fn leave(room: &RoomSession, me: &crypto::Identity, ts: u64) -> Self {
        let path = format!("/rooms/{}/peers/{}", room.room_id, room.node_id);
        HttpJob {
            verb: Verb::Delete,
            url: format!("{}{path}", room.worker),
            body: Vec::new(),
            headers: Vec::from(me.request_headers("DELETE", &path, ts, b"")),
            on_result: on_left,
            session: room.session,
            chat: None,
        }
    }

    /// A room's chat request (`chatroom::Call`), signed when it changes something.
    pub(crate) fn chat(
        chat: &chatroom::ChatRoom,
        me: &crypto::Identity,
        call: chatroom::Call,
        now_ms: u64,
    ) -> Self {
        let headers = if call.signed {
            let ts = chat.server_now(now_ms);
            Vec::from(me.request_headers(call.method, &call.path, ts, &call.body))
        } else {
            Vec::new()
        };
        HttpJob {
            verb: Verb::of(call.method),
            url: call.url(&chat.server),
            body: call.body,
            headers,
            on_result: on_chat_answer,
            session: 0,
            chat: Some((chat.room.clone(), call.what)),
        }
    }
}

pub(crate) struct HttpThreadInit {
    pub(crate) job: HttpJob,
    /// The participant's `MeetState`, handed back to `job.on_result` in a `Reply`.
    pub(crate) app: RefAny,
}

/// What a finished request resumes with: the participant's `MeetState`, the room session the
/// request was sent in, and for a chat request its room and what it was.
pub(crate) struct Reply {
    pub(crate) app: RefAny,
    pub(crate) session: u32,
    pub(crate) chat: Option<(String, chatroom::CallKind)>,
}

/// The participant and the session of a resumed request.
pub(crate) fn reply_parts(mut data: RefAny) -> Option<(RefAny, u32)> {
    let reply = data.downcast_ref::<Reply>()?;
    Some((reply.app.clone(), reply.session))
}

/// The participant, the room and what the request was, of a resumed chat request.
pub(crate) fn reply_chat(mut data: RefAny) -> Option<(RefAny, String, chatroom::CallKind)> {
    let reply = data.downcast_ref::<Reply>()?;
    let (room, what) = reply.chat.clone()?;
    Some((reply.app.clone(), room, what))
}

/// Runs one request on a worker thread. `http_request` blocks here, then queues its answer,
/// which the UI thread delivers to `on_result` on its next pump (the 15 ms link timer).
pub(crate) extern "C" fn http_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((verb, url, body, headers, on_result, reply)) =
        init.downcast_ref::<HttpThreadInit>().map(|i| {
            (
                i.job.verb,
                i.job.url.clone(),
                i.job.body.clone(),
                i.job.headers.clone(),
                i.job.on_result,
                Reply {
                    app: i.app.clone(),
                    session: i.job.session,
                    chat: i.job.chat.clone(),
                },
            )
        })
    else {
        return;
    };
    let method = match verb {
        Verb::Get => HttpMethod::Get,
        Verb::Post => HttpMethod::Post,
        Verb::Put => HttpMethod::Put,
        Verb::Delete => HttpMethod::Delete,
    };
    let mut config = HttpRequestConfig::create()
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_user_agent(USER_AGENT)
        .with_header("accept", "application/json");
    for (name, value) in headers {
        config = config.with_header(name, value);
    }
    let _request = config.http_request(
        method,
        url.as_str(),
        U8Vec::from(body),
        "application/json",
        RefAny::new(reply),
        on_result,
    );
}

pub(crate) fn spawn_http(info: &mut CallbackInfo, app: RefAny, job: HttpJob) {
    let init = RefAny::new(HttpThreadInit { job, app });
    info.add_thread(
        ThreadId::unique(),
        Thread::create(init, RefAny::new(()), http_thread),
    );
}

pub(crate) fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => t("azmeet-timed-out"),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The status and JSON body of a finished request, or why there is none.
pub(crate) fn http_answer(result: RefAny) -> Result<(u16, Option<Json>), String> {
    let answer = HttpGetResult::downcast(result)
        .into_option()
        .ok_or_else(|| t("azmeet-no-answer"))?;
    let response = answer
        .result
        .into_result()
        .map_err(|e| http_error_text(&e))?;
    let json = response
        .body_as_string()
        .into_option()
        .and_then(|body| Json::parse(body).into_result().ok());
    Ok((response.status_code, json))
}

/// The status and the body text of a finished request; no status when nothing answered.
pub(crate) fn http_reply(result: RefAny) -> (Option<u16>, String) {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return (None, String::new());
    };
    match answer.result.into_result() {
        Ok(response) => (
            Some(response.status_code),
            response
                .body_as_string()
                .into_option()
                .map(|body| body.as_str().to_string())
                .unwrap_or_default(),
        ),
        Err(_) => (None, String::new()),
    }
}

pub(crate) fn json_text(json: &Json, key: &str) -> Option<String> {
    let value = json.get_key(key).into_option()?;
    let text = value.as_string().into_option()?;
    Some(text.as_str().to_string())
}

/// What `POST /rooms` and `GET /rooms/<key>` answer, and the invite this side holds for it.
#[derive(Clone)]
pub(crate) struct RoomInfo {
    pub(crate) room: String,
    pub(crate) code: String,
    /// The Worker's app link (`azlin://meet/<room>`), without a fragment.
    pub(crate) link: String,
    pub(crate) kind: chatroom::RoomKind,
    /// The meeting's times, seconds since 1970.
    pub(crate) starts_at: Option<u64>,
    pub(crate) ends_at: Option<u64>,
    /// The invite key the room was registered with; `None`: not an encrypted room.
    pub(crate) invite_key: Option<String>,
    /// The invite of the link this side holds; `None`: joined with the code (a knock).
    pub(crate) invite: Option<crypto::Invite>,
}

impl RoomInfo {
    /// The link others join with: with the invite secret when this side holds it.
    pub(crate) fn share_link(&self) -> String {
        rooms::link_with_secret(&self.link, self.invite.as_ref().map(crypto::Invite::secret))
    }
}

pub(crate) fn room_info(json: &Json) -> Option<RoomInfo> {
    let room = json_text(json, "room")?;
    let time = |key: &str| {
        json_text(json, key)
            .as_deref()
            .and_then(azul_storage::time::parse_iso8601)
    };
    Some(RoomInfo {
        code: json_text(json, "code").unwrap_or_default(),
        link: json_text(json, "link")
            .unwrap_or_else(|| format!("{}{room}", rooms::APP_LINK_PREFIX)),
        kind: chatroom::RoomKind::parse(json_text(json, "kind").as_deref()),
        starts_at: time("starts_at"),
        ends_at: time("ends_at"),
        invite_key: json_text(json, "invite_key").filter(|k| crypto::is_device(k)),
        invite: None,
        room,
    })
}

pub(crate) fn peers_from(json: &Json) -> Vec<PeerRecord> {
    let Some(list) = json.get_key("peers").into_option() else {
        return Vec::new();
    };
    (0..list.len())
        .filter_map(|i| list.get_index(i).into_option())
        .filter_map(|p| {
            Some(PeerRecord {
                node_id: json_text(&p, "node_id")?,
                ticket: json_text(&p, "ticket")?,
                name: json_text(&p, "name").unwrap_or_default(),
                device: json_text(&p, "device"),
                sig: json_text(&p, "sig"),
            })
        })
        .collect()
}

/// The peers of `listed` whose announcement a member of `chat` signed, each with that member's
/// name (CRYPTO.md section 10); every other is left out, and logged once per read.
pub(crate) fn verified_peers(
    chat: Option<&chatroom::ChatRoom>,
    listed: Vec<PeerRecord>,
    me: &str,
) -> Vec<PeerRecord> {
    let Some(chat) = chat else {
        return Vec::new();
    };
    listed
        .into_iter()
        .filter_map(|p| {
            let member =
                chat.verify_peer(&p.node_id, &p.ticket, p.device.as_deref(), p.sig.as_deref());
            match member {
                Some(m) => Some(PeerRecord {
                    name: member_name(&m.name),
                    ..p
                }),
                None => {
                    if p.node_id != me {
                        eprintln!(
                            "[azmeet] left out {}: no member signed its announcement",
                            short_id(&p.node_id)
                        );
                    }
                    None
                }
            }
        })
        .collect()
}

/// What to tell the user about a failed request.
pub(crate) fn server_trouble(worker: &str, answer: &Result<(u16, Option<Json>), String>) -> String {
    match answer {
        Ok((429, _)) => t("azmeet-too-many-attempts"),
        Ok((status, json)) => {
            let message = json
                .as_ref()
                .and_then(|j| json_text(j, "message"))
                .unwrap_or_default();
            t_args(
                "azmeet-server-answered",
                &[("status", Arg::from(status.to_string())), ("message", Arg::from(message))],
            )
        }
        Err(e) => t_args("azmeet-server-unreachable", &[("server", Arg::from(worker)), ("why", Arg::from(e))]),
    }
}

/// What the header says when the meetings service failed a request: that it does not answer,
/// or what it answered. Never its address or the network's reason, which name the address too:
/// Settings > Meetings shows those ([`server_trouble`]).
pub(crate) fn service_notice(answer: &Result<(u16, Option<Json>), String>) -> String {
    match answer {
        Err(_) => t("azmeet-service-down"),
        answered => server_trouble("", answered),
    }
}

/// The header of a waiting room shown with no meetings service answering (`--screen waiting`).
pub(crate) fn preview_notice(room: &RoomSession) -> String {
    t(if room.worker.is_empty() {
        "azmeet-preview-no-server"
    } else {
        "azmeet-preview-silent-server"
    })
}

/// What "Join now" says in a waiting room of a meeting made here: nobody can join it.
pub(crate) fn cannot_join_notice(_room: &RoomSession) -> String {
    t("azmeet-preview-cannot-join")
}

/// The room is gone from the server: back to the start screen, unless people are connected.
pub(crate) fn meeting_gone(s: &mut MeetState) {
    let connected = !s.remotes.is_empty();
    if let Some(room) = s.room.as_mut() {
        if connected {
            room.stage = Stage::Ended;
            room.busy = false;
        } else {
            room.leave();
        }
    }
    s.notice = t(if connected {
        "azmeet-link-expired"
    } else {
        "azmeet-meeting-ended"
    });
}

/// The answer to `POST /rooms` (a new meeting) or `GET /rooms/<key>` (joining a link): the
/// waiting room shows the meeting (a script's `AZMEET_JOIN` / `AZMEET_AUTOCREATE` enters it at
/// once).
/// `found` checked against what this side knows of it (CRYPTO.md section 4): a room this side
/// minted must come back under the id it made, a link's secret must derive the key the room was
/// registered with, and a room without an invite key (an older AzMeet's, not encrypted) is
/// refused. The invite this side holds goes with it; a code holds none (a knock).
pub(crate) fn check_room(
    mut found: RoomInfo,
    minting: Option<Minting>,
    secret: Option<String>,
) -> Result<RoomInfo, String> {
    let Some(key) = found.invite_key.clone() else {
        return Err(t("azmeet-meeting-not-encrypted"));
    };
    let invite = match (minting, secret) {
        (Some(minting), _) => Some(minting.invite),
        (None, Some(secret)) => {
            Some(crypto::Invite::new(&found.room, &secret).ok_or_else(|| t("azmeet-secret-damaged"))?)
        }
        (None, None) => None,
    };
    if let Some(invite) = &invite {
        if invite.room() != found.room || invite.invite_key() != key {
            return Err(t("azmeet-room-mismatch"));
        }
    }
    found.invite = invite;
    Ok(found)
}

/// The chat room of `found` on this device: made when it is not here yet (with the invite of the
/// link, this device's name), its code, kind and times as the meeting server said.
pub(crate) fn ensure_chat(s: &mut MeetState, found: &RoomInfo) {
    let worker = s.room.as_ref().map(|r| r.worker.clone()).unwrap_or_default();
    let name = s.name.clone();
    let chat = s.chats.entry(found.room.clone()).or_insert_with(|| {
        chatroom::ChatRoom::new(
            &worker,
            &found.room,
            &found.code,
            found.kind,
            found.invite.clone(),
            &name,
        )
    });
    chat.kind = found.kind;
    chat.starts_at = found.starts_at;
    chat.ends_at = found.ends_at;
    if !found.code.is_empty() {
        chat.code = found.code.clone();
    }
}

/// `meet/rooms.json` learns what this device knows of room `id` now: kept on the next
/// `flush_files` when it changed.
pub(crate) fn remember_room(s: &mut MeetState, id: &str) {
    let Some(me) = s.identity.as_ref() else {
        return;
    };
    let Some(chat) = s.chats.get(id) else {
        return;
    };
    if !matches!(
        chat.state,
        chatroom::Membership::Member
            | chatroom::Membership::Knocking
            | chatroom::Membership::Joining
    ) {
        return;
    }
    let existing = s.index.get(me.device(), id).cloned();
    // The invite secret is sealed once: a new seal (a new nonce) would rewrite the file each time.
    let invite = existing
        .as_ref()
        .and_then(|e| e.invite.clone())
        .or_else(|| chat.invite().and_then(|i| me.seal_local(id, i.secret()).ok()));
    let entry = roomlist::RoomEntry {
        room: id.to_string(),
        code: chat.code.clone(),
        kind: chat.kind.as_str().to_string(),
        server: chat.server.clone(),
        starts_at: chat.starts_at,
        ends_at: chat.ends_at,
        device: me.device().to_string(),
        invite,
        member: chat.state != chatroom::Membership::Knocking,
        read_seq: chat.read_seq,
        departed: chat.departed().clone(),
        joined: existing.map_or_else(azul_storage::time::now_unix, |e| e.joined),
    };
    if s.index.upsert(entry) {
        s.unsaved.index = true;
    }
}

/// The meeting `found` was opened: a chat room opens in the room view (joined), a meeting in its
/// waiting room - or at once for a script (`AZMEET_JOIN`, `AZMEET_AUTOCREATE`). The request to
/// send next, if any.
pub(crate) fn open_found(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    found: RoomInfo,
) -> Option<HttpJob> {
    ensure_chat(s, &found);
    if found.kind == chatroom::RoomKind::Chat {
        if let Some(room) = s.room.as_mut() {
            room.stage = Stage::Start;
            room.created = false;
        }
        println!("AZMEET_LINK {}", found.share_link());
        if !found.code.is_empty() {
            println!("AZMEET_CODE {}", found.code);
        }
        open_room_view(s, &found.room, true);
        return None;
    }
    let straight_in = s.room.as_ref().is_some_and(|room| room.straight_in);
    if straight_in {
        apply_join_defaults(s);
        return enter_room(s, info, app, found);
    }
    // For scripts: the waiting room is up, for this link.
    println!("AZMEET_WAITING {}", found.share_link());
    if let (Some(a), Some(b)) = (found.starts_at, found.ends_at) {
        println!(
            "AZMEET_TIMES {} {}",
            azul_storage::time::iso8601(a),
            azul_storage::time::iso8601(b)
        );
    }
    eprintln!(
        "[azmeet] {}: in the waiting room of meeting {} ({}){}",
        s.name,
        found.code,
        found.room,
        if found.invite.is_none() {
            ", with its code: a member lets this device in"
        } else {
            ""
        }
    );
    let room = s.room.as_mut()?;
    room.stage = Stage::Waiting;
    room.copied = false;
    room.waiting = Some(found);
    room.close_waiting();
    // Who is in the meeting already: asked now, then every poll while waiting.
    let job = room.waiting_job();
    s.notice.clear();
    apply_join_defaults(s);
    job
}

/// The answer to `POST /rooms` (a room this side minted) or `GET /rooms/<key>` (a link or a code
/// looked up): checked against the link (`check_room`), then opened (`open_found`).
pub(crate) extern "C" fn on_room_opened(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let app = data.clone();
    let follow_up = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.session != session || room.stage != Stage::Opening {
            return Update::DoNothing;
        }
        room.busy = false;
        let minting = room.minting.take();
        let making_a_meeting = minting
            .as_ref()
            .is_some_and(|m| m.kind == chatroom::RoomKind::Meeting);
        let secret = room.join_secret.take();
        let found = match &answer {
            Ok((200 | 201, Some(json))) => room_info(json),
            _ => None,
        };
        match found.map(|found| check_room(found, minting, secret)) {
            Some(Ok(found)) => open_found(s, &mut info, &app, found),
            Some(Err(why)) => {
                if let Some(room) = s.room.as_mut() {
                    room.stage = Stage::Start;
                }
                eprintln!("[azmeet] {}: {why}", s.name);
                s.notice = why;
                None
            }
            // A meeting the service did not make (it is down, or answered an error): made
            // here, so the waiting room and its camera still open.
            None if making_a_meeting && !matches!(&answer, Ok((200 | 201 | 404, _))) => {
                let notice = service_notice(&answer);
                let _ = open_local_meeting(s, notice);
                None
            }
            None => {
                if let Some(room) = s.room.as_mut() {
                    room.stage = Stage::Start;
                }
                s.notice = match &answer {
                    Ok((404, _)) => t("azmeet-meeting-ended"),
                    Ok((200 | 201, _)) => t("azmeet-answer-unreadable"),
                    other => service_notice(other),
                };
                None
            }
        }
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The answer to an announcement; a successful one is followed by a read of the peers list, and
/// says how long the Worker keeps a ticket (the next announcement comes after a sixth of that).
pub(crate) extern "C" fn on_announced(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let (follow_up, update) = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.session != session || room.stage != Stage::InRoom {
            // An answer from a meeting since left.
            return Update::DoNothing;
        }
        room.busy = false;
        match &answer {
            Ok((200, json)) => {
                let ttl = json
                    .as_ref()
                    .and_then(|j| j.get_key("peer_ttl_seconds").into_option())
                    .and_then(|v| {
                        v.as_int()
                            .into_option()
                            .map(|n| n as f64)
                            .or_else(|| v.as_float().into_option())
                    })
                    .filter(|ttl| *ttl >= 1.0);
                if let Some(ttl) = ttl {
                    room.reannounce_polls = rooms::reannounce_polls(ttl, ROOM_POLL_MS);
                }
                let update = if room.trouble {
                    room.trouble = false;
                    s.notice.clear();
                    Update::RefreshDom
                } else {
                    Update::DoNothing
                };
                room.busy = true;
                room.busy_polls = 0;
                (Some(HttpJob::poll(room)), update)
            }
            Ok((404, _)) => {
                meeting_gone(s);
                (None, Update::RefreshDom)
            }
            other => {
                room.announced_at = None;
                room.trouble = true;
                s.notice = service_notice(other);
                (None, Update::RefreshDom)
            }
        }
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, data.clone(), job);
    }
    update
}

/// The peers list: only the announcements a member's device signed count (CRYPTO.md section 10);
/// dial who this side should dial, and let in the connections that waited for theirs.
pub(crate) extern "C" fn on_peers(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    if room.session != session || room.stage != Stage::InRoom {
        // An answer from a meeting since left.
        return Update::DoNothing;
    }
    room.busy = false;
    match answer {
        Ok((200, Some(json))) => {
            let me = room.node_id.clone();
            let listed = verified_peers(s.chats.get(&room.room_id), peers_from(&json), &me);
            let diff = rooms::diff_peers(&room.peers, &listed, &room.node_id);
            for p in &diff.joined {
                eprintln!(
                    "[azmeet] {}: {} joined ({})",
                    s.name,
                    p.name,
                    short_id(&p.node_id)
                );
            }
            for p in &diff.left {
                eprintln!("[azmeet] {}: {} left", s.name, p.name);
            }
            let connected: BTreeSet<String> = s.remotes.iter().map(|r| r.node_id.clone()).collect();
            let plan =
                rooms::plan_dials(&room.node_id, &listed, &connected, &room.dialed, room.polls);
            if let Some(endpoint) = s.endpoint.as_ref() {
                for p in &plan {
                    eprintln!(
                        "[azmeet] {}: dialing {} ({})",
                        s.name,
                        p.name,
                        short_id(&p.node_id)
                    );
                    let _dialing = endpoint.connect(p.ticket.as_str());
                    room.dialed.insert(
                        p.node_id.clone(),
                        Dialed {
                            ticket: p.ticket.clone(),
                            at_poll: room.polls,
                        },
                    );
                }
            }
            let mut changed = !diff.is_empty() || !plan.is_empty();
            room.peers = listed;
            if room.trouble {
                room.trouble = false;
                s.notice.clear();
                changed = true;
            }
            changed |= settle_pending(s);
            if changed {
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        Ok((404, _)) => {
            meeting_gone(s);
            Update::RefreshDom
        }
        other => {
            room.trouble = true;
            s.notice = service_notice(&other);
            Update::RefreshDom
        }
    }
}

/// The waiting room's read of its meeting's peers list: who is in the call already ("Ada is in
/// this meeting"), by the names of the members whose devices signed the announcements (a device
/// with only the code reads no names: "Someone"). An answer for a waiting room since left is
/// ignored; a meeting that ended says so.
pub(crate) extern "C" fn on_waiting_peers(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    if room.session != session || room.stage != Stage::Waiting {
        return Update::DoNothing;
    }
    room.waiting_busy = false;
    match answer {
        Ok((200, Some(json))) => {
            let meeting = room.waiting.as_ref().map(|found| found.room.clone()).unwrap_or_default();
            let me = room.node_id.clone();
            let names: Vec<String> = verified_peers(s.chats.get(&meeting), peers_from(&json), &me)
                .into_iter()
                .filter(|p| p.node_id != me)
                .map(|p| p.name)
                .collect();
            if room.waiting_people.as_ref() == Some(&names) {
                return Update::DoNothing;
            }
            eprintln!(
                "[azmeet] {}: in the waiting room: {}",
                s.name,
                ui::who_is_here(&names)
            );
            room.waiting_people = Some(names);
            Update::RefreshDom
        }
        Ok((404, _)) => {
            s.notice = t("azmeet-meeting-ended");
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    }
}
