//! The data tree (`store.rs`): the settings, each meeting's record and chat in its folder, the room
//! list, written through azul-appkit's file jobs.

use super::*;

/// The data root this run keeps its files in: azul-appkit's (`--data-dir`, `AZLIN_DATA`, else
/// `<data dir>/Azlin`). `None` in a headless run that was given none: a test never touches the
/// user's files.
pub(crate) fn files_root() -> Option<&'static std::path::Path> {
    static ROOT: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let flag = launch_args().kit.data_dir.as_deref();
        let given = flag.is_some() || std::env::var_os(azul_appkit::data::DATA_VAR).is_some();
        (devices_allowed() || given).then(|| store::data_root(flag))
    })
    .as_deref()
}

/// `meet/settings.json` as it was at start (the defaults without one), read once before any
/// window opens: what decides the name, the meeting server and the look before a kit exists.
pub(crate) fn saved_settings() -> &'static azul_appkit::AppSettings {
    static SAVED: std::sync::OnceLock<azul_appkit::AppSettings> = std::sync::OnceLock::new();
    SAVED.get_or_init(|| files_root().map(store::load_settings).unwrap_or_default())
}

/// A participant's azul-appkit kit: the settings page and `meet/settings.json` (the kit reads it
/// now and writes it on every change). A headless run without a data root of its own
/// (`files_root` is `None`) gives the kit an empty folder of this process in the temp folder, so
/// a test never reads or writes the user's settings.
pub(crate) fn make_kit() -> RefAny {
    let mut kit_args = launch_args().kit.clone();
    if files_root().is_none() {
        let scratch = std::env::temp_dir().join(format!("azmeet-{}", std::process::id()));
        kit_args.data_dir = Some(scratch);
    }
    kit::create_kit(
        args::SPEC,
        ABOUT,
        &keys::SHORTCUTS,
        &ui::APP_CATEGORIES,
        kit_args,
    )
}

/// What the kit's settings remember (`store::Prefs`).
pub(crate) fn kit_prefs(kit_ref: &RefAny) -> store::Prefs {
    let mut kit_ref = kit_ref.clone();
    kit_ref
        .downcast_ref::<kit::Kit>()
        .map(|k| store::Prefs::read(&k.settings))
        .unwrap_or_default()
}

/// Changes what AzMeet remembers besides the theme and the mode (`store::Prefs`) in the kit's
/// settings - the one copy, which the kit's own Appearance writes too; [`save_prefs`] writes it.
pub(crate) fn remember(s: &mut MeetState, change: impl FnOnce(&mut store::Prefs)) {
    let changed = {
        let mut kit_ref = s.kit.clone();
        let Some(mut k) = kit_ref.downcast_mut::<kit::Kit>() else {
            return;
        };
        let mut prefs = store::Prefs::read(&k.settings);
        let before = prefs.clone();
        change(&mut prefs);
        if prefs != before {
            prefs.write(&mut k.settings);
        }
        prefs != before
    };
    if changed {
        s.prefs_unsaved = true;
    }
}

/// Has the kit write `meet/settings.json` on its file thread (`AZMEET_SETTINGS_SAVED`) when
/// [`remember`] changed something since the last time. Not in a headless run without a data root
/// of its own (`files_root` is `None`): its kit only remembers.
pub(crate) fn save_prefs(s: &mut MeetState, info: &mut CallbackInfo) {
    if !std::mem::take(&mut s.prefs_unsaved) {
        return;
    }
    if files_root().is_some() {
        kit::save_settings(&s.kit, info);
    }
}

/// The folder name of the meeting this side is in: the meeting server's room id.
pub(crate) fn meeting_name(s: &MeetState) -> String {
    s.room
        .as_ref()
        .map(|room| room.room_id.clone())
        .unwrap_or_default()
}

/// The record of the meeting just entered (`meeting.json`): this side first. Its link is the bare
/// one: the invite secret stays out of the plain files (`meet/rooms.json` keeps it sealed).
pub(crate) fn enter_record(s: &mut MeetState) {
    let (link, server) = match &s.room {
        Some(room) => (
            rooms::link_with_secret(&room.link, None),
            room.worker.clone(),
        ),
        None => (String::new(), String::new()),
    };
    s.record = Some(store::MeetingRecord {
        meeting: meeting_name(s),
        link,
        server,
        joined: azul_storage::time::now_unix(),
        people: vec![s.name.clone()],
    });
    s.unsaved.record = true;
}

