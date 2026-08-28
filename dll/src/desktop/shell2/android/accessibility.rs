//! Android accessibility bridge — `AccessibilityNodeProvider` ↔ Azul.
//!
//! `accesskit` has adapters for Windows, macOS and Unix; it has NONE for
//! Android. Android's own model is a **virtual view hierarchy**: a single real
//! `View` implements `getAccessibilityNodeProvider()` and vends
//! `AccessibilityNodeInfo`s for virtual children addressed by `int` id. That is
//! exactly the situation a self-drawing UI toolkit is in, so it is what this
//! bridge implements.
//!
//! ```text
//!  TalkBack ── AccessibilityNodeProvider (Java, AzulAccessibilityBridge)
//!                   │ nativeGetNodeCount / nativeDescribeNode
//!                   │ nativePerformAction
//!                   ▼
//!             AndroidAccessibilityAdapter (this file)
//!                   │ A11ySnapshot (labels/bounds/actions)
//!                   │ A11yActionQueue (inbound actions)
//!                   ▼
//!             AndroidWindow::process_accessibility_actions()  (loop thread)
//! ```
//!
//! # Threading
//!
//! `performAction` and `createAccessibilityNodeInfo` are called on the Java UI
//! thread (or on the accessibility thread), while `android_main` drives the
//! `LayoutWindow` on its own native thread. Nothing here touches the
//! `LayoutWindow`: reads answer from an owned [`A11ySnapshot`] behind a lock,
//! and actions are pushed onto the `Arc<Mutex<..>>`-backed [`A11yActionQueue`]
//! for the loop thread to drain. Calling into the layout window from the JNI
//! upcall would be a data race with the frame loop.
//!
//! # Wire format
//!
//! `nativeDescribeNode` returns ONE `String` per node rather than a dozen
//! accessor natives, because every extra JNI round trip is paid on the UI
//! thread while TalkBack waits. Fields are separated by `\u{1}` (a codepoint no
//! label can contain — `AccessibilityNodeInfo` text is human-readable):
//!
//! ```text
//! label ⟂ value ⟂ className ⟂ left ⟂ top ⟂ right ⟂ bottom ⟂ actions ⟂ flags ⟂ childIds
//! ```
//!
//! Bounds are PHYSICAL pixels, view-local; the Java side adds the view's screen
//! offset for `setBoundsInScreen`. `actions` and `flags` are the bitmasks
//! declared below — deliberately Azul's own, NOT
//! `AccessibilityNodeInfo.ACTION_*`. The Java side translates. Neither half
//! then hard-codes the other platform's numbers, so a framework constant
//! changing value cannot silently retarget an action.

#[cfg(feature = "a11y")]
use std::sync::{Arc, Mutex};

use azul_core::dom::{AccessibilityAction, DomId, NodeId};
#[cfg(feature = "a11y")]
use azul_layout::managers::a11y_snapshot::A11ySnapshot;

use crate::desktop::shell2::common::accessibility::A11yActionQueue;

/// Field separator in the `nativeDescribeNode` wire format.
pub const FIELD_SEP: char = '\u{1}';

/// The virtual view id Android uses for "the host View itself".
/// (`AccessibilityNodeProvider.HOST_VIEW_ID`, which is `-1`.)
pub const HOST_VIEW_ID: i32 = -1;

// ─── Action bitmask (Rust → Java) ─────────────────────────────────────
//
// Azul's own bits. `AzulAccessibilityBridge.java` maps each onto the matching
// `AccessibilityNodeInfo.ACTION_*` constant.

pub const ACT_CLICK: i32 = 1 << 0;
pub const ACT_FOCUS: i32 = 1 << 1;
pub const ACT_CLEAR_FOCUS: i32 = 1 << 2;
pub const ACT_SCROLL_FORWARD: i32 = 1 << 3;
pub const ACT_SCROLL_BACKWARD: i32 = 1 << 4;

