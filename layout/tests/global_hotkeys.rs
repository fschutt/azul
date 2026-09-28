//! System-wide ("global") hotkeys: the platform-independent half.
//!
//! What is pinned here, bottom to top:
//!
//! 1. **The accelerator** (`azul_core::global_hotkey`): parsing, normalisation (case, spacing,
//!    modifier order, left/right twins, `CmdOrCtrl`), the menu-combo conversion, the rule that a
//!    typing key needs Ctrl/Alt/Cmd, the display string and the xkb / portal trigger names the
//!    Linux backends grab by.
//! 2. **Reconciliation** (`azul_layout::managers::global_hotkey::GlobalHotkeyManager`): every
//!    source (the `AppConfig`, each window's `layout()`) DECLARES the set it wants, and `sync`
//!    makes the OS grabs equal the union - registering what is new, releasing what is gone, and
//!    touching nothing that stayed. An identical re-declaration costs zero backend calls; a new
//!    callback for a kept accelerator is swapped in without one.
//! 3. **Ownership**: one grab per accelerator however many windows declare it, exactly one
//!    callback per press, a window's declaration shadows the app's, and among windows the most
//!    recently focused declarer wins, then the oldest window.
//! 4. **Status feedback**: failures are sticky until a retry (so a fallback converges instead of
//!    looping), a pending (portal) grab settles from another thread through the sink, and only
//!    the sources that READ a status are asked to lay out again.
//! 5. **The mailbox**: bounded, and a press for an accelerator released meanwhile is dropped.
//!
//! Every test builds its own manager with its own recording backend, so nothing here is shared
//! between tests and no test needs a lock. The OS backends (Carbon, Win32, X11, the portal)
//! cannot run here; their manual check recipes are in `scripts/GLOBAL_HOTKEYS_2026_09_28.md`.

#![cfg(feature = "text_layout")]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use azul_core::{
    callbacks::{CoreCallback, Update},
    global_hotkey::{
        portal_trigger, xkb_keysym_name, GlobalHotkey, GlobalHotkeyCallbackData,
        GlobalHotkeyError, GlobalHotkeyId, GlobalHotkeyOwner, GlobalHotkeyState,
        GlobalHotkeyStatus, HotkeyModifiers,
    },
    refany::{OptionRefAny, RefAny},
    window::{VirtualKeyCode as K, VirtualKeyCodeCombo, VirtualKeyCodeVec},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::CallbackInfo,
    managers::global_hotkey::{
        BackendEvent, BackendGrant, GlobalHotkeyBackend, GlobalHotkeyManager, HotkeyDelivery,
        HotkeySource, SharedGlobalHotkeys, MAX_PENDING_FIRES, SIMULATED_BACKEND_NAME,
    },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn invalid(result: Result<GlobalHotkey, GlobalHotkeyError>) -> bool {
    matches!(result, Err(GlobalHotkeyError::InvalidAccelerator(_)))
}

fn combo(keys: &[K]) -> VirtualKeyCodeCombo {
    VirtualKeyCodeCombo {
        keys: VirtualKeyCodeVec::from_vec(keys.to_vec()),
    }
}

/// What the recording backend was asked, in order: `register Ctrl+Alt+K`,
/// `unregister Ctrl+Alt+K`, `commit`. Per test, never shared.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    fn push(&self, entry: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }

    /// Everything since the last `take`.
    fn take(&self) -> Vec<String> {
        core::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Everything since the last `take`, without the batch `commit`s.
    fn grabs(&self) -> Vec<String> {
        self.take().into_iter().filter(|e| e != "commit").collect()
    }
}

/// A backend that grants what it is asked and writes every call to its
/// `Log`. Ctrl+Alt+T is "taken by another application" (the GNOME / Ubuntu
/// terminal shortcut); `pending` answers like the Wayland portal.
struct RecordingBackend {
    log: Log,
    pending: bool,
    /// OS id -> accelerator, so `unregister` can say what it released.
    ids: Arc<Mutex<BTreeMap<u32, GlobalHotkey>>>,
}