/// Everyone met in this meeting goes into its record, once, by their member's name (a peer whose
/// announcement no member signed is not in the call).
pub(crate) fn note_people(s: &mut MeetState) {
    let names: Vec<String> = match &s.room {
        Some(room) => s
            .remotes
            .iter()
            .filter_map(|r| {
                room.peers
                    .iter()
                    .find(|p| p.node_id == r.node_id)
                    .map(|p| p.name.clone())
            })
            .collect(),
        None => Vec::new(),
    };
    let Some(record) = s.record.as_mut() else {
        return;
    };
    for name in names {
        if record.met(&name) {
            s.unsaved.record = true;
        }
    }
}

/// Writes what changed since the last call into the data tree, on the save thread (never here):
/// the meeting's record, each changed room's chat, the room list (the settings are the kit's:
/// [`save_prefs`]). Nothing in a run without a data root. Called on every pump: without a change
/// it returns at once.
pub(crate) fn flush_files(s: &mut MeetState, info: &mut CallbackInfo) {
    let mut unsaved = std::mem::take(&mut s.unsaved);
    let meeting = meeting_name(s);
    // The rooms whose earlier visit's files are on their way back: written once they are in
    // (`on_history_read`), never the new ones over them.
    if unsaved.record && s.reading_history.contains(&meeting) {
        s.unsaved.record = true;
        unsaved.record = false;
    }
    let reading: Vec<String> = unsaved
        .chats
        .iter()
        .filter(|id| s.reading_history.contains(*id))
        .cloned()
        .collect();
    for id in reading {
        unsaved.chats.remove(&id);
        s.unsaved.chats.insert(id);
    }
    if !unsaved.record && unsaved.chats.is_empty() && !unsaved.index {
        return;
    }
    let Some(root) = files_root() else {
        return;
    };
    let mut files = Vec::new();
    if unsaved.record {
        if let (Some(key), Some(record)) = (store::meeting_key(&meeting), s.record.as_ref()) {
            files.push((key, record.to_json().into_bytes()));
        }
    }
    for id in &unsaved.chats {
        if let (Some(key), Some(chat)) = (store::chat_key(id), s.chats.get(id)) {
            files.push((key, store::chat_lines(chat.messages()).into_bytes()));
        }
    }
    if unsaved.index {
        files.push((store::index_key(), s.index.to_json().into_bytes()));
    }
    store::save(info, root, files);
}

/// Reads the files an earlier visit to room `room` left (`meet/<room>/chat.jsonl` and
/// `meeting.json`) on a Thread; `on_history_read` lists that chat in its places.
pub(crate) fn read_history(s: &mut MeetState, info: &mut CallbackInfo, app: RefAny, room: &str) {
    let Some(root) = files_root() else {
        return;
    };
    if !s.reading_history.insert(room.to_string()) {
        return;
    }
    store::read_meeting(info, root, room, app, on_history_read);
}

