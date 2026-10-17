//! The room view: a room's chat, members and safety codes outside a call, joining its call.

use super::*;

/// A row's button (a room of the list, a member, a knock): the app and the row's key.
pub(crate) struct RowClick {
    pub(crate) app: RefAny,
    pub(crate) key: String,
}

/// The app and the key of a row's click.
pub(crate) fn row_click(mut data: RefAny) -> Option<(RefAny, String)> {
    let click = data.downcast_ref::<RowClick>()?;
    Some((click.app.clone(), click.key.clone()))
}

/// The room view of room `id`: its chat read now (and every 2 seconds while it is open), and with
/// `join` this device joins it when it is not in it.
pub(crate) fn open_room_view(s: &mut MeetState, id: &str, join: bool) {
    let Some(chat) = s.chats.get_mut(id) else {
        return;
    };
    if join {
        chat.join();
    }
    chat.mark_read();
    s.open_room = Some(id.to_string());
    s.room_copied = false;
    s.notice.clear();
    println!("AZMEET_OPEN {id}");
    remember_room(s, id);
}

/// A room of the start screen's list: its view.
pub(crate) extern "C" fn on_open_room(data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, room)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    open_room_view(&mut guard, &room, false);
    Update::RefreshDom
}

/// The room view's Back: to the start screen.
pub(crate) extern "C" fn on_room_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.open_room = None;
        s.room_copied = false;
    }
    Update::RefreshDom
}

/// "Leave room": this device leaves the room (its signed leave goes out next); the others' next
/// message is under a key it has no copy of (CRYPTO.md section 7).
pub(crate) extern "C" fn on_room_leave(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(id) = s.open_room.clone() else {
        return Update::DoNothing;
    };
    if let Some(chat) = s.chats.get_mut(&id) {
        chat.leave();
        println!("AZMEET_LEAVING_ROOM {id}");
        s.notice = t("azmeet-leaving-room");
    }
    Update::RefreshDom
}

/// The room view's "Join call": the room's waiting room, then its call.
pub(crate) extern "C" fn on_room_call(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(id) = s.open_room.clone() else {
            return Update::DoNothing;
        };
        let Some(chat) = s.chats.get(&id) else {
            return Update::DoNothing;
        };
        if chat.state != chatroom::Membership::Member {
            s.notice = t("azmeet-members-only-call");
            return Update::RefreshDom;
        }
        let found = RoomInfo {
            room: id.clone(),
            code: chat.code.clone(),
            link: format!("{}{id}", rooms::APP_LINK_PREFIX),
            kind: chat.kind,
            starts_at: chat.starts_at,
            ends_at: chat.ends_at,
            invite_key: chat.invite().map(crypto::Invite::invite_key),
            invite: chat.invite().cloned(),
        };
        let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
            return Update::DoNothing;
        };
        room.session = room.session.wrapping_add(1);
        room.created = false;
        room.straight_in = false;
        room.stage = Stage::Waiting;
        room.copied = false;
        room.waiting = Some(found);
        room.close_waiting();
        let job = room.waiting_job();
        s.open_room = None;
        s.notice.clear();
        apply_join_defaults(s);
        job
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The room view's "Copy link": the room's link with its invite secret.
pub(crate) extern "C" fn on_room_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let link = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(link) = s
            .open_room
            .as_ref()
            .and_then(|id| s.chats.get(id))
            .map(|chat| chat.link(rooms::APP_LINK_PREFIX))
        else {
            return Update::DoNothing;
        };
        s.room_copied = true;
        link
    };
    copy_text(&mut info, &link);
    Update::RefreshDom
}

/// "Admit" beside a knock (`<room> <device>`): the device is let in, and gets a key at once.
pub(crate) extern "C" fn on_admit(data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, key)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some((room, device)) = key.split_once(' ') else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if let Some(chat) = s.chats.get_mut(room) {
        chat.admit(device);
        println!("AZMEET_ADMITTING {room} {device}");
    }
    Update::RefreshDom
}

/// "Verified" beside a member (`<room> <device>`): the user compared its safety code; kept by
/// device in `meet/rooms.json`, so it holds in every room.
pub(crate) extern "C" fn on_verify(data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some((room, device)) = key.split_once(' ') else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let name = chat_member_name(s, room, device);
    if s.index.verify(device, &name) {
        s.unsaved.index = true;
        println!("AZMEET_VERIFIED {device}");
    }
    flush_files(s, &mut info);
    Update::RefreshDom
}

/// The meetings service's Retry (Settings > Meetings): it is asked again now.
pub(crate) extern "C" fn on_retry_server(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
            return Update::DoNothing;
        };
        if room.worker.is_empty() {
            room.server_status = no_server_notice();
            return Update::RefreshDom;
        }
        check_server(room, wall_ms())
    };
    spawn_http(&mut info, data.clone(), job);
    Update::RefreshDom
}

