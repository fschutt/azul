//! System-wide ("global") hotkeys: the platform-independent half.
//!
//! What is pinned here, bottom to top:
//!
//! 1. **The accelerator** (`azul_core::global_hotkey`): parsing, normalisation (case, spacing,
//!    modifier order, left/right twins, `CmdOrCtrl`), the menu-combo conversion, the rule that a
//!    typing key needs Ctrl/Alt/Cmd, the display string and the xkb / portal trigger names the
//!    Linux backends grab by.
//! 2. **The registry** (`azul_layout::managers::global_hotkey::GlobalHotkeyRegistry`) against a
//!    fake backend: register, duplicate, a combination another app owns, unregister, a pending
//!    (portal) answer that arrives later, and moving registrations onto a new backend.
//! 3. **The drain**: a fire parked by a backend comes back out of `take_fired` with the callback
//!    and the `RefAny` it was registered with, in order, once - and a fire for a registration that
//!    is gone is dropped.
//! 4. **The headless simulation**: with `simulated_backend` installed, `simulate` presses a
//!    registered combination (however it is spelled) and nothing else.
//!
//! The OS backends (Carbon, Win32, X11, the portal) cannot run here; their
//! manual check recipes are in `scripts/GLOBAL_HOTKEYS_2026_09_28.md`.

#![cfg(feature = "text_layout")]

use std::sync::{Mutex, MutexGuard, PoisonError};

use azul_core::{
    callbacks::{CoreCallback, Update},
    global_hotkey::{
        portal_trigger, xkb_keysym_name, GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId,
        GlobalHotkeyStatus, HotkeyModifiers,
    },
    menu::CoreMenuCallback,
    refany::{OptionRefAny, RefAny},
    window::{VirtualKeyCode as K, VirtualKeyCodeCombo, VirtualKeyCodeVec},
};
use azul_layout::{
    callbacks::CallbackInfo,
    managers::global_hotkey::{
        self as registry, BackendGrant, GlobalHotkeyBackend, GlobalHotkeyRegistry,
        MAX_PENDING_FIRES, SIMULATED_BACKEND_NAME,
    },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The fake backend records its calls here, and the process-wide registry is
/// one static - both are shared by every test in this binary, so the tests
/// that touch either take this lock.
static SERIAL: Mutex<()> = Mutex::new(());
static CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn exclusive() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    CALLS.lock().unwrap_or_else(PoisonError::into_inner).clear();
    guard
}

fn record(call: String) {
    CALLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(call);
}

fn calls() -> Vec<String> {
    CALLS.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn mods(ctrl: bool, alt: bool, shift: bool, meta: bool) -> HotkeyModifiers {
    HotkeyModifiers {
        ctrl,
        alt,
        shift,
        meta,
    }
}

fn hk(modifiers: HotkeyModifiers, key: K) -> GlobalHotkey {
    GlobalHotkey { modifiers, key }
}

fn ctrl_alt(key: K) -> GlobalHotkey {
    hk(mods(true, true, false, false), key)
}

fn fake_probe() -> Result<(), String> {
    Ok(())
}

/// Grants everything except Ctrl+Alt+T, which "another application" owns
/// (it is the terminal shortcut on GNOME and Ubuntu).
fn fake_register(
    id: GlobalHotkeyId,
    hotkey: &GlobalHotkey,
) -> Result<BackendGrant, GlobalHotkeyError> {
    record(format!("register {} {}", id.id, hotkey.to_display_string_for(false)));
    if *hotkey == ctrl_alt(K::T) {
        return Err(GlobalHotkeyError::TakenByAnotherApp);
    }
    Ok(BackendGrant::Active)
}

fn fake_unregister(id: GlobalHotkeyId) {
    record(format!("unregister {}", id.id));
}

fn fake_poll() {}

fn fake_backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: "fake",
        probe: fake_probe,
        register: fake_register,
        unregister: fake_unregister,
        poll: fake_poll,
        needs_loop_polling: false,
    }
}

/// Like the Wayland portal: accepts the request, answers later.
fn pending_register(
    id: GlobalHotkeyId,
    hotkey: &GlobalHotkey,
) -> Result<BackendGrant, GlobalHotkeyError> {
    record(format!(
        "pending-register {} {}",
        id.id,
        hotkey.to_display_string_for(false)
    ));
    Ok(BackendGrant::Pending)
}