/// The earlier visit's files are in: its chat is listed in its places (`AZMEET_CHAT_RESTORED
/// <n>`, the messages the file held; one the meeting server still has is listed once), the
/// people it met join the record; then the files are written whole.
pub(crate) extern "C" fn on_history_read(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((meeting, chat, record)) = msg
        .downcast_ref::<store::Earlier>()
        .map(|e| (e.meeting.clone(), e.chat.clone(), e.record.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.reading_history.remove(&meeting);
    let me = s
        .identity
        .as_ref()
        .map(|i| i.device().to_string())
        .unwrap_or_default();
    let earlier = chat
        .map(|text| store::parse_chat(&text, &me))
        .unwrap_or_default();
    if !earlier.is_empty() {
        println!("AZMEET_CHAT_RESTORED {}", earlier.len());
        if let Some(room) = s.chats.get_mut(&meeting) {
            room.restore(earlier);
        }
        s.unsaved.chats.insert(meeting.clone());
    }
    if meeting_name(s) == meeting {
        let people = record
            .map(|text| store::record_people(&text))
            .unwrap_or_default();
        if let Some(current) = s.record.as_mut() {
            for name in people {
                if current.met(&name) {
                    s.unsaved.record = true;
                }
            }
        }
    }
    flush_files(s, &mut info);
    Update::RefreshDom
}

/// The answer to the leave request; nothing waits on it.
pub(crate) extern "C" fn on_left(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let name = match reply_parts(data) {
        Some((mut app, _)) => {
            let name = app.downcast_ref::<MeetState>().map(|s| s.name.clone());
            name.unwrap_or_default()
        }
        None => String::new(),
    };
    match http_answer(result) {
        Ok((200, _)) => {
            eprintln!("[azmeet] {name}: the meeting server no longer lists this participant")
        }
        Ok((404, _)) => eprintln!("[azmeet] {name}: the meeting had already ended on the server"),
        Ok((status, _)) => eprintln!(
            "[azmeet] {name}: the meeting server answered {status} to leaving; the record expires \
             by itself"
        ),
        Err(e) => eprintln!(
            "[azmeet] {name}: could not reach the meeting server to leave ({e}); the record \
             expires by itself"
        ),
    }
    Update::DoNothing
}

/// This computer's clock, milliseconds since 1970 (what the meeting server's times are in).
pub(crate) fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// This computer's time zone (seconds east of UTC) at `unix` seconds: summer time where it is
/// summer time then.
pub(crate) fn local_offset_at(unix: u64) -> i32 {
    use chrono::TimeZone;
    i64::try_from(unix)
        .ok()
        .and_then(|t| chrono::Local.timestamp_opt(t, 0).single())
        .map_or_else(
            || chrono::Local::now().offset().local_minus_utc(),
            |t| t.offset().local_minus_utc(),
        )
}

/// What the notice line says while no meetings service is set.
pub(crate) fn no_server_notice() -> String {
    t("azmeet-no-server")
}

/// A check of the meeting server (`GET /health`), counted and timed.
pub(crate) fn check_server(room: &mut RoomSession, now: u64) -> HttpJob {
    room.checks = room.checks.wrapping_add(1);
    room.checked_at = now;
    room.server_status = t_args("azmeet-asking-server", &[("server", Arg::from(&room.worker))]);
    HttpJob::health(room)
}

/// On the start screen, a meeting server that did not answer is asked again every
/// [`SERVER_RETRY_MS`]: an outage ends by itself, not with a restart.
pub(crate) fn server_retry(s: &mut MeetState) -> Option<HttpJob> {
    let room = s.room.as_mut()?;
    if room.stage != Stage::Start || room.server_ok || room.worker.is_empty() {
        return None;
    }
    let now = wall_ms();
    if now.saturating_sub(room.checked_at) < SERVER_RETRY_MS {
        return None;
    }
    Some(check_server(room, now))
}

/// The signer of this device's announcements in the call's room: its identity and the meeting
/// server's time, once the room's chat says this device is a member (the Worker takes an
/// announcement from a member only).
pub(crate) fn call_signer_ts(s: &MeetState) -> Option<u64> {
    let id = s.room.as_ref().map(|room| room.room_id.as_str())?;
    s.chats
        .get(id)
        .filter(|c| c.state == chatroom::Membership::Member)
        .map(|c| c.server_now(wall_ms()))
}

/// The ticket this endpoint announces names its addresses as they are now: the `Ready` event's
/// may predate its home relay (or a new network), and without the relay in the ticket a peer
/// behind NAT cannot be reached where no address lookup runs (azcloud's lesson, 10267afec). A
/// changed ticket is announced at once; a peer not connected yet dials the new one.
pub(crate) fn refresh_ticket(s: &mut MeetState) {
    let (Some(endpoint), Some(room)) = (s.endpoint.as_ref(), s.room.as_mut()) else {
        return;
    };
    if room.ticket.is_empty() {
        // Not ready yet: the `Ready` event brings the first one.
        return;
    }
    let now = endpoint.ticket().as_str().to_string();
    if !now.is_empty() && now != room.ticket {
        eprintln!("[azmeet] {}: this endpoint's addresses changed: announcing them", s.name);
        room.ticket = now;
        room.announced_at = None;
    }
}

/// Every 2 seconds: in a room, announce when due (once this device is a member there), else read
/// the peers list; on the start screen, ask a meeting server that did not answer again now and
/// then.
pub(crate) extern "C" fn room_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let (job, chat_jobs, direct_changed) = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => {
            let s = &mut *guard;
            // A direct call nobody took, a connection that never said hello.
            let direct_changed = direct_call::tick(s);
            refresh_ticket(s);
            let ts = call_signer_ts(s);
            let signer = s.identity.as_ref().zip(ts);
            let job = s.room.as_mut().and_then(|room| room.next_job(signer));
            // A room view's chat between calls: its presence on the room's rendezvous.
            let chat_jobs = chat_link::next_jobs(s);
            (job.or_else(|| server_retry(s)), chat_jobs, direct_changed)
        }
        None => (None, Vec::new(), false),
    };
    if let Some(job) = job {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    for job in chat_jobs {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    if direct_changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// Every [`CHAT_TICK_MS`]: each room's next request (`ChatRoom::next_call`), on its Thread.
pub(crate) extern "C" fn chat_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let jobs = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => chat_jobs(&mut guard),
        None => Vec::new(),
    };
    for job in jobs {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// The requests the rooms send now: the room on screen is read every [`CHAT_OPEN_SYNC_MS`], the
/// others every [`CHAT_IDLE_SYNC_MS`] (their unread counts).
pub(crate) fn chat_jobs(s: &mut MeetState) -> Vec<HttpJob> {
    let open = active_room_id(s);
    let now = wall_ms();
    let Some(me) = s.identity.as_ref() else {
        return Vec::new();
    };
    let mut jobs = Vec::new();
    for (id, chat) in s.chats.iter_mut() {
        let every = if open.as_deref() == Some(id.as_str()) {
            CHAT_OPEN_SYNC_MS
        } else {
            CHAT_IDLE_SYNC_MS
        };
        if let Some(call) = chat.next_call(me, now, every) {
            jobs.push(HttpJob::chat(chat, me, call, now));
        }
    }
    jobs
}

/// A room's answer: into its `ChatRoom`, and what that changed out to stdout, the files and the
/// window (`apply_chat_changes`).
pub(crate) extern "C" fn on_chat_answer(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, room, what)) = reply_chat(data) else {
        return Update::DoNothing;
    };
    let (status, body) = http_reply(result);
    let handle = app.clone();
    let (update, follow_up) = {
        let Some(mut guard) = app.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let changes = {
            let (Some(me), Some(chat)) = (s.identity.as_ref(), s.chats.get_mut(&room)) else {
                return Update::DoNothing;
            };
            chat.on_answer(me, &what, status, &body, wall_ms())
        };
        apply_chat_changes(s, &mut info, &handle, &room, changes)
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, handle, job);
    }
    update
}