/// The window is up: the pumps start (the iroh link, the room's polls, the chat rooms), this
/// device's identity is loaded, and what the command line asked for starts once it is.
pub(crate) extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // azul-appkit's `--shot <png>`: the window as a PNG after its settle time, then the process
    // ends (`AzMeet --screen waiting --test-pattern --shot waiting.png`).
    if let Some(kit_ref) = data.downcast_ref::<MeetState>().map(|s| s.kit.clone()) {
        kit::on_window_created(&kit_ref, &mut info);
    }
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), pump_link, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(pace::BUSY_MS))),
    );
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), room_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(ROOM_POLL_MS))),
    );
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), chat_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(CHAT_TICK_MS))),
    );
    // The meetings service is asked now that the window is up: the window never waits for it.
    let first_check = data.downcast_mut::<MeetState>().and_then(|mut s| server_retry(&mut s));
    if let Some(job) = first_check {
        spawn_http(&mut info, data.clone(), job);
    }
    load_identity(&mut data, &mut info);
    autostart(&mut data, &mut info)
}

/// A chunk from the microphone: sent to the peers, and shown on the level meter.
pub(crate) extern "C" fn mic_on_frame(mut data: RefAny, info: CallbackInfo, frame: AudioFrame) -> Update {
    let samples: &[f32] = frame.samples.as_ref();
    let meter = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        send_audio(s, frame.sample_rate, frame.channels, samples);
        s.sound_check.feed(frame.sample_rate, frame.channels, samples);
        meter_change(s, samples)
    };
    if let Some((bar, level)) = meter {
        show_level(info, bar, level);
    }
    Update::DoNothing
}

pub(crate) extern "C" fn mic_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mic_on = !s.mic_on;
        sync_mic(s);
        send_state_to_all(s);
        network_changed(s, false);
    }
    Update::RefreshDom
}

pub(crate) extern "C" fn deafen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.deafened = !s.deafened;
        if s.deafened {
            if let Some(shared) = s.playout.as_ref() {
                for jitter in lock(shared).peers.values_mut() {
                    jitter.clear();
                }
            }
        }
        send_state_to_all(s);
    }
    Update::RefreshDom
}

/// Leaves the call: disconnects every peer, stops announcing and polling, and returns to the
/// start screen, where the room stays in "Your rooms" (its chat goes on; "Leave room" in its view
/// leaves the room itself). Returns the signed request that takes this endpoint off the room's
/// list at once (without it the meeting server drops the record after its peer TTL).
pub(crate) fn leave_meeting(s: &mut MeetState) -> Option<HttpJob> {
    if let Some(endpoint) = s.endpoint.as_ref() {
        for r in &s.remotes {
            endpoint.disconnect(r.handle);
        }
        for p in &s.pending {
            endpoint.disconnect(p.handle);
        }
    }
    s.remotes.clear();
    s.pending.clear();
    drop_audio(s, None);
    s.packetizer.reset();
    s.opus_framer.reset();
    for out in s.video_out.values_mut() {
        out.stop();
    }
    s.drop_video.clear();
    s.plan = routes::Plan::default();
    s.relay = Relaying::default();
    s.record = None;
    s.unsaved.record = false;
    s.speaker = speaker::ActiveSpeaker::new();
    s.link_status = String::from("not in a meeting");
    s.notice = t("azmeet-left-meeting");
    let ts = call_signer_ts(s).unwrap_or_else(wall_ms);
    let room = s.room.as_mut()?;
    // An ended meeting is gone from the server already.
    let job = match s.identity.as_ref() {
        Some(me) if room.stage == Stage::InRoom && !room.node_id.is_empty() && !room.direct => {
            Some(HttpJob::leave(room, me, ts))
        }
        _ => None,
    };
    s.direct.end_call();
    eprintln!("[azmeet] {}: left meeting {}", s.name, room.code);
    room.leave();
    job
}

pub(crate) extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => {
            // The chat as it stands goes into the meeting's folder before it is cleared.
            flush_files(&mut guard, &mut info);
            leave_meeting(&mut guard)
        }
        None => return Update::DoNothing,
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The camera on or off: the encoders of its renditions close with it (`stop_culled`), and the
/// others hear it in the next report.
pub(crate) extern "C" fn cam_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.cam_on = !s.cam_on;
        network_changed(s, false);
    }
    Update::RefreshDom
}
pub(crate) extern "C" fn screen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.screen_on = !s.screen_on;
        network_changed(s, false);
    }
    Update::RefreshDom
}

/// Grid or speaker view: the tiles change size, so what this side asks for changes.
pub(crate) extern "C" fn view_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.view = match s.view {
            ViewMode::Grid => ViewMode::Speaker,
            ViewMode::Speaker => ViewMode::Grid,
        };
        // Until the tiles are laid out again, the new boxes' heights count.
        for r in s.remotes.iter_mut() {
            r.tile_height = [None; 2];
            r.tile_px = [None; 2];
        }
        network_changed(s, false);
    }
    Update::RefreshDom
}