fn pending_backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: "pending",
        register: pending_register,
        needs_loop_polling: true,
        ..fake_backend()
    }
}

/// A backend that refuses everything - the "new" backend a migration moves
/// registrations onto when it cannot take them.
fn refusing_register(
    _id: GlobalHotkeyId,
    _hotkey: &GlobalHotkey,
) -> Result<BackendGrant, GlobalHotkeyError> {
    Err(GlobalHotkeyError::Unavailable("refusing backend".into()))
}

fn refusing_backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: "refusing",
        register: refusing_register,
        ..fake_backend()
    }
}

extern "C" fn callback_a(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

extern "C" fn callback_b(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::RefreshDom
}

/// The data a registration carries, so a delivery can be traced back to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Marker(u32);

fn callback(marker: u32, cb: extern "C" fn(RefAny, CallbackInfo) -> Update) -> CoreMenuCallback {
    CoreMenuCallback {
        refany: RefAny::new(Marker(marker)),
        callback: CoreCallback {
            cb: cb as usize,
            ctx: OptionRefAny::None,
        },
    }
}

fn marker_of(callback: &CoreMenuCallback) -> Option<u32> {
    let mut data = callback.refany.clone();
    let marker = data.downcast_ref::<Marker>().map(|m| m.0);
    marker
}

fn invalid(result: Result<GlobalHotkey, GlobalHotkeyError>) -> bool {
    matches!(result, Err(GlobalHotkeyError::InvalidAccelerator(_)))
}

fn combo(keys: &[K]) -> VirtualKeyCodeCombo {
    VirtualKeyCodeCombo {
        keys: VirtualKeyCodeVec::from_vec(keys.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// 1. The accelerator
// ---------------------------------------------------------------------------

#[test]
fn an_accelerator_parses_into_modifiers_and_one_key() {
    assert_eq!(
        GlobalHotkey::parse_for("Ctrl+Alt+K", false),
        Ok(ctrl_alt(K::K))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Cmd+Shift+K", true),
        Ok(hk(mods(false, false, true, true), K::K))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Ctrl+Shift+F5", false),
        Ok(hk(mods(true, false, true, false), K::F5))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Alt+Space", false),
        Ok(hk(mods(false, true, false, false), K::Space))
    );
}

/// Case, spacing and modifier order are not part of the combination: two
/// spellings of one chord must compare equal, or "already registered" could
/// never be detected.
#[test]
fn an_accelerator_is_normalised() {
    let canonical = hk(mods(true, false, true, false), K::K);
    for spelling in [
        "Ctrl+Shift+K",
        "shift+ctrl+k",
        "  SHIFT + Control +k ",
        "Control+Shift+Shift+K",
    ] {
        assert_eq!(
            GlobalHotkey::parse_for(spelling, false),
            Ok(canonical),
            "{spelling:?} must parse to Ctrl+Shift+K"
        );
    }
    // The aliases name the same keys and modifiers.
    assert_eq!(
        GlobalHotkey::parse_for("Option+Command+Esc", true),
        Ok(hk(mods(false, true, false, true), K::Escape))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Win+Alt+Return", false),
        Ok(hk(mods(false, true, false, true), K::Return))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Ctrl+Alt+ArrowUp", false),
        Ok(ctrl_alt(K::Up))
    );
    assert_eq!(
        GlobalHotkey::parse_for("Ctrl+Alt+-", false),
        Ok(ctrl_alt(K::Minus))
    );
}

/// `CmdOrCtrl` is the platform's primary modifier - the one-definition
/// spelling a cross-platform app wants.
#[test]
fn cmd_or_ctrl_is_the_platform_primary_modifier() {
    assert_eq!(
        GlobalHotkey::parse_for("CmdOrCtrl+Shift+K", true),
        Ok(hk(mods(false, false, true, true), K::K))
    );
    assert_eq!(
        GlobalHotkey::parse_for("CmdOrCtrl+Shift+K", false),
        Ok(hk(mods(true, false, true, false), K::K))
    );
    assert_eq!(HotkeyModifiers::primary_for(true), mods(false, false, false, true));
    assert_eq!(HotkeyModifiers::primary_for(false), mods(true, false, false, false));
}

#[test]
fn a_malformed_accelerator_is_refused_with_a_reason() {
    for bad in [
        "",
        "   ",
        "Ctrl+Alt",        // no key
        "Ctrl+K+J",        // two keys
        "Ctrl+Banana",     // unknown token
        "Ctrl++",          // empty part: the + key is spelled "Plus"
        "Ctrl+Alt+Kana",   // a key no backend can name
        "Ctrl+Alt+LShift", // not a token at all
    ] {
        assert!(
            invalid(GlobalHotkey::parse_for(bad, false)),
            "{bad:?} must be refused as an invalid accelerator, got {:?}",
            GlobalHotkey::parse_for(bad, false)
        );
    }
    // The reason is carried, not just the verdict.
    match GlobalHotkey::parse_for("Ctrl+K+J", false) {
        Err(GlobalHotkeyError::InvalidAccelerator(why)) => assert!(
            why.as_str().contains("ONE key"),
            "the reason must say what is wrong, got {:?}",
            why.as_str()
        ),
        other => panic!("expected InvalidAccelerator, got {other:?}"),
    }
}

/// Grabbing a bare or Shift-only character key would swallow it in every
/// other app for as long as this one runs. Function, media and system keys
/// type nothing and may stand alone.
#[test]
fn a_typing_key_needs_ctrl_alt_or_cmd() {
    for bad in ["K", "Shift+K", "Space", "Shift+1", "Enter", "Left", "Shift+Tab"] {
        assert!(
            invalid(GlobalHotkey::parse_for(bad, false)),
            "{bad:?} would steal typing from every app and must be refused"
        );
    }
    for good in [
        "F13",
        "Shift+F5",
        "PrintScreen",
        "MediaPlayPause",
        "VolumeUp",
        "Pause",
        "Alt+K",
        "Super+K",
    ] {
        assert!(
            GlobalHotkey::parse_for(good, false).is_ok(),
            "{good:?} is a usable global hotkey, got {:?}",
            GlobalHotkey::parse_for(good, false)
        );
    }
    // The rule is on the combination itself, whatever built it.
    assert!(hk(HotkeyModifiers::NONE, K::K).validate().is_err());
    assert!(hk(mods(false, false, true, false), K::K).validate().is_err());
    assert!(hk(mods(false, false, false, true), K::K).validate().is_ok());
    // A modifier or a lock key is never the key.
    assert!(ctrl_alt(K::LShift).validate().is_err());
    assert!(ctrl_alt(K::Capital).validate().is_err());
    assert!(ctrl_alt(K::Numlock).validate().is_err());
}

/// The menu accelerator convention: `LWin` is the PRIMARY modifier (Cmd on a
/// Mac, Ctrl elsewhere), left and right twins are one modifier, and exactly
/// one other code is the key.
#[test]
fn a_menu_combo_converts_with_the_menu_rules() {
    let primary_shift_k = combo(&[K::LWin, K::LShift, K::K]);
    assert_eq!(
        GlobalHotkey::from_combo_for(&primary_shift_k, true),
        Ok(hk(mods(false, false, true, true), K::K))
    );
    assert_eq!(
        GlobalHotkey::from_combo_for(&primary_shift_k, false),
        Ok(hk(mods(true, false, true, false), K::K))
    );
    assert_eq!(
        GlobalHotkey::from_combo_for(&combo(&[K::RControl, K::RAlt, K::K]), false),
        Ok(ctrl_alt(K::K))
    );
    assert!(GlobalHotkey::from_combo_for(&combo(&[K::LControl, K::K, K::J]), false).is_err());
    assert!(GlobalHotkey::from_combo_for(&combo(&[K::LControl, K::LShift]), false).is_err());
    assert!(GlobalHotkey::from_combo_for(&combo(&[K::K]), false).is_err());
}

/// What the app shows the user, and what the parser reads back.
#[test]
fn the_display_string_is_the_platform_spelling_and_round_trips() {
    assert_eq!(
        hk(mods(false, false, true, true), K::K).to_display_string_for(true),
        "Cmd+Shift+K"
    );
    assert_eq!(ctrl_alt(K::K).to_display_string_for(false), "Ctrl+Alt+K");
    assert_eq!(
        hk(mods(true, true, true, true), K::F5).to_display_string_for(true),
        "Cmd+Ctrl+Option+Shift+F5"
    );
    assert_eq!(
        hk(mods(true, true, true, true), K::Space).to_display_string_for(false),
        "Ctrl+Alt+Shift+Super+Space"
    );
    for mac in [true, false] {
        for hotkey in [
            ctrl_alt(K::K),
            hk(mods(false, false, true, true), K::Key7),
            hk(mods(true, false, false, false), K::PageDown),
            hk(HotkeyModifiers::NONE, K::F13),
            hk(mods(false, true, false, false), K::Apostrophe),
            hk(HotkeyModifiers::NONE, K::PlayPause),
        ] {
            let shown = hotkey.to_display_string_for(mac);
            assert_eq!(
                GlobalHotkey::parse_for(&shown, mac),
                Ok(hotkey),
                "{shown:?} (mac={mac}) must parse back to what it shows"
            );
        }
    }
}

/// The X11 backend grabs by `XStringToKeysym(name)`, the Wayland portal takes
/// `preferred_trigger` in the XDG shortcuts format. One table feeds both.
#[test]
fn the_linux_backends_get_xkb_names_and_a_portal_trigger() {
    assert_eq!(xkb_keysym_name(K::K), Some("k"));
    assert_eq!(xkb_keysym_name(K::Key1), Some("1"));
    assert_eq!(xkb_keysym_name(K::F5), Some("F5"));
    assert_eq!(xkb_keysym_name(K::Space), Some("space"));
    assert_eq!(xkb_keysym_name(K::PageUp), Some("Page_Up"));
    assert_eq!(xkb_keysym_name(K::PlayPause), Some("XF86AudioPlay"));
    assert_eq!(xkb_keysym_name(K::Kana), None);

    assert_eq!(
        portal_trigger(&ctrl_alt(K::K)).as_deref(),
        Some("CTRL+ALT+k")
    );
    assert_eq!(
        portal_trigger(&hk(mods(false, false, true, true), K::K)).as_deref(),
        Some("SHIFT+LOGO+k")
    );
    assert_eq!(
        portal_trigger(&hk(mods(true, true, true, true), K::Return)).as_deref(),
        Some("CTRL+ALT+SHIFT+LOGO+Return")
    );
    assert_eq!(
        portal_trigger(&hk(HotkeyModifiers::NONE, K::F13)).as_deref(),
        Some("F13")
    );
}

// ---------------------------------------------------------------------------
// 2. The registry
// ---------------------------------------------------------------------------

#[test]
fn without_a_backend_registration_is_unsupported() {
    let mut r = GlobalHotkeyRegistry::new();
    assert_eq!(
        r.register(ctrl_alt(K::K), callback(1, callback_a)),
        Err(GlobalHotkeyError::Unsupported)
    );
    assert!(!r.has_registrations());
}

#[test]
fn registering_grabs_at_the_backend_and_is_active() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());

    let id = r
        .register(ctrl_alt(K::K), callback(1, callback_a))
        .expect("Ctrl+Alt+K is free");
    assert_ne!(id.id, 0, "0 is reserved for \"no hotkey\"");
    assert_eq!(r.status(id), GlobalHotkeyStatus::Active);
    assert_eq!(r.find(&ctrl_alt(K::K)), Some(id));
    assert!(r.has_registrations());
    assert_eq!(calls(), vec![format!("register {} Ctrl+Alt+K", id.id)]);

    let other = r
        .register(ctrl_alt(K::J), callback(2, callback_a))
        .expect("Ctrl+Alt+J is free");
    assert_ne!(other, id, "two combinations get two ids");
}

/// Registering a combination this app already holds is an error that names
/// the holder - and the backend is not asked again.
#[test]
fn a_duplicate_is_refused_and_names_the_holder() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());

    let first = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    let again = GlobalHotkey::parse_for("alt + ctrl + k", false).unwrap();
    assert_eq!(
        r.register(again, callback(2, callback_b)),
        Err(GlobalHotkeyError::AlreadyRegistered(first))
    );
    assert_eq!(
        calls().len(),
        1,
        "the backend must not be asked for a duplicate: {:?}",
        calls()
    );
    assert_eq!(r.registrations().len(), 1);
}