/// The name of the member `device` of room `room`, as the window shows it.
pub(crate) fn chat_member_name(s: &MeetState, room: &str, device: &str) -> String {
    s.chats
        .get(room)
        .and_then(|c| c.member(device))
        .map(|m| member_name(&m.name))
        .unwrap_or_else(|| short_id(device).to_string())
}

/// What a room's answer changed: the lines for scripts (`AZMEET_CHAT`, `AZMEET_KEY`,
/// `AZMEET_MEMBER`, `AZMEET_HISTORY`, `AZMEET_ADMITTED`), what is on screen marked read, the files,
/// the room list, a knock let in entering its meeting, a room left forgotten. The window's
/// update, and a request to send.
pub(crate) fn apply_chat_changes(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    id: &str,
    changes: chatroom::Changes,
) -> (Update, Option<HttpJob>) {
    for m in &changes.arrived {
        let name = member_name(&m.name);
        println!("AZMEET_CHAT {name}: {}", m.text);
        eprintln!("[azmeet] {}: chat from {name}: {}", s.name, m.text);
    }
    for k in &changes.keys {
        let by = if k.made_here {
            String::from("me")
        } else {
            chat_member_name(s, id, &k.sender)
        };
        println!(
            "AZMEET_KEY {id} epoch={} key={} members={} by={by}",
            k.epoch, k.key_id, k.members
        );
    }
    for (what, list) in [
        ("joined", &changes.joined),
        ("left", &changes.left),
        ("knocking", &changes.knocking),
    ] {
        for m in list.iter() {
            println!(
                "AZMEET_MEMBER {id} {what} {} {}",
                member_name(&m.name),
                m.safety_code
            );
        }
    }
    if let Some(n) = changes.history {
        println!("AZMEET_HISTORY {id} {n}");
    }
    if changes.admitted {
        println!("AZMEET_ADMITTED {id}");
    }
    if let Some(problem) = &changes.problem {
        eprintln!("[azmeet] {}: room {}: {problem}", s.name, short_id(id));
        if active_room_id(s).as_deref() == Some(id) {
            s.notice = t_text(problem);
        }
    }
    // What is on screen is read.
    let on_screen = s.open_room.as_deref() == Some(id)
        || (call_room_id(s).as_deref() == Some(id) && s.panel == SidePanel::Chat);
    if on_screen {
        if let Some(chat) = s.chats.get_mut(id) {
            chat.mark_read();
        }
    }
    if !changes.arrived.is_empty() || changes.history.is_some() || changes.shown {
        s.unsaved.chats.insert(id.to_string());
    }
    let mut follow_up = None;
    match s.chats.get(id).map(|c| c.state) {
        Some(chatroom::Membership::Left) => forget_room(s, id),
        Some(chatroom::Membership::Member) => {
            remember_room(s, id);
            follow_up = enter_admitted(s, info, app, id);
        }
        Some(_) => remember_room(s, id),
        None => {}
    }
    flush_files(s, info);
    let update = if changes.shown || follow_up.is_some() {
        Update::RefreshDom
    } else {
        Update::DoNothing
    };
    (update, follow_up)
}