impl RecordingBackend {
    fn new(log: &Log) -> Self {
        Self {
            log: log.clone(),
            pending: false,
            ids: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn pending(log: &Log) -> Self {
        Self {
            pending: true,
            ..Self::new(log)
        }
    }
}

fn name(hotkey: &GlobalHotkey) -> String {
    hotkey.to_display_string_for(false)
}

impl GlobalHotkeyBackend for RecordingBackend {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn probe(&self) -> Result<(), String> {
        Ok(())
    }

    fn register(
        &mut self,
        os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        _description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        self.log.push(format!("register {}", name(hotkey)));
        if *hotkey == ctrl_alt(K::T) {
            return Err(GlobalHotkeyError::TakenByAnotherApp);
        }
        self.ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(os_id.id, *hotkey);
        Ok(if self.pending {
            BackendGrant::Pending
        } else {
            BackendGrant::Active
        })
    }

    fn unregister(&mut self, os_id: GlobalHotkeyId) {
        let released = self
            .ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&os_id.id);
        self.log.push(format!(
            "unregister {}",
            released.as_ref().map_or_else(|| "?".to_string(), name)
        ));
    }

    fn commit(&mut self) {
        self.log.push("commit".to_string());
    }
}

/// A manager with a recording backend installed, and the backend's log.
fn manager() -> (GlobalHotkeyManager, Log) {
    let log = Log::default();
    let mut m = GlobalHotkeyManager::new();
    m.install_backend(Box::new(RecordingBackend::new(&log)));
    (m, log)
}

extern "C" fn callback_a(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

extern "C" fn callback_b(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::RefreshDom
}

/// The data a declaration carries, so a delivery can be traced back to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Marker(u32);

fn declared(
    hotkey: GlobalHotkey,
    marker: u32,
    cb: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> GlobalHotkeyCallbackData {
    GlobalHotkeyCallbackData {
        hotkey,
        description: AzString::from_const_str(""),
        callback: CoreCallback {
            cb: cb as usize,
            ctx: OptionRefAny::None,
        },
        refany: RefAny::new(Marker(marker)),
    }
}

fn marker_of(delivery: &HotkeyDelivery) -> Option<u32> {
    let mut data = delivery.callback.refany.clone();
    let marker = data.downcast_ref::<Marker>().map(|m| m.0);
    marker
}

const W1: HotkeySource = HotkeySource::Window(1);
const W2: HotkeySource = HotkeySource::Window(2);

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
// 2. Reconciliation: declare the whole wanted set, sync the OS to it
// ---------------------------------------------------------------------------

#[test]
fn declaring_registers_each_new_accelerator_once() {
    let (mut m, log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::A), 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+A"]);
    assert_eq!(m.status(&ctrl_alt(K::A)), GlobalHotkeyStatus::Active);

    m.declare(
        W1,
        vec![
            declared(ctrl_alt(K::A), 1, callback_a),
            declared(ctrl_alt(K::B), 2, callback_a),
        ],
        false,
    );
    let _ = m.sync();
    assert_eq!(
        log.grabs(),
        vec!["register Ctrl+Alt+B"],
        "the accelerator that stayed must not be grabbed again"
    );
}

/// THE core property: `layout()` re-runs on every `RefreshDom`, so a
/// re-declaration of the same set is the common case and must cost nothing
/// at the OS - not a release + grab, not even a commit. Order and fresh
/// `RefAny`s do not make it a different set.
#[test]
fn an_identical_redeclaration_calls_nothing() {
    let (mut m, log) = manager();
    m.declare(
        W1,
        vec![
            declared(ctrl_alt(K::A), 1, callback_a),
            declared(ctrl_alt(K::B), 2, callback_a),
        ],
        false,
    );
    let _ = m.sync();
    let _ = log.take();

    m.declare(
        W1,
        vec![
            declared(ctrl_alt(K::B), 20, callback_b),
            declared(ctrl_alt(K::A), 10, callback_b),
        ],
        false,
    );
    let outcome = m.sync();
    assert_eq!(log.take(), Vec::<String>::new());
    assert!(!outcome.changed, "nothing about any status moved");
}

#[test]
fn a_new_callback_for_a_kept_accelerator_swaps_without_a_backend_call_and_the_next_press_runs_it()
{
    let (mut m, log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    let _ = log.take();

    m.declare(W1, vec![declared(ctrl_alt(K::K), 2, callback_b)], false);
    let _ = m.sync();
    assert_eq!(log.take(), Vec::<String>::new());

    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(marker_of(&deliveries[0]), Some(2), "the NEW data runs");
    assert_eq!(
        deliveries[0].callback.callback.cb, callback_b as usize,
        "the NEW callback runs"
    );
    assert_eq!(deliveries[0].event.hotkey, ctrl_alt(K::K));
    assert_eq!(deliveries[0].event.state, GlobalHotkeyState::Pressed);
}

/// A press that races the undeclaration must not run the callback of an
/// accelerator the app no longer wants.
#[test]
fn an_undeclared_accelerator_is_released_and_its_queued_press_is_dropped() {
    let (mut m, log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    let _ = log.take();

    assert!(m.simulate(&ctrl_alt(K::K)));
    m.declare(W1, Vec::new(), false);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["unregister Ctrl+Alt+K"]);
    assert_eq!(m.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::NotRegistered);
    assert!(m.take_deliveries().is_empty());
    assert!(!m.simulate(&ctrl_alt(K::K)), "nothing holds it any more");
}

/// Switching A for B inside one pass never holds both - some platforms cap
/// how many an app may hold, and a user rebinding a key expects the old one
/// to be free the moment the new one is taken.
#[test]
fn a_release_comes_before_a_grab_in_one_batch() {
    let (mut m, log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::A), 1, callback_a)], false);
    let _ = m.sync();
    let _ = log.take();

    m.declare(W1, vec![declared(ctrl_alt(K::B), 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(
        log.take(),
        vec!["unregister Ctrl+Alt+A", "register Ctrl+Alt+B", "commit"]
    );
}

/// The portal binds a whole batch in ONE session (one approval dialog), so a
/// batch must end in exactly one `commit`, after every grab of the batch.
#[test]
fn one_batch_commits_once() {
    let (mut m, log) = manager();
    m.declare(
        W1,
        vec![
            declared(ctrl_alt(K::A), 1, callback_a),
            declared(ctrl_alt(K::B), 2, callback_a),
            declared(ctrl_alt(K::C), 3, callback_a),
        ],
        false,
    );
    let _ = m.sync();
    assert_eq!(
        log.take(),
        vec![
            "register Ctrl+Alt+A",
            "register Ctrl+Alt+B",
            "register Ctrl+Alt+C",
            "commit"
        ]
    );
}

#[test]
fn an_invalid_accelerator_never_reaches_the_backend() {
    let (mut m, log) = manager();
    let bare_k = hk(HotkeyModifiers::NONE, K::K);
    m.declare(W1, vec![declared(bare_k, 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.take(), Vec::<String>::new());
    assert!(matches!(
        m.status(&bare_k),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::InvalidAccelerator(_))
    ));
}

// ---------------------------------------------------------------------------
// 3. Several sources: one grab, one callback per press, the owner rule
// ---------------------------------------------------------------------------

/// N windows running the same `layout()` share ONE grab; it survives until
/// the last declarer is gone.
#[test]
fn two_windows_share_one_grab_forgetting_one_keeps_it_forgetting_both_releases_it() {
    let (mut m, log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    m.declare(W2, vec![declared(ctrl_alt(K::K), 2, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+K"]);

    m.forget_source(W1);
    let _ = m.sync();
    assert_eq!(log.take(), Vec::<String>::new(), "window 2 still wants it");
    assert_eq!(m.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::Active);

    m.forget_source(W2);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["unregister Ctrl+Alt+K"]);
}

/// Exactly one callback runs per press - a summon key in a three-window app
/// must not summon three times - and it is the one of the window the user
/// last worked in.
#[test]
fn the_most_recently_focused_declarer_runs() {
    let (mut m, _log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    m.declare(W2, vec![declared(ctrl_alt(K::K), 2, callback_a)], false);
    let _ = m.sync();

    m.note_focus(2);
    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries.len(), 1, "one press, one callback");
    assert_eq!(deliveries[0].target, W2);
    assert_eq!(marker_of(&deliveries[0]), Some(2));

    m.note_focus(1);
    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries[0].target, W1);
    assert_eq!(marker_of(&deliveries[0]), Some(1));
}

#[test]
fn an_unfocused_tie_goes_to_the_oldest_window() {
    let (mut m, _log) = manager();
    // Declared in the "wrong" order on purpose: age is the window's
    // sequence number, not who declared first.
    m.declare(W2, vec![declared(ctrl_alt(K::K), 2, callback_a)], false);
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].target, W1);
}

/// The more specific context wins: a window that declares the accelerator
/// runs it even though the `AppConfig` declares it too.
#[test]
fn a_window_declaration_shadows_the_app_one() {
    let (mut m, log) = manager();
    m.declare(
        HotkeySource::App,
        vec![declared(ctrl_alt(K::K), 0, callback_a)],
        false,
    );
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+K"], "still one grab");

    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].target, W1);

    // The window goes away: the app's declaration takes over, no OS churn.
    m.forget_source(W1);
    let _ = m.sync();
    assert_eq!(log.take(), Vec::<String>::new());
    assert!(m.simulate(&ctrl_alt(K::K)));
    let deliveries = m.take_deliveries();
    assert_eq!(deliveries[0].target, HotkeySource::App);
    assert_eq!(marker_of(&deliveries[0]), Some(0));
}

/// What `get_global_hotkeys()` reports, from one window's point of view.
#[test]
fn the_info_list_names_the_owner_relative_to_the_viewer() {
    let (mut m, _log) = manager();
    m.declare(
        HotkeySource::App,
        vec![declared(ctrl_alt(K::A), 0, callback_a)],
        false,
    );
    m.declare(W1, vec![declared(ctrl_alt(K::B), 1, callback_a)], false);
    m.declare(
        W2,
        vec![
            declared(ctrl_alt(K::C), 2, callback_a),
            declared(ctrl_alt(K::T), 2, callback_a),
        ],
        false,
    );
    let _ = m.sync();
    // Ctrl+Alt+T was refused; once nobody declares it the failure is still
    // listed, with nobody as its owner.
    m.declare(W2, vec![declared(ctrl_alt(K::C), 2, callback_a)], false);
    let _ = m.sync();

    let infos = m.infos_for(W1);
    let owner = |key: K| {
        infos
            .iter()
            .find(|i| i.hotkey == ctrl_alt(key))
            .map(|i| i.owner)
    };
    assert_eq!(owner(K::A), Some(GlobalHotkeyOwner::App));
    assert_eq!(owner(K::B), Some(GlobalHotkeyOwner::ThisWindow));
    assert_eq!(owner(K::C), Some(GlobalHotkeyOwner::OtherWindow));
    assert_eq!(owner(K::T), Some(GlobalHotkeyOwner::Nobody));
    let b = infos.iter().find(|i| i.hotkey == ctrl_alt(K::B)).unwrap();
    assert_eq!(b.status, GlobalHotkeyStatus::Active);
    assert_eq!(b.trigger.as_str(), ctrl_alt(K::B).to_display_string().as_str());
}

// ---------------------------------------------------------------------------
// 4. Status feedback
// ---------------------------------------------------------------------------

/// Taken -> re-declared -> NOT asked again (declared or not) -> retry -> asked
/// once. Without this, "Ctrl+Alt+T is taken, show a warning" would hammer
/// the OS on every `RefreshDom`, and on Wayland re-show a declined dialog.
#[test]
fn a_failure_is_sticky_until_a_retry() {
    let (mut m, log) = manager();
    let t = ctrl_alt(K::T);
    m.declare(W1, vec![declared(t, 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+T"]);
    assert_eq!(
        m.status(&t),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::TakenByAnotherApp)
    );

    m.declare(W1, vec![declared(t, 1, callback_a)], false);
    let _ = m.sync();
    m.declare(W1, Vec::new(), false);
    let _ = m.sync();
    m.declare(W1, vec![declared(t, 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(log.take(), Vec::<String>::new(), "a failure is not re-asked");
    assert_eq!(
        m.status(&t),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::TakenByAnotherApp)
    );

    m.retry(t);
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+T"], "a retry asks once");
}

/// "Ctrl+Alt+T is taken, use Ctrl+Alt+J": with failures forgotten on
/// undeclare this loops forever (T fails, J is declared, T reads
/// NotRegistered, T is declared again, ...). Sticky, it converges.
#[test]
fn a_fallback_after_a_failure_converges() {
    let (mut m, log) = manager();
    let t = ctrl_alt(K::T);
    let j = ctrl_alt(K::J);
    m.declare(W1, vec![declared(t, 1, callback_a)], true);
    let _ = m.sync();
    assert!(matches!(m.status(&t), GlobalHotkeyStatus::Failed(_)));

    // The app's layout() read "failed" and fell back.
    m.declare(W1, vec![declared(j, 1, callback_a)], true);
    let _ = m.sync();
    assert!(
        matches!(m.status(&t), GlobalHotkeyStatus::Failed(_)),
        "the first choice must still read failed, or the app flips back"
    );
    assert_eq!(m.status(&j), GlobalHotkeyStatus::Active);
    let _ = log.take();

    // One more pass (the status-driven relayout) declares the same: done.
    m.declare(W1, vec![declared(j, 1, callback_a)], true);
    let outcome = m.sync();
    assert_eq!(log.take(), Vec::<String>::new());
    assert!(outcome.relayout.is_empty(), "converged: nobody is asked again");
}

#[test]
fn without_a_backend_every_declaration_reads_unsupported_and_nothing_is_called() {
    let mut m = GlobalHotkeyManager::new();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    assert_eq!(
        m.status(&ctrl_alt(K::K)),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::Unsupported)
    );
    assert!(!m.simulate(&ctrl_alt(K::K)));
    assert!(m.take_deliveries().is_empty());
}

/// Declarations made before the run chose its backend (an `AppConfig` list,
/// a first layout on a slow start) reach the backend once it is installed -
/// and never a different one before it.
#[test]
fn declarations_before_a_backend_wait_for_it() {
    let mut m = GlobalHotkeyManager::new();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();

    let log = Log::default();
    m.install_backend(Box::new(RecordingBackend::new(&log)));
    let _ = m.sync();
    assert_eq!(log.grabs(), vec!["register Ctrl+Alt+K"]);
    assert_eq!(m.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::Active);
}

/// A new backend (the headless simulation replacing the platform's) takes
/// the whole set over: the old grabs are released, the new ones taken.
#[test]
fn replacing_the_backend_moves_every_grab() {
    let (mut m, old_log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    let _ = old_log.take();

    let new_log = Log::default();
    m.install_backend(Box::new(RecordingBackend::new(&new_log)));
    assert_eq!(old_log.grabs(), vec!["unregister Ctrl+Alt+K"]);
    let _ = m.sync();
    assert_eq!(new_log.grabs(), vec!["register Ctrl+Alt+K"]);
}

/// The portal shape: the grab is `Pending` until the desktop answers on
/// ANOTHER thread, which reaches the manager only through the sink. Only
/// the sources whose last pass READ a status are asked to lay out again.
#[test]
fn a_pending_grab_settles_from_another_thread_through_the_sink() {
    let log = Log::default();
    let mut m = GlobalHotkeyManager::new();
    let backend = RecordingBackend::pending(&log);
    let ids = backend.ids.clone();
    m.install_backend(Box::new(backend));

    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], true);
    m.declare(W2, vec![declared(ctrl_alt(K::J), 2, callback_a)], false);
    let _ = m.sync();
    assert_eq!(m.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::Pending);

    let os_id = ids
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(_, h)| **h == ctrl_alt(K::K))
        .map(|(id, _)| GlobalHotkeyId { id: *id })
        .expect("the backend was asked for Ctrl+Alt+K");
    let sink = m.sink();
    std::thread::spawn(move || {
        sink.push(BackendEvent::Settled {
            os_id,
            result: Ok(AzString::from("Ctrl+Alt+K (as the desktop shows it)")),
        });
    })
    .join()
    .expect("the answering thread");

    let outcome = m.sync();
    assert!(outcome.changed);
    assert_eq!(m.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::Active);
    assert_eq!(
        outcome.relayout,
        vec![W1],
        "window 1 read a status, window 2 did not"
    );
    let infos = m.infos_for(W1);
    let k = infos.iter().find(|i| i.hotkey == ctrl_alt(K::K)).unwrap();
    assert_eq!(
        k.trigger.as_str(),
        "Ctrl+Alt+K (as the desktop shows it)",
        "the desktop's own spelling of the trigger is kept"
    );
}

#[test]
fn a_refusal_that_arrives_later_is_sticky_too() {
    let log = Log::default();
    let mut m = GlobalHotkeyManager::new();
    let backend = RecordingBackend::pending(&log);
    let ids = backend.ids.clone();
    m.install_backend(Box::new(backend));
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], true);
    let _ = m.sync();
    let os_id = ids
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .keys()
        .next()
        .map(|id| GlobalHotkeyId { id: *id })
        .expect("asked");
    m.sink().push(BackendEvent::Settled {
        os_id,
        result: Err(GlobalHotkeyError::Denied),
    });
    let _ = m.sync();
    assert_eq!(
        m.status(&ctrl_alt(K::K)),
        GlobalHotkeyStatus::Failed(GlobalHotkeyError::Denied)
    );
    let _ = log.take();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], true);
    let _ = m.sync();
    assert_eq!(
        log.take(),
        Vec::<String>::new(),
        "a declined dialog must never be re-shown by an ordinary relayout"
    );
}