// ─── Node flags (Rust → Java) ─────────────────────────────────────────

pub const FLAG_FOCUSABLE: i32 = 1 << 0;
pub const FLAG_FOCUSED: i32 = 1 << 1;
pub const FLAG_ENABLED: i32 = 1 << 2;
pub const FLAG_CHECKABLE: i32 = 1 << 3;
pub const FLAG_CHECKED: i32 = 1 << 4;
pub const FLAG_EDITABLE: i32 = 1 << 5;
pub const FLAG_CLICKABLE: i32 = 1 << 6;
pub const FLAG_SCROLLABLE: i32 = 1 << 7;

// ─── Verb ids (Java → Rust) ───────────────────────────────────────────
//
// What `performAction` was asked to do, already translated out of Android's
// constants by the Java side.

pub const VERB_CLICK: i32 = 0;
pub const VERB_FOCUS: i32 = 1;
pub const VERB_CLEAR_FOCUS: i32 = 2;
pub const VERB_SCROLL_FORWARD: i32 = 3;
pub const VERB_SCROLL_BACKWARD: i32 = 4;
pub const VERB_A11Y_FOCUS: i32 = 5;
pub const VERB_CLEAR_A11Y_FOCUS: i32 = 6;

/// The Android side of the accessibility bridge.
///
/// Shared with the JNI upcalls through `Arc<Mutex<..>>` on the snapshot, since
/// those run on a different thread from the frame loop that replaces it.
#[cfg(feature = "a11y")]
pub struct AndroidAccessibilityAdapter {
    /// Actions TalkBack requested, drained by the frame loop.
    queue: A11yActionQueue,
    /// What the Java side is currently being shown. Behind a `Mutex` because
    /// the reader is the UI thread and the writer is the loop thread.
    snapshot: Arc<Mutex<A11ySnapshot>>,
    /// Logical → physical scale (`density / 160`, which equals `dpi / 96` given
    /// how `android_main` derives the framework dpi). Android wants physical
    /// pixels; the snapshot carries logical units.
    scale: f32,
}