/// A knock was let in: the waiting room that asked enters its meeting now, with the link the
/// room's first key brought.
pub(crate) fn enter_admitted(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    id: &str,
) -> Option<HttpJob> {
    let invite = s.chats.get(id).and_then(|c| c.invite().cloned())?;
    let room = s.room.as_mut()?;
    let waiting_here = room.waiting.as_ref().map(|found| found.room.as_str()) == Some(id);
    if room.stage != Stage::Waiting || !room.asked || !waiting_here {
        return None;
    }
    let mut found = room.waiting.take()?;
    found.invite = Some(invite);
    eprintln!("[azmeet] {}: let in: into the meeting", s.name);
    enter_room(s, info, app, found)
}

/// Room `id` was left: off the room list, out of memory, its view closed.
pub(crate) fn forget_room(s: &mut MeetState, id: &str) {
    if let Some(me) = s.identity.as_ref() {
        if s.index.remove(me.device(), id) {
            s.unsaved.index = true;
        }
    }
    s.chats.remove(id);
    if s.open_room.as_deref() == Some(id) {
        s.open_room = None;
        s.room_copied = false;
    }
    s.notice = t("azmeet-left-room");
    println!("AZMEET_LEFT_ROOM {id}");
}

/// Makes a new room here - its id and its invite secret (CRYPTO.md section 4) - and registers it
/// with the meeting server; its waiting room (a chat room's view) opens with the answer
/// (`straight_in`: a meeting is entered at once - a script's). `times`: RFC 3339 start and end.
pub(crate) fn begin_new_room(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    straight_in: bool,
    kind: chatroom::RoomKind,
    times: Option<(String, String)>,
) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if s.identity.is_none() {
            s.notice = t("azmeet-key-not-loaded");
            return Update::RefreshDom;
        }
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        if room.worker.is_empty() {
            if kind == chatroom::RoomKind::Meeting {
                return open_local_meeting(s, t("azmeet-preview-no-server"));
            }
            s.notice = no_server_notice();
            return Update::RefreshDom;
        }
        let invite = crypto::random_id()
            .ok()
            .and_then(|id| crypto::Invite::generate(&id).ok());
        let Some(invite) = invite else {
            s.notice = t("azmeet-no-random");
            return Update::RefreshDom;
        };
        room.stage = Stage::Opening;
        room.busy = true;
        room.created = true;
        room.straight_in = straight_in;
        room.join_secret = None;
        let minting = Minting { invite, kind, times };
        let job = HttpJob::create_room(room, &minting);
        room.minting = Some(minting);
        s.open_room = None;
        s.notice = t(match kind {
            chatroom::RoomKind::Chat => "azmeet-making-chat-room",
            chatroom::RoomKind::Meeting => "azmeet-making-meeting",
        });
        job
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

/// Looks the meeting of `text` (a link or a code) up; its waiting room (a chat room's view) opens
/// with the answer (`straight_in`: a meeting is entered at once - a script's). A bare link of a
/// room this device is in uses the link it keeps.
pub(crate) fn begin_join(data: &mut RefAny, info: &mut CallbackInfo, text: &str, straight_in: bool) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if s.identity.is_none() {
            s.notice = t("azmeet-key-not-loaded");
            return Update::RefreshDom;
        }
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        if room.worker.is_empty() {
            s.notice = no_server_notice();
            return Update::RefreshDom;
        }
        let Some(link) = rooms::read_room_link(text) else {
            s.notice = t("azmeet-not-a-link");
            return Update::RefreshDom;
        };
        let kept = match &link.key {
            RoomKey::Id(id) => s
                .chats
                .get(id)
                .and_then(|c| c.invite().map(|i| i.secret().to_string())),
            RoomKey::Code(_) => None,
        };
        room.stage = Stage::Opening;
        room.busy = true;
        room.created = false;
        room.straight_in = straight_in;
        room.minting = None;
        room.join_secret = link.secret.clone().or(kept);
        s.open_room = None;
        s.notice = t("azmeet-looking-up");
        HttpJob::look_up(room, &link.key)
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