/// "Another app owns it" comes back as its own error, leaves nothing behind,
/// and does not poison the combination for a later retry.
#[test]
fn a_combination_another_app_owns_is_reported_and_leaves_nothing_behind() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());

    assert_eq!(
        r.register(ctrl_alt(K::T), callback(1, callback_a)),
        Err(GlobalHotkeyError::TakenByAnotherApp)
    );
    assert!(!r.has_registrations());
    assert!(r.registrations().is_empty());
    assert_eq!(r.find(&ctrl_alt(K::T)), None);
    // A retry is asked of the backend again, not answered "duplicate".
    assert_eq!(
        r.register(ctrl_alt(K::T), callback(1, callback_a)),
        Err(GlobalHotkeyError::TakenByAnotherApp)
    );
    assert_eq!(calls().len(), 2);
}

#[test]
fn an_invalid_combination_never_reaches_the_backend() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());
    assert!(matches!(
        r.register(hk(HotkeyModifiers::NONE, K::K), callback(1, callback_a)),
        Err(GlobalHotkeyError::InvalidAccelerator(_))
    ));
    assert!(calls().is_empty(), "the backend was asked: {:?}", calls());
}

#[test]
fn unregistering_releases_the_grab_and_the_id_is_never_reused() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());

    let id = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    assert!(r.unregister(id));
    assert_eq!(r.status(id), GlobalHotkeyStatus::NotRegistered);
    assert!(!r.has_registrations());
    assert!(
        calls().contains(&format!("unregister {}", id.id)),
        "the grab must be released: {:?}",
        calls()
    );
    assert!(!r.unregister(id), "a second unregister is a no-op");

    let again = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    assert_ne!(
        again, id,
        "a stale id must never start meaning a new registration"
    );
    assert_eq!(r.status(again), GlobalHotkeyStatus::Active);
}