#[cfg(feature = "a11y")]
impl AndroidAccessibilityAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: A11yActionQueue::new(),
            snapshot: Arc::new(Mutex::new(A11ySnapshot::default())),
            scale: 1.0,
        }
    }

    /// Pop one queued action. Same signature as the desktop adapters'
    /// `poll_action`, so the frame pumps are identical.
    #[must_use]
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        self.queue.poll_action()
    }

    /// Replace the tree the Java side reads. Called after every layout.
    pub fn update_snapshot(&mut self, snapshot: A11ySnapshot, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if let Ok(mut guard) = self.snapshot.lock() {
            *guard = snapshot;
        }
    }

    /// Number of virtual views, i.e. exposed DOM nodes.
    #[must_use]
    pub fn node_count(&self) -> i32 {
        self.snapshot
            .lock()
            .map_or(0, |s| i32::try_from(s.len()).unwrap_or(i32::MAX))
    }

    /// Serialise one node for `createAccessibilityNodeInfo`.
    ///
    /// `virtual_view_id == HOST_VIEW_ID` describes the host View: no label of
    /// its own, children are the snapshot roots. TalkBack needs that node to
    /// exist or the whole subtree is unreachable.
    ///
    /// Returns `None` for an id that is not in the current snapshot — a stale
    /// id from before the last layout. Java answers `null`, which is the
    /// documented way to say "that virtual view is gone".
    #[must_use]
    pub fn describe_node(&self, virtual_view_id: i32) -> Option<String> {
        let guard = self.snapshot.lock().ok()?;

        if virtual_view_id == HOST_VIEW_ID {
            let children = guard
                .roots
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let w = (guard.window_size.width * self.scale).round() as i32;
            let h = (guard.window_size.height * self.scale).round() as i32;
            return Some(join_fields(&[
                guard.title.clone(),
                String::new(),
                "android.view.View".to_string(),
                "0".to_string(),
                "0".to_string(),
                w.to_string(),
                h.to_string(),
                "0".to_string(),
                FLAG_ENABLED.to_string(),
                children,
            ]));
        }

        let index = usize::try_from(virtual_view_id).ok()?;
        let element = guard.element(index)?;

        let px = |v: f32| (v * self.scale).round() as i32;
        let left = px(element.bounds.origin.x);
        let top = px(element.bounds.origin.y);
        let right = px(element.bounds.origin.x + element.bounds.size.width);
        let bottom = px(element.bounds.origin.y + element.bounds.size.height);

        let mut actions = 0;
        if element.supports(&AccessibilityAction::Default) {
            actions |= ACT_CLICK;
        }
        if element.supports(&AccessibilityAction::Focus) {
            actions |= ACT_FOCUS;
        }
        if element.supports(&AccessibilityAction::Blur) {
            actions |= ACT_CLEAR_FOCUS;
        }
        if element.supports(&AccessibilityAction::ScrollDown)
            || element.supports(&AccessibilityAction::ScrollRight)
        {
            actions |= ACT_SCROLL_FORWARD;
        }
        if element.supports(&AccessibilityAction::ScrollUp)
            || element.supports(&AccessibilityAction::ScrollLeft)
        {
            actions |= ACT_SCROLL_BACKWARD;
        }

        let mut flags = 0;
        if element.focusable {
            flags |= FLAG_FOCUSABLE;
        }
        if element.focused {
            flags |= FLAG_FOCUSED;
        }
        if !element.disabled {
            flags |= FLAG_ENABLED;
        }
        if element.checked.is_some() {
            flags |= FLAG_CHECKABLE;
        }
        if element.checked == Some(true) {
            flags |= FLAG_CHECKED;
        }
        if element.editable {
            flags |= FLAG_EDITABLE;
        }
        if actions & ACT_CLICK != 0 {
            flags |= FLAG_CLICKABLE;
        }
        if actions & (ACT_SCROLL_FORWARD | ACT_SCROLL_BACKWARD) != 0 {
            flags |= FLAG_SCROLLABLE;
        }

        let children = element
            .children
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");

        Some(join_fields(&[
            sanitize(&element.label),
            element.value.as_deref().map(sanitize).unwrap_or_default(),
            class_name_for(element).to_string(),
            left.to_string(),
            top.to_string(),
            right.to_string(),
            bottom.to_string(),
            actions.to_string(),
            flags.to_string(),
            children,
        ]))
    }

    /// Translate a `performAction` verb into an Azul action and queue it.
    ///
    /// Returns whether the action was accepted; Java returns that straight to
    /// the framework, which is how TalkBack knows whether to announce a
    /// failure. Answering `true` for something the engine will drop would tell
    /// the user an action happened when none did.
    ///
    /// The scroll verbs are direction-agnostic in Android (`SCROLL_FORWARD` /
    /// `SCROLL_BACKWARD`), so they resolve against the element's own declared
    /// axis: a vertically scrollable node scrolls down, a horizontally
    /// scrollable one scrolls right.
    pub fn perform_action(&self, virtual_view_id: i32, verb: i32) -> bool {
        let Ok(guard) = self.snapshot.lock() else {
            return false;
        };
        let Ok(index) = usize::try_from(virtual_view_id) else {
            // HOST_VIEW_ID or garbage: no DOM node stands behind it.
            return false;
        };
        let Some(element) = guard.element(index) else {
            return false;
        };

        let action = match verb {
            VERB_CLICK => AccessibilityAction::Default,
            // Android's "accessibility focus" (the green TalkBack cursor) is a
            // separate concept from input focus, but azul has one focus model,
            // and a screen-reader user expects the focused node to be the one
            // the engine considers focused. Both map to Focus.
            VERB_FOCUS | VERB_A11Y_FOCUS => AccessibilityAction::Focus,
            VERB_CLEAR_FOCUS | VERB_CLEAR_A11Y_FOCUS => AccessibilityAction::Blur,
            VERB_SCROLL_FORWARD => {
                if element.supports(&AccessibilityAction::ScrollDown) {
                    AccessibilityAction::ScrollDown
                } else {
                    AccessibilityAction::ScrollRight
                }
            }
            VERB_SCROLL_BACKWARD => {
                if element.supports(&AccessibilityAction::ScrollUp) {
                    AccessibilityAction::ScrollUp
                } else {
                    AccessibilityAction::ScrollLeft
                }
            }
            _ => return false,
        };

        if !element.supports(&action) {
            return false;
        }
        self.queue.push(element.dom_id, element.node_id, action);
        true
    }
}