pub(crate) extern "C" fn on_new_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    begin_new_room(&mut data, &mut info, false, chatroom::RoomKind::Meeting, None)
}

pub(crate) extern "C" fn on_new_chat_room(mut data: RefAny, mut info: CallbackInfo) -> Update {
    begin_new_room(&mut data, &mut info, false, chatroom::RoomKind::Chat, None)
}

/// The times the "Schedule" form asks for: the start in this computer's time zone, the length in
/// minutes; RFC 3339 in UTC, as the Worker takes them.
pub(crate) fn schedule_times(start: &str, minutes: &str) -> Result<(String, String), String> {
    let today = local_offset_at(azul_storage::time::now_unix());
    let first_guess = rooms::parse_local_start(start, today)
        .ok_or_else(|| t("azmeet-schedule-bad-start"))?;
    // The offset of the day it starts on (summer time or not), not today's.
    let starts =
        rooms::parse_local_start(start, local_offset_at(first_guess)).unwrap_or(first_guess);
    let minutes = minutes
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|m| (1..=10_080).contains(m))
        .ok_or_else(|| t("azmeet-schedule-bad-length"))?;
    Ok((
        azul_storage::time::iso8601(starts),
        azul_storage::time::iso8601(starts + minutes * 60),
    ))
}

/// "Schedule": a meeting with the form's start and length.
pub(crate) extern "C" fn on_schedule(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let times = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let (start, minutes) = s
            .room
            .as_ref()
            .map(|room| (room.schedule_start.clone(), room.schedule_minutes.clone()))
            .unwrap_or_default();
        match schedule_times(&start, &minutes) {
            Ok(times) => times,
            Err(why) => {
                s.notice = why;
                return Update::RefreshDom;
            }
        }
    };
    begin_new_room(
        &mut data,
        &mut info,
        false,
        chatroom::RoomKind::Meeting,
        Some(times),
    )
}