/// The portal shape: the backend accepts now and answers later. A refusal
/// keeps the registration readable, with its reason, until unregistered.
#[test]
fn a_pending_registration_is_settled_by_a_later_report() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(pending_backend());

    let id = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    assert_eq!(r.status(id), GlobalHotkeyStatus::Pending);
    assert!(r.has_registrations());
    assert!(r.needs_loop_polling());
    // Still held while pending: a second request is a duplicate.
    assert_eq!(
        r.register(ctrl_alt(K::K), callback(2, callback_a)),
        Err(GlobalHotkeyError::AlreadyRegistered(id))
    );

    r.report(id, Ok(()));
    assert_eq!(r.status(id), GlobalHotkeyStatus::Active);

    let denied = r.register(ctrl_alt(K::J), callback(3, callback_a)).unwrap();
    r.report(denied, Err(GlobalHotkeyError::Denied));
    assert_eq!(
        r.status(denied),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::Denied)
    );
    // A failed registration does not fire, and does not block a retry.
    r.push_fired(denied);
    assert!(r.take_fired().is_empty());
    assert!(r.register(ctrl_alt(K::J), callback(3, callback_a)).is_ok());
}

/// Registrations made before the backend is known (an `App` registering
/// before `run()` turns out headless) move onto the new backend.
#[test]
fn replacing_the_backend_moves_the_registrations() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());
    let id = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();

    let refused = r.set_backend(pending_backend());
    assert!(refused.is_empty());
    assert_eq!(
        calls(),
        vec![
            format!("register {} Ctrl+Alt+K", id.id),
            format!("unregister {}", id.id),
            format!("pending-register {} Ctrl+Alt+K", id.id),
        ]
    );
    assert_eq!(r.status(id), GlobalHotkeyStatus::Pending);

    let refused = r.set_backend(refusing_backend());
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, id);
    assert!(matches!(
        r.status(id),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::Unavailable(_))
    ));
}