// ---------------------------------------------------------------------------
// 5. The mailbox and the simulation
// ---------------------------------------------------------------------------

#[test]
fn a_stuck_sender_cannot_grow_the_sink() {
    let (mut m, _log) = manager();
    m.declare(W1, vec![declared(ctrl_alt(K::K), 1, callback_a)], false);
    let _ = m.sync();
    for _ in 0..(MAX_PENDING_FIRES * 10) {
        assert!(m.simulate(&ctrl_alt(K::K)));
    }
    assert_eq!(m.take_deliveries().len(), MAX_PENDING_FIRES);
    assert!(m.take_deliveries().is_empty(), "delivered once");
}

#[test]
fn a_simulated_press_reaches_the_current_owner_however_it_is_spelled() {
    let shared = SharedGlobalHotkeys::new();
    shared.install_simulated_backend();
    assert_eq!(shared.backend_name(), Some(SIMULATED_BACKEND_NAME));
    shared.declare(W1, vec![declared(ctrl_alt(K::K), 7, callback_a)], false);
    let _ = shared.sync();
    assert_eq!(shared.status(&ctrl_alt(K::K)), GlobalHotkeyStatus::Active);

    let spelled = GlobalHotkey::parse_for("alt+CTRL+k", false).unwrap();
    assert!(shared.simulate(&spelled));
    let deliveries = shared.lock().take_deliveries();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].target, W1);
    assert_eq!(marker_of(&deliveries[0]), Some(7));

    assert!(!shared.simulate(&ctrl_alt(K::J)), "nobody declares it");
}