pub(crate) extern "C" fn on_schedule_start(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.schedule_start = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

pub(crate) extern "C" fn on_schedule_minutes(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.schedule_minutes = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

pub(crate) extern "C" fn on_join(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.room.as_ref().map(|room| room.join_text.clone()))
        .unwrap_or_default();
    begin_join(&mut data, &mut info, &text, false)
}

/// Enters the meeting `found`: from the waiting room's "Join now" / "Start meeting", or at once
/// for a script. This device joins the room's chat (its member record goes out); announcing
/// starts once it is a member (the announcement to send right away, when it can, is returned),
/// and the meeting's record and the chat of an earlier visit come in.
pub(crate) fn enter_room(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    found: RoomInfo,
) -> Option<HttpJob> {
    ensure_chat(s, &found);
    let id = found.room.clone();
    if let Some(chat) = s.chats.get_mut(&id) {
        chat.join();
    }
    let times = found.starts_at.zip(found.ends_at);
    let room = s.room.as_mut()?;
    room.enter(found);
    println!("AZMEET_ROOM {}", room.room_id);
    println!("AZMEET_LINK {}", room.link);
    if !room.code.is_empty() {
        println!("AZMEET_CODE {}", room.code);
    }
    if let Some((a, b)) = times {
        println!(
            "AZMEET_TIMES {} {}",
            azul_storage::time::iso8601(a),
            azul_storage::time::iso8601(b)
        );
    }
    eprintln!(
        "[azmeet] {}: in meeting {} ({})",
        s.name, room.code, room.room_id
    );
    s.notice.clear();
    s.link_status = String::from("waiting for others to join");
    s.open_room = None;
    s.room_copied = false;
    remember_room(s, &id);
    enter_record(s);
    // Back in a meeting this side was in before: its chat and people come back.
    read_history(s, info, app.clone(), &id);
    flush_files(s, info);
    // The name typed in the waiting room, should its field still have the focus.
    save_prefs(s, info);
    // The others hear this side's microphone and camera as the waiting room left them.
    network_changed(s, false);
    let ts = call_signer_ts(s);
    let signer = s.identity.as_ref().zip(ts);
    s.room.as_mut()?.first_job(signer)
}

/// A meeting was found: the microphone and the camera start as the settings' Meetings say (on,
/// unless "Join with the microphone off" / "... the camera off"). Not where they are a test tone
/// or a test pattern (a headless run, `--test-tone`, `--test-pattern`): those keep the state
/// their switches gave them, so a script knows what it gets.
/// The start screen shows this side's camera and microphone live, as Meet does before a
/// meeting: they open with the join defaults (muted or the camera off when the settings say
/// so), whether or not a meetings service answers.
pub(crate) fn open_devices_at_start(s: &mut MeetState) {
    apply_join_defaults(s);
}

pub(crate) fn apply_join_defaults(s: &mut MeetState) {
    if !devices_allowed() {
        return;
    }
    if !setting_on("AZMEET_TEST_TONE") {
        s.mic_on = !s.join_muted;
        sync_mic(s);
    }
    if !setting_on("AZMEET_TEST_PATTERN") {
        s.cam_on = !s.join_camera_off;
    }
}

/// The waiting room's "Join now" / "Start meeting": into the meeting it shows; with only the
/// meeting's code, "Ask to join": a knock, and the meeting is entered once a member lets this
/// device in (`enter_admitted`).
pub(crate) extern "C" fn on_join_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if let Some(room) = s.room.as_ref().filter(|room| room.preview) {
            // The preview's meeting exists only here: nobody could join it.
            s.notice = cannot_join_notice(room);
            return Update::RefreshDom;
        }
        let knock = s
            .room
            .as_ref()
            .filter(|room| room.stage == Stage::Waiting && !room.asked)
            .and_then(|room| room.waiting.as_ref())
            .filter(|found| found.invite.is_none())
            .map(|found| found.room.clone());
        if let Some(id) = knock {
            if let Some(chat) = s.chats.get_mut(&id) {
                chat.join();
            }
            if let Some(room) = s.room.as_mut() {
                room.asked = true;
            }
            remember_room(s, &id);
            println!("AZMEET_KNOCK {id}");
            s.notice = t("azmeet-asked-to-join");
            return Update::RefreshDom;
        }
        let found = s
            .room
            .as_mut()
            .filter(|room| {
                room.stage == Stage::Waiting
                    && room.waiting.as_ref().is_some_and(|f| f.invite.is_some())
            })
            .and_then(|room| room.waiting.take());
        let Some(found) = found else {
            return Update::DoNothing;
        };
        enter_room(s, &mut info, &app, found)
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The waiting room's Back: to the start screen without joining (a meeting this side made
/// expires on the server by itself; the devices close with the waiting room).
pub(crate) extern "C" fn on_waiting_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Waiting) else {
        return Update::DoNothing;
    };
    // A new session: the answer to a read of this waiting room's peers still in flight is
    // ignored, whatever waiting room opens next.
    room.session = room.session.wrapping_add(1);
    room.stage = Stage::Start;
    room.waiting = None;
    room.close_waiting();
    room.created = false;
    room.copied = false;
    s.notice.clear();
    Update::RefreshDom
}

pub(crate) extern "C" fn on_join_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.join_text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// `text` to the clipboard.
pub(crate) fn copy_text(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
        files: azul::vec::StringVec::create(),
        image: azul::option::OptionU8Vec::None,
        link: azul::option::OptionString::None,
        concealed: false,
    });
}

/// "Copy link": the meeting's link with its invite secret (the waiting room's before it is
/// entered) to the clipboard.
pub(crate) extern "C" fn on_copy_link(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let link = {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        let link = match &room.waiting {
            Some(found) if room.link.is_empty() => found.share_link(),
            _ => room.link.clone(),
        };
        if link.is_empty() {
            return Update::DoNothing;
        }
        room.copied = true;
        link
    };
    copy_text(&mut info, &link);
    Update::RefreshDom
}

/// The times `--starts-at` / `--ends-at` ask for (both or neither).
pub(crate) fn launch_times() -> Option<(String, String)> {
    setting("AZMEET_STARTS_AT").zip(setting("AZMEET_ENDS_AT"))
}

/// `--open <link>`: the room view of that room - one this device is in at once, another looked
/// up and joined.
pub(crate) fn open_link(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let known = match rooms::parse_room_link(text) {
        Some(RoomKey::Id(id)) => data
            .downcast_mut::<MeetState>()
            .map(|mut s| {
                let here = s.chats.contains_key(&id);
                if here {
                    open_room_view(&mut s, &id, true);
                }
                here
            })
            .unwrap_or(false),
        _ => false,
    };
    if known {
        return Update::RefreshDom;
    }
    begin_join(data, info, text, false)
}