// ---------------------------------------------------------------------------
// 3. The drain -> callback delivery
// ---------------------------------------------------------------------------

/// What a backend parks comes back out with the callback AND the data it was
/// registered with, in arrival order, once.
#[test]
fn a_fire_is_delivered_with_its_own_callback_and_data() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());
    let a = r.register(ctrl_alt(K::A), callback(10, callback_a)).unwrap();
    let b = r.register(ctrl_alt(K::B), callback(20, callback_b)).unwrap();

    r.push_fired(b);
    r.push_fired(a);
    r.push_fired(b);
    assert!(r.has_pending_fires());

    let fired = r.take_fired();
    let ids: Vec<GlobalHotkeyId> = fired.iter().map(|f| f.id).collect();
    assert_eq!(ids, vec![b, a, b], "arrival order, repeats kept");
    assert_eq!(fired[0].hotkey, ctrl_alt(K::B));
    assert_eq!(marker_of(&fired[0].callback), Some(20));
    assert_eq!(fired[0].callback.callback.cb, callback_b as usize);
    assert_eq!(marker_of(&fired[1].callback), Some(10));
    assert_eq!(fired[1].callback.callback.cb, callback_a as usize);

    assert!(!r.has_pending_fires());
    assert!(r.take_fired().is_empty(), "delivered once");
}

