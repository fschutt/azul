//! This device's identity (`identity.rs`, CRYPTO.md section 3): its key from the keyring or the
//! identity file, made on the first start.

use super::*;

/// A keyring answer, by name (never with the secret): a word the window says.
pub(crate) fn keyring_outcome(result: &KeyringResult) -> Arg {
    let (key, english) = match result {
        KeyringResult::Stored => ("azmeet-keyring-stored", "stored"),
        KeyringResult::Retrieved(_) => ("azmeet-keyring-retrieved", "retrieved"),
        KeyringResult::Deleted => ("azmeet-keyring-deleted", "deleted"),
        KeyringResult::NotFound => ("azmeet-keyring-not-found", "not found"),
        KeyringResult::Denied => ("azmeet-keyring-denied", "denied"),
        KeyringResult::Unavailable => ("azmeet-keyring-unavailable", "unavailable"),
        KeyringResult::Error => ("azmeet-keyring-error", "error"),
    };
    Arg::word(key, english)
}

/// This device's identity: from the identity file (`--identity-file`) at once, else the keyring
/// is asked (its answer is the window's `KeyringResult` event: `on_keyring_result`).
pub(crate) fn load_identity(data: &mut RefAny, info: &mut CallbackInfo) {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return;
    };
    let s = &mut *guard;
    if s.identity.is_some() || s.keyring.is_some() {
        return;
    }
    match setting("AZMEET_IDENTITY_FILE") {
        Some(path) => match identity::load_or_create(std::path::Path::new(&path)) {
            Ok((me, created)) => identity_ready(s, me, identity::Source::File(path), created),
            Err(why) => {
                eprintln!("[azmeet] {}: {why}", s.name);
                session_identity(s, Phrase::new("azmeet-keyring-file-unused").arg("why", why.as_str()).into());
            }
        },
        None => {
            info.keyring_get(identity::KEYRING_KEY);
            s.keyring = Some(KeyringStep::Get);
        }
    }
}

/// An identity for this run only (`why`: what the keyring said, words the window says).
pub(crate) fn session_identity(s: &mut MeetState, why: Text) {
    match crypto::Identity::generate() {
        Ok(me) => {
            s.notice = t_args("azmeet-keyring-session", &[("why", Arg::from(t_text(&why)))]);
            identity_ready(s, me, identity::Source::Session(why), true);
        }
        Err(e) => {
            s.notice = t_args("azmeet-no-device-key", &[("why", Arg::from(e.to_string()))]);
        }
    }
}

/// The identity is here: stdout says it (`AZMEET_IDENTITY <device> <keyring|file|session>`,
/// `AZMEET_SAFETY <code>`), and the rooms of `meet/rooms.json` come back.
pub(crate) fn identity_ready(
    s: &mut MeetState,
    me: crypto::Identity,
    source: identity::Source,
    created: bool,
) {
    println!("AZMEET_IDENTITY {} {}", me.device(), source.word());
    println!("AZMEET_SAFETY {}", me.safety_code());
    eprintln!(
        "[azmeet] {}: this device {} ({}){}",
        s.name,
        short_id(me.device()),
        source.describe(),
        if created { ", made now" } else { "" }
    );
    s.identity = Some(me);
    s.identity_source = Some(source);
    restore_rooms(s);
}

/// The rooms `meet/rooms.json` lists for this device: back in memory, read on the next ticks. A
/// room whose invite secret does not open here (another device's seal) is left out.
pub(crate) fn restore_rooms(s: &mut MeetState) {
    let Some(me) = s.identity.as_ref() else {
        return;
    };
    let worker = s.room.as_ref().map(|r| r.worker.clone()).unwrap_or_default();
    let entries: Vec<roomlist::RoomEntry> = s
        .index
        .rooms_of(me.device())
        .into_iter()
        .cloned()
        .collect();
    for entry in entries {
        if s.chats.contains_key(&entry.room) {
            continue;
        }
        let invite = entry
            .invite
            .as_deref()
            .and_then(|sealed| me.open_local(&entry.room, sealed))
            .and_then(|secret| crypto::Invite::new(&entry.room, &secret));
        if entry.member && invite.is_none() {
            eprintln!(
                "[azmeet] {}: room {}: its link does not open on this device; left out",
                s.name,
                short_id(&entry.room)
            );
            continue;
        }
        let server = if entry.server.is_empty() {
            worker.clone()
        } else {
            entry.server.clone()
        };
        let mut chat = chatroom::ChatRoom::new(
            &server,
            &entry.room,
            &entry.code,
            chatroom::RoomKind::parse(Some(entry.kind.as_str())),
            invite,
            &s.name,
        );
        chat.state = if entry.member {
            chatroom::Membership::Member
        } else {
            chatroom::Membership::Knocking
        };
        chat.read_seq = entry.read_seq;
        chat.starts_at = entry.starts_at;
        chat.ends_at = entry.ends_at;
        chat.restore_departed(entry.departed.clone());
        s.chats.insert(entry.room.clone(), chat);
    }
}

/// The keyring answered (a window event): the seed read, or a new one made and stored, or - when
/// the keyring cannot keep one - a seed for this run only. Then what the command line asked for
/// starts (`autostart`).
pub(crate) extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let OptionKeyringResult::Some(result) = info.get_keyring_result() else {
        return Update::DoNothing;
    };
    let ready = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        match (s.keyring.take(), result) {
            (Some(KeyringStep::Get), KeyringResult::Retrieved(secret)) => {
                match crypto::Identity::from_secret_json(secret.as_str()) {
                    Ok(me) => identity_ready(s, me, identity::Source::Keyring, false),
                    Err(e) => session_identity(
                        s,
                        Phrase::new("azmeet-keyring-unreadable").arg("why", e.to_string()).into(),
                    ),
                }
                true
            }
            (Some(KeyringStep::Get), KeyringResult::NotFound) => {
                match crypto::Identity::generate() {
                    Ok(me) => {
                        let json = me.to_secret_json();
                        info.keyring_store(identity::KEYRING_KEY, json.as_str(), false);
                        s.keyring = Some(KeyringStep::Store);
                        identity_ready(s, me, identity::Source::Keyring, true);
                    }
                    Err(e) => {
                        s.notice = t_args("azmeet-no-device-key", &[("why", Arg::from(e.to_string()))]);
                    }
                }
                true
            }
            (Some(KeyringStep::Get), other) => {
                session_identity(s, Phrase::new("azmeet-keyring-is").arg("state", keyring_outcome(&other)).into());
                true
            }
            (Some(KeyringStep::Store), KeyringResult::Stored) => {
                eprintln!("[azmeet] {}: this device's key is in the system keyring", s.name);
                false
            }
            (Some(KeyringStep::Store), other) => {
                let why: Text = Phrase::new("azmeet-keyring-not-kept").arg("state", keyring_outcome(&other)).into();
                s.notice = t_args("azmeet-keyring-run-only", &[("why", Arg::from(t_text(&why)))]);
                s.identity_source = Some(identity::Source::Session(why));
                false
            }
            (None, _) => false,
        }
    };
    if ready {
        let _ = autostart(&mut data, &mut info);
    }
    Update::RefreshDom
}