/// What the command line asked to start with, once this device's identity is loaded (nothing is
/// joined or made before): `--open <link>`, `--join <link>` / `--autocreate` / `--screen call`
/// (a meeting without a click, past the waiting room unless `--waiting-room`; a chat room with
/// `--chat-room`), `--screen waiting` (a new meeting's waiting room, or a preview of one when no
/// meeting server answers). Once.
pub(crate) fn autostart(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let due = data
        .downcast_mut::<MeetState>()
        .is_some_and(|mut s| s.identity.is_some() && std::mem::take(&mut s.autostart_due));
    if !due {
        return Update::DoNothing;
    }
    let straight_in = !setting_on("AZMEET_WAITING_ROOM");
    if let Some(link) = setting("AZMEET_OPEN") {
        return open_link(data, info, &link);
    }
    if let Some(link) = setting("AZMEET_JOIN") {
        if let Some(mut s) = data.downcast_mut::<MeetState>() {
            if let Some(room) = s.room.as_mut() {
                room.join_text = link.clone();
            }
        }
        return begin_join(data, info, &link, straight_in);
    }
    // The service is being asked as this runs: a meeting it does not make in the end is made
    // here (`open_local_meeting`), so a service that is set is tried.
    let has_server = data
        .downcast_ref::<MeetState>()
        .is_some_and(|s| s.room.as_ref().is_some_and(|room| !room.worker.is_empty()));
    let kind = if setting_on("AZMEET_CHAT_ROOM") {
        chatroom::RoomKind::Chat
    } else {
        chatroom::RoomKind::Meeting
    };
    match launch_args().screen {
        args::Screen::Call => begin_new_room(data, info, straight_in, kind, launch_times()),
        args::Screen::Waiting if has_server => {
            begin_new_room(data, info, false, kind, launch_times())
        }
        args::Screen::Waiting => preview_waiting_room(data),
        _ if setting_on("AZMEET_AUTOCREATE") => {
            begin_new_room(data, info, straight_in, kind, launch_times())
        }
        _ => Update::DoNothing,
    }
}

/// `--screen waiting` with no meeting server answering (a screenshot of the waiting room): the
/// meeting made here ([`open_local_meeting`]).
pub(crate) fn preview_waiting_room(data: &mut RefAny) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let notice = match s.room.as_ref() {
        Some(room) if room.stage == Stage::Start => preview_notice(room),
        _ => return Update::DoNothing,
    };
    open_local_meeting(s, notice)
}

/// A meeting made here, when no meetings service answers (none is set, it is down, or it did
/// not answer the creation): its waiting room with a code and a link made up here and nobody
/// in it. The camera preview, the microphone's level, the sound check, the devices and the
/// settings all work in it; only what needs the service - letting others join, announcing,
/// the peers list - says that it is unreachable, and "Retry" asks it again. stdout:
/// `AZMEET_WAITING <link>`, as for a real one.
pub(crate) fn open_local_meeting(s: &mut MeetState, notice: String) -> Update {
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    let code = gen_link();
    let found = RoomInfo {
        room: code.clone(),
        link: format!("{}{code}", rooms::APP_LINK_PREFIX),
        code,
        kind: chatroom::RoomKind::Meeting,
        starts_at: None,
        ends_at: None,
        invite_key: None,
        invite: None,
    };
    println!("AZMEET_WAITING {}", found.link);
    eprintln!(
        "[azmeet] {}: meeting {} made here: no meetings service answers at {}",
        s.name, found.code, room.worker
    );
    room.stage = Stage::Waiting;
    room.busy = false;
    room.created = true;
    room.copied = false;
    room.minting = None;
    room.waiting = Some(found);
    room.close_waiting();
    room.preview = true;
    s.notice = notice;
    apply_join_defaults(s);
    Update::RefreshDom
}

/// The waiting room of a meeting made here: "Retry" asks the meetings service again to make
/// the meeting for real; while it still does not answer, the meeting stays the one made here.
pub(crate) extern "C" fn on_preview_retry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kind = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let Some(room) = guard.room.as_mut().filter(|room| room.preview) else {
            return Update::DoNothing;
        };
        room.stage = Stage::Start;
        room.preview = false;
        room.waiting = None;
        chatroom::RoomKind::Meeting
    };
    begin_new_room(&mut data, &mut info, false, kind, None)
}