#[cfg(feature = "a11y")]
impl Default for AndroidAccessibilityAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// The Android widget class name TalkBack announces for an element.
///
/// TalkBack derives most of its phrasing from this string ("Button",
/// "double-tap to activate"), so it is not cosmetic — a `<button>` announced as
/// `android.view.View` gets no activation hint at all.
#[cfg(feature = "a11y")]
fn class_name_for(element: &azul_layout::managers::a11y_snapshot::A11yElement) -> &'static str {
    use azul_core::dom::AccessibilityRole;
    match element.role {
        AccessibilityRole::PushButton
        | AccessibilityRole::SplitButton
        | AccessibilityRole::ButtonDropdown
        | AccessibilityRole::ButtonMenu => "android.widget.Button",
        AccessibilityRole::CheckButton => "android.widget.CheckBox",
        AccessibilityRole::RadioButton => "android.widget.RadioButton",
        AccessibilityRole::Link | AccessibilityRole::StaticText => "android.widget.TextView",
        AccessibilityRole::Text => "android.widget.EditText",
        AccessibilityRole::Graphic | AccessibilityRole::Diagram => "android.widget.ImageView",
        AccessibilityRole::Slider => "android.widget.SeekBar",
        AccessibilityRole::List => "android.widget.ListView",
        AccessibilityRole::ProgressBar => "android.widget.ProgressBar",
        _ => "android.view.View",
    }
}

/// Strip the field separator (and control characters that would confuse the
/// Java parser) out of user-controlled text.
///
/// The DOM's text is app data, and an app that puts `\u{1}` in a label must not
/// be able to shift every later field of the record — that would misreport
/// bounds and actions for the node, which is a correctness bug, not a
/// formatting one.
#[cfg(feature = "a11y")]
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if (c as u32) < 0x20 { ' ' } else { c })
        .collect()
}

#[cfg(feature = "a11y")]
fn join_fields(fields: &[String]) -> String {
    fields.join(&FIELD_SEP.to_string())
}

// ─── Stub when the `a11y` feature is off ──────────────────────────────

#[cfg(not(feature = "a11y"))]
pub struct AndroidAccessibilityAdapter {
    queue: A11yActionQueue,
}

#[cfg(not(feature = "a11y"))]
impl AndroidAccessibilityAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: A11yActionQueue::new(),
        }
    }
    #[must_use]
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        self.queue.poll_action()
    }
    #[must_use]
    pub fn node_count(&self) -> i32 {
        0
    }
    #[must_use]
    pub fn describe_node(&self, _virtual_view_id: i32) -> Option<String> {
        None
    }
    pub fn perform_action(&self, _virtual_view_id: i32, _verb: i32) -> bool {
        false
    }
}

#[cfg(not(feature = "a11y"))]
impl Default for AndroidAccessibilityAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── JNI bridge — AzulAccessibilityBridge.java → Rust ─────────────────
//
// Same contract as `jni_bridge` in `android/mod.rs`: the Java side holds the
// `AndroidWindow*` as a `jlong` cookie and passes it back on every call, so
// there is no static state on either side.