/// A press that races an unregister must not run the callback of a
/// registration the app already dropped.
#[test]
fn a_fire_for_a_dropped_registration_is_not_delivered() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());
    let id = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    r.push_fired(id);
    assert!(r.unregister(id));
    assert!(r.take_fired().is_empty());

    // An id nothing ever held is dropped too.
    r.push_fired(GlobalHotkeyId { id: 9_999 });
    assert!(r.take_fired().is_empty());
}

#[test]
fn a_stuck_sender_cannot_grow_the_mailbox() {
    let _g = exclusive();
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(fake_backend());
    let id = r.register(ctrl_alt(K::K), callback(1, callback_a)).unwrap();
    for _ in 0..(MAX_PENDING_FIRES * 10) {
        r.push_fired(id);
    }
    let fired = r.take_fired();
    assert_eq!(fired.len(), MAX_PENDING_FIRES);
}

// ---------------------------------------------------------------------------
// 4. The headless simulation
// ---------------------------------------------------------------------------

#[test]
fn a_simulated_press_fires_the_registration_however_it_is_spelled() {
    let mut r = GlobalHotkeyRegistry::new();
    let _ = r.set_backend(registry::simulated_backend());
    let id = r.register(ctrl_alt(K::K), callback(7, callback_a)).unwrap();
    assert_eq!(r.status(id), GlobalHotkeyStatus::Active);

    let spelled = GlobalHotkey::parse_for("alt+CTRL+k", false).unwrap();
    assert!(r.simulate(&spelled));
    let fired = r.take_fired();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].id, id);
    assert_eq!(marker_of(&fired[0].callback), Some(7));

    // A combination nobody registered presses nothing.
    assert!(!r.simulate(&ctrl_alt(K::J)));
    assert!(r.take_fired().is_empty());
}

/// The process-wide API end to end, the way the headless shell and an
/// `AZ_E2E` step drive it: install the simulation, register, press, drain.
#[test]
fn the_process_wide_registry_runs_on_the_simulation() {
    let _g = exclusive();
    let _ = registry::install_backend(registry::simulated_backend());
    assert_eq!(registry::backend_name(), Some(SIMULATED_BACKEND_NAME));
    let probe = registry::probe();
    assert!(probe.available, "{probe:?}");
    assert_eq!(probe.backend, SIMULATED_BACKEND_NAME);

    let hotkey = ctrl_alt(K::F7);
    let id = registry::register(hotkey, callback(42, callback_b)).expect("simulation grants");
    assert_eq!(registry::status(id), GlobalHotkeyStatus::Active);
    assert!(registry::is_registered(id));
    assert!(registry::needs_loop_polling());

    assert!(registry::simulate(&hotkey));
    assert!(registry::has_pending_fires());
    let fired = registry::take_fired();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].id, id);
    assert_eq!(marker_of(&fired[0].callback), Some(42));

    assert!(registry::unregister(id));
    assert!(!registry::is_registered(id));
    assert!(!registry::simulate(&hotkey));
    assert!(registry::take_fired().is_empty());
}