#[cfg(all(target_os = "android", feature = "android-activity"))]
mod jni_bridge {
    use super::HOST_VIEW_ID;

    /// Reach the accessibility adapter of the `AndroidWindow` at `native_ptr`.
    ///
    /// SAFETY / THREADING: `native_ptr` is the address `android_main`
    /// published. These upcalls run on the Java UI thread WHILE the loop
    /// thread holds `&mut AndroidWindow`, so this deliberately never forms a
    /// reference to the whole window — `addr_of!` projects straight to the
    /// adapter field, whose own state is behind a `Mutex`. Handing the closure
    /// a `&AndroidWindow` (what the gesture bridge does) would alias the
    /// loop thread's `&mut` for the entire struct.
    ///
    /// The adapter must therefore never be moved or reallocated while the
    /// activity is alive, which holds: it is a field of the window that
    /// `android_main` keeps on its stack frame for the whole run.
    unsafe fn with_adapter<R>(
        native_ptr: i64,
        default: R,
        f: impl FnOnce(&super::AndroidAccessibilityAdapter) -> R,
    ) -> R {
        if native_ptr == 0 {
            return default;
        }
        let window = native_ptr as *const super::super::AndroidWindow;
        let adapter = &*core::ptr::addr_of!((*window).accessibility_adapter);
        f(adapter)
    }

    /// How many virtual views the provider should expose.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_azul_a11y_AzulAccessibilityBridge_nativeGetNodeCount(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        native_ptr: i64,
    ) -> jni::sys::jint {
        with_adapter(
            native_ptr,
            0,
            super::AndroidAccessibilityAdapter::node_count,
        )
    }

    /// One node, packed. See the module docs for the field layout.
    /// Returns `null` for an id the current snapshot does not contain.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_azul_a11y_AzulAccessibilityBridge_nativeDescribeNode(
        env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        native_ptr: i64,
        virtual_view_id: jni::sys::jint,
    ) -> jni::sys::jstring {
        let null = core::ptr::null_mut();
        let described = with_adapter(native_ptr, None, |a| a.describe_node(virtual_view_id));
        let Some(text) = described else {
            return null;
        };
        let Ok(mut env) = jni::JNIEnv::from_raw(env) else {
            return null;
        };
        match env.new_string(text) {
            Ok(s) => s.into_raw(),
            Err(_) => null,
        }
    }

    /// Perform an action on a virtual view. The verb is already translated out
    /// of `AccessibilityNodeInfo.ACTION_*` by the Java side.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_azul_a11y_AzulAccessibilityBridge_nativePerformAction(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        native_ptr: i64,
        virtual_view_id: jni::sys::jint,
        verb: jni::sys::jint,
    ) -> jni::sys::jboolean {
        // HOST_VIEW_ID carries no DOM node; let the framework handle it.
        if virtual_view_id == HOST_VIEW_ID {
            return 0;
        }
        let ok = with_adapter(native_ptr, false, |a| {
            a.perform_action(virtual_view_id, verb)
        });
        u8::from(ok)
    }
}

#[cfg(test)]
#[cfg(feature = "a11y")]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_the_field_separator() {
        let dirty = format!("a{FIELD_SEP}b\nc");
        let clean = sanitize(&dirty);
        assert!(!clean.contains(FIELD_SEP));
        assert_eq!(clean, "a b c");
    }

    #[test]
    fn host_node_lists_the_roots_and_never_fails() {
        let adapter = AndroidAccessibilityAdapter::new();
        // Empty snapshot: the host node must still exist, or TalkBack has no
        // entry point into the tree at all.
        let host = adapter
            .describe_node(HOST_VIEW_ID)
            .expect("the host node must always be describable");
        assert_eq!(host.split(FIELD_SEP).count(), 10);
        // Unknown virtual ids answer None (Java -> null), never a wrong node.
        assert!(adapter.describe_node(7).is_none());
        assert!(!adapter.perform_action(7, VERB_CLICK));
    }
}
