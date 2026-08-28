//! iOS accessibility bridge — UIKit `UIAccessibility` ↔ Azul.
//!
//! `accesskit` has adapters for Windows, macOS and Unix. It has NONE for
//! UIKit, so iOS cannot reuse the `accesskit::TreeUpdate` the four desktop
//! backends push. This module is the hand-written equivalent: it turns
//! [`azul_layout::managers::a11y_snapshot::A11ySnapshot`] into a list of
//! `UIAccessibilityElement`s that VoiceOver / Switch Control / Voice Control
//! can navigate, and turns the actions those technologies invoke back into
//! [`azul_core::dom::AccessibilityAction`]s on the shared queue.
//!
//! # Shape
//!
//! `AzulView` is a **`UIAccessibilityContainer`**: it is not itself an element
//! (`isAccessibilityElement` → `NO`) and instead vends one element per exposed
//! DOM node through `accessibilityElementCount` /
//! `accessibilityElementAtIndex:` / `indexOfAccessibilityElement:`. That is the
//! standard shape for a single-view app that draws its own UI — UIKit has no
//! other way to expose sub-elements of a view it cannot introspect.
//!
//! The vended elements are instances of `AzulA11yElement`, a
//! `UIAccessibilityElement` subclass carrying one ivar: the snapshot index of
//! the node it stands for. Everything else (label, value, traits, frame) is set
//! on the element itself, so VoiceOver reads it without calling back into Rust.
//! Only ACTIONS call back:
//!
//! | UIKit                                  | Azul                            |
//! |----------------------------------------|---------------------------------|
//! | `accessibilityActivate`                | `AccessibilityAction::Default`  |
//! | `accessibilityIncrement`               | `Increment`                     |
//! | `accessibilityDecrement`               | `Decrement`                     |
//! | `accessibilityScroll:` (up/down/l/r)   | `ScrollUp` / `Down` / `Left` / `Right` |
//! | `accessibilityElementDidBecomeFocused` | `Focus`                         |
//! | `accessibilityElementDidLoseFocus`     | `Blur`                          |
//!
//! Each pushes onto the window's [`A11yActionQueue`]; `IOSWindow::
//! process_accessibility_actions()` drains it from the `CADisplayLink` tick,
//! i.e. on the same thread and in the same phase the desktop backends drain
//! their adapters. Applying an action inline from the UIKit callback would
//! re-enter the layout window while UIKit holds the main thread mid-traversal.
//!
//! # Ownership
//!
//! UIKit does NOT retain the elements a container vends — the container must
//! keep them alive for as long as it reports them. [`IOSAccessibilityAdapter`]
//! owns the retained `id`s and releases them when the snapshot is replaced or
//! the adapter is dropped.
//!
//! # Element indices are snapshot-scoped
//!
//! An index is only meaningful for the snapshot that produced it. Every rebuild
//! posts `UIAccessibilityLayoutChangedNotification`, which is UIKit's signal to
//! re-read the container, and a stale index resolves to `None` rather than
//! panicking — the index arrives from UIKit, i.e. from outside our control.

#[cfg(feature = "a11y")]
use std::ffi::CString;

#[cfg(feature = "a11y")]
use objc::declare::ClassDecl;
#[cfg(feature = "a11y")]
use objc::runtime::{Class, Object, Sel};
#[cfg(feature = "a11y")]
use objc::{class, msg_send, sel, sel_impl};

#[cfg(feature = "a11y")]
use azul_core::dom::{AccessibilityAction, AccessibilityRole, DomId, NodeId};
#[cfg(feature = "a11y")]
use azul_layout::managers::a11y_snapshot::A11ySnapshot;

#[cfg(feature = "a11y")]
use crate::desktop::shell2::common::accessibility::A11yActionQueue;
#[cfg(feature = "a11y")]
use crate::desktop::shell2::common::debug_server::LogCategory;
#[cfg(feature = "a11y")]
use crate::log_error;

// ─── UIKit constants ──────────────────────────────────────────────────
//
// The trait values are `UIKIT_EXTERN const UIAccessibilityTraits` — the
// header declares the symbols, not the numbers, so they are linked rather
// than hard-coded. A guessed constant is silently wrong; a wrong symbol is a
// link error, which is the failure mode to prefer.

#[cfg(feature = "a11y")]
#[link(name = "UIKit", kind = "framework")]
extern "C" {
    static UIAccessibilityTraitNone: u64;
    static UIAccessibilityTraitButton: u64;
    static UIAccessibilityTraitLink: u64;
    static UIAccessibilityTraitImage: u64;
    static UIAccessibilityTraitStaticText: u64;
    static UIAccessibilityTraitHeader: u64;
    static UIAccessibilityTraitNotEnabled: u64;
    static UIAccessibilityTraitSelected: u64;
    static UIAccessibilityTraitAdjustable: u64;

    /// `UIAccessibilityNotifications` is `uint32_t`.
    static UIAccessibilityLayoutChangedNotification: u32;

    fn UIAccessibilityPostNotification(notification: u32, argument: *mut Object);
    fn UIAccessibilityIsVoiceOverRunning() -> bool;
}

/// `UIAccessibilityScrollDirection`. These ARE spelled out in the header as an
/// `NS_ENUM` with explicit values, so they are safe to name here.
#[cfg(feature = "a11y")]
mod scroll_direction {
    pub const RIGHT: i64 = 1;
    pub const LEFT: i64 = 2;
    pub const UP: i64 = 3;
    pub const DOWN: i64 = 4;
}

/// `NSNotFound`, i.e. `NSIntegerMax`. What
/// `indexOfAccessibilityElement:` must return for an element it does not own.
#[cfg(feature = "a11y")]
const NS_NOT_FOUND: i64 = i64::MAX;

/// Name of the ivar on `AzulA11yElement` holding the snapshot index.
#[cfg(feature = "a11y")]
const INDEX_IVAR: &str = "azulSnapshotIndex";

// ─── Adapter ──────────────────────────────────────────────────────────

/// Owns the UIKit element list and the inbound action queue.
#[cfg(feature = "a11y")]
pub struct IOSAccessibilityAdapter {
    /// Actions VoiceOver requested, drained by the frame loop.
    queue: A11yActionQueue,
    /// The tree UIKit is currently being shown.
    snapshot: A11ySnapshot,
    /// Retained `AzulA11yElement`s, index-aligned with `snapshot.elements`.
    /// UIKit does not retain what a container vends, so these are ours to keep
    /// alive and ours to release.
    elements: Vec<*mut Object>,
}

#[cfg(feature = "a11y")]
impl IOSAccessibilityAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: A11yActionQueue::new(),
            snapshot: A11ySnapshot::default(),
            elements: Vec::new(),
        }
    }

    /// Pop one queued action, decoded into Azul types. Same signature as the
    /// desktop adapters' `poll_action`, so the frame pumps are identical.
    #[must_use]
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        self.queue.poll_action()
    }

    /// Queue an action for the element at `index`.
    ///
    /// Refuses when the index is stale (a rebuild raced UIKit's traversal) or
    /// when the element does not declare the action. Refusing is the point: the
    /// engine would drop an unsupported action anyway, and a silently-dropped
    /// action is indistinguishable from a working one.
    pub fn queue_action_for(&self, index: usize, action: &AccessibilityAction) -> bool {
        let Some(element) = self.snapshot.element(index) else {
            return false;
        };
        if !element.supports(action) {
            return false;
        }
        self.queue
            .push(element.dom_id, element.node_id, action.clone());
        true
    }

    /// Number of elements the container vends.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// The retained element at `index`, or null when out of range.
    #[must_use]
    pub fn element_at(&self, index: usize) -> *mut Object {
        self.elements
            .get(index)
            .copied()
            .unwrap_or(core::ptr::null_mut())
    }

    /// Position of a vended element, or `None` if it is not one of ours.
    #[must_use]
    pub fn index_of(&self, element: *mut Object) -> Option<usize> {
        self.elements.iter().position(|e| *e == element)
    }

    /// Replace the exposed tree.
    ///
    /// Called after every layout: rebuild the element objects from `snapshot`,
    /// release the previous ones, and tell UIKit the layout changed so it
    /// re-reads the container. Doing the release AFTER the rebuild would be
    /// simpler; doing it in this order means the container is never observed
    /// holding dangling pointers.
    pub fn update_snapshot(&mut self, snapshot: A11ySnapshot, container: *mut Object) {
        let old = core::mem::take(&mut self.elements);

        self.elements = Vec::with_capacity(snapshot.elements.len());
        for (index, element) in snapshot.elements.iter().enumerate() {
            let obj = unsafe { make_element(container, index, element) };
            if obj.is_null() {
                // Element construction failed (class registration refused, OOM).
                // Push the null so indices stay aligned with the snapshot —
                // `element_at` hands UIKit a null, which it treats as "no
                // element", rather than silently shifting every later index onto
                // the wrong DOM node.
                log_error!(
                    LogCategory::General,
                    "[iOS a11y] could not create UIAccessibilityElement for snapshot index {}",
                    index
                );
            }
            self.elements.push(obj);
        }
        self.snapshot = snapshot;

        for obj in old {
            if !obj.is_null() {
                unsafe {
                    let _: () = msg_send![obj, release];
                }
            }
        }

        Self::post_layout_changed();
    }

    /// Tell UIKit the element list changed.
    fn post_layout_changed() {
        unsafe {
            UIAccessibilityPostNotification(
                UIAccessibilityLayoutChangedNotification,
                core::ptr::null_mut(),
            );
        }
    }

    /// Is VoiceOver actually listening?
    ///
    /// Informational only — the element list is built regardless, because
    /// Switch Control and Voice Control are not VoiceOver and this query does
    /// not see them. Skipping the build on `false` would leave those two users
    /// with nothing.
    #[must_use]
    pub fn voiceover_running(&self) -> bool {
        unsafe { UIAccessibilityIsVoiceOverRunning() }
    }
}

#[cfg(feature = "a11y")]
impl Default for IOSAccessibilityAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "a11y")]
impl Drop for IOSAccessibilityAdapter {
    fn drop(&mut self) {
        for obj in self.elements.drain(..) {
            if !obj.is_null() {
                unsafe {
                    let _: () = msg_send![obj, release];
                }
            }
        }
    }
}

// ─── Element construction ─────────────────────────────────────────────

/// UTF-8 → `NSString*` (autoreleased). Returns null for a string containing an
/// interior NUL, which `stringWithUTF8String:` cannot represent.
#[cfg(feature = "a11y")]
unsafe fn ns_string(s: &str) -> *mut Object {
    let Ok(c) = CString::new(s) else {
        return core::ptr::null_mut();
    };
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

/// Build one retained `AzulA11yElement` for `element`.
#[cfg(feature = "a11y")]
unsafe fn make_element(
    container: *mut Object,
    index: usize,
    element: &azul_layout::managers::a11y_snapshot::A11yElement,
) -> *mut Object {
    use super::{CGPoint, CGRect, CGSize};

    let cls = get_or_create_a11y_element_class();
    let alloc: *mut Object = msg_send![cls, alloc];
    if alloc.is_null() {
        return core::ptr::null_mut();
    }
    let obj: *mut Object = msg_send![alloc, initWithAccessibilityContainer: container];
    if obj.is_null() {
        return core::ptr::null_mut();
    }

    // Snapshot index — the only state the element carries. Everything else is
    // pushed into UIKit's own properties so a VoiceOver read never re-enters
    // Rust (it happens on the main thread, mid-traversal).
    {
        let obj_ref: &mut Object = &mut *obj;
        obj_ref.set_ivar::<i64>(INDEX_IVAR, index as i64);
    }

    let label = ns_string(&element.label);
    if !label.is_null() {
        let _: () = msg_send![obj, setAccessibilityLabel: label];
    }
    if let Some(value) = element.value.as_deref() {
        let value = ns_string(value);
        if !value.is_null() {
            let _: () = msg_send![obj, setAccessibilityValue: value];
        }
    }

    let _: () = msg_send![obj, setAccessibilityTraits: traits_for(element)];

    // `accessibilityFrameInContainerSpace` (iOS 10+) lets UIKit do the
    // view→screen conversion itself. The snapshot's bounds are logical units,
    // which on iOS ARE points, so no scaling is needed here — the reason the
    // snapshot deliberately does not pre-multiply by the HiDPI factor.
    let frame = CGRect {
        origin: CGPoint {
            x: f64::from(element.bounds.origin.x),
            y: f64::from(element.bounds.origin.y),
        },
        size: CGSize {
            width: f64::from(element.bounds.size.width),
            height: f64::from(element.bounds.size.height),
        },
    };
    let _: () = msg_send![obj, setAccessibilityFrameInContainerSpace: frame];

    obj
}

/// Map an Azul role + state onto `UIAccessibilityTraits`.
///
/// Traits are how VoiceOver decides what to SAY ("button", "link", "heading")
/// and which gestures to offer (`Adjustable` is what enables the swipe-up /
/// swipe-down increment gesture). Getting this wrong is not cosmetic: an
/// element with no `Adjustable` trait can never be incremented no matter what
/// the engine supports.
#[cfg(feature = "a11y")]
fn traits_for(element: &azul_layout::managers::a11y_snapshot::A11yElement) -> u64 {
    unsafe {
        let mut traits = UIAccessibilityTraitNone;

        traits |= match element.role {
            AccessibilityRole::PushButton
            | AccessibilityRole::CheckButton
            | AccessibilityRole::RadioButton
            | AccessibilityRole::SplitButton
            | AccessibilityRole::ButtonDropdown
            | AccessibilityRole::ButtonMenu => UIAccessibilityTraitButton,
            AccessibilityRole::Link => UIAccessibilityTraitLink,
            AccessibilityRole::Graphic | AccessibilityRole::Diagram => UIAccessibilityTraitImage,
            AccessibilityRole::StaticText => UIAccessibilityTraitStaticText,
            AccessibilityRole::Slider | AccessibilityRole::SpinButton | AccessibilityRole::Dial => {
                UIAccessibilityTraitAdjustable
            }
            _ => UIAccessibilityTraitNone,
        };

        // The engine's own capability is the authority for Adjustable: the
        // Increment/Decrement gestures must appear exactly when the engine will
        // act on them, not when the role happens to look slider-ish.
        if element.supports(&AccessibilityAction::Increment)
            || element.supports(&AccessibilityAction::Decrement)
        {
            traits |= UIAccessibilityTraitAdjustable;
        }
        if element.disabled {
            traits |= UIAccessibilityTraitNotEnabled;
        }
        if element.checked == Some(true) {
            traits |= UIAccessibilityTraitSelected;
        }

        // KNOWN GAP, stated rather than faked: `AccessibilityRole` has no
        // Heading variant, so `<h1>`-`<h6>` currently arrive here as
        // `StaticText` (see `a11y_snapshot::node_type_to_role`) and VoiceOver's
        // heading rotor will not list them. Fixing it needs a Heading role in
        // `core/src/a11y.rs`, which is an FFI-visible enum change and belongs
        // in its own commit. The symbol is named so the intent is greppable.
        let _ = UIAccessibilityTraitHeader;

        traits
    }
}

// ─── AzulA11yElement class ────────────────────────────────────────────

#[cfg(feature = "a11y")]
fn get_or_create_a11y_element_class() -> &'static Class {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = core::ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(UIAccessibilityElement);
        let mut decl = ClassDecl::new("AzulA11yElement", superclass)
            .expect("AzulA11yElement class name already registered");
        decl.add_ivar::<i64>(INDEX_IVAR);

        decl.add_method(
            sel!(accessibilityActivate),
            a11y_activate as extern "C" fn(&Object, Sel) -> bool,
        );
        decl.add_method(
            sel!(accessibilityIncrement),
            a11y_increment as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(accessibilityDecrement),
            a11y_decrement as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(accessibilityScroll:),
            a11y_scroll as extern "C" fn(&Object, Sel, i64) -> bool,
        );
        decl.add_method(
            sel!(accessibilityElementDidBecomeFocused),
            a11y_did_become_focused as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(accessibilityElementDidLoseFocus),
            a11y_did_lose_focus as extern "C" fn(&Object, Sel),
        );

        CLS = decl.register();
    });
    unsafe { &*CLS }
}

/// Snapshot index stored on an element.
#[cfg(feature = "a11y")]
fn element_index(this: &Object) -> usize {
    let raw: i64 = unsafe { *this.get_ivar::<i64>(INDEX_IVAR) };
    usize::try_from(raw).unwrap_or(usize::MAX)
}

/// Queue `action` for the element `this` stands for.
///
/// Returns whether the action was accepted, which UIKit uses as the return
/// value of `accessibilityActivate` / `accessibilityScroll:` — VoiceOver plays
/// the "failed" earcon on `NO`, so answering `YES` for an action that will be
/// dropped would tell the user something happened when nothing did.
#[cfg(feature = "a11y")]
fn queue_from_element(this: &Object, action: AccessibilityAction) -> bool {
    let index = element_index(this);
    let Some(window) = (unsafe { super::azul_ios_window() }) else {
        return false;
    };
    window
        .accessibility_adapter
        .queue_action_for(index, &action)
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_activate(this: &Object, _cmd: Sel) -> bool {
    queue_from_element(this, AccessibilityAction::Default)
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_increment(this: &Object, _cmd: Sel) {
    let _ = queue_from_element(this, AccessibilityAction::Increment);
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_decrement(this: &Object, _cmd: Sel) {
    let _ = queue_from_element(this, AccessibilityAction::Decrement);
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_scroll(this: &Object, _cmd: Sel, direction: i64) -> bool {
    // UIKit's direction is where the CONTENT should move toward; the engine's
    // ScrollUp/Down move the viewport. Up == show earlier content, which is
    // what `UIAccessibilityScrollDirectionUp` asks for.
    let action = match direction {
        scroll_direction::UP => AccessibilityAction::ScrollUp,
        scroll_direction::DOWN => AccessibilityAction::ScrollDown,
        scroll_direction::LEFT => AccessibilityAction::ScrollLeft,
        scroll_direction::RIGHT => AccessibilityAction::ScrollRight,
        // Next/Previous (page navigation) has no engine equivalent; answering
        // NO lets UIKit fall back to its own paging instead of pretending.
        _ => return false,
    };
    queue_from_element(this, action)
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_did_become_focused(this: &Object, _cmd: Sel) {
    let _ = queue_from_element(this, AccessibilityAction::Focus);
}

#[cfg(feature = "a11y")]
extern "C" fn a11y_did_lose_focus(this: &Object, _cmd: Sel) {
    let _ = queue_from_element(this, AccessibilityAction::Blur);
}

// ─── UIAccessibilityContainer methods, installed on AzulView ──────────
//
// `UIAccessibilityContainer` is an INFORMAL protocol (a category on NSObject),
// so it must NOT be passed to `ClassDecl::add_protocol` — `Protocol::get`
// returns None for it and the unwrap would abort at startup. Adding the four
// methods is the whole conformance.

/// A container is not itself an element; returning `YES` here would collapse
/// the entire UI into one unreadable blob.
#[cfg(feature = "a11y")]
pub extern "C" fn view_is_accessibility_element(_this: &Object, _cmd: Sel) -> bool {
    false
}

#[cfg(feature = "a11y")]
pub extern "C" fn view_accessibility_element_count(_this: &Object, _cmd: Sel) -> i64 {
    unsafe { super::azul_ios_window() }
        .map_or(0, |w| w.accessibility_adapter.element_count() as i64)
}

#[cfg(feature = "a11y")]
pub extern "C" fn view_accessibility_element_at_index(
    _this: &Object,
    _cmd: Sel,
    index: i64,
) -> *mut Object {
    let Ok(index) = usize::try_from(index) else {
        return core::ptr::null_mut();
    };
    unsafe { super::azul_ios_window() }.map_or(core::ptr::null_mut(), |w| {
        w.accessibility_adapter.element_at(index)
    })
}

#[cfg(feature = "a11y")]
pub extern "C" fn view_index_of_accessibility_element(
    _this: &Object,
    _cmd: Sel,
    element: *mut Object,
) -> i64 {
    unsafe { super::azul_ios_window() }
        .and_then(|w| w.accessibility_adapter.index_of(element))
        .map_or(NS_NOT_FOUND, |i| i as i64)
}

/// Register the four container methods on the `AzulView` class being declared.
///
/// Called from `get_or_create_view_class` while the class is still mutable.
#[cfg(feature = "a11y")]
pub fn install_container_methods(decl: &mut ClassDecl) {
    unsafe {
        decl.add_method(
            sel!(isAccessibilityElement),
            view_is_accessibility_element as extern "C" fn(&Object, Sel) -> bool,
        );
        decl.add_method(
            sel!(accessibilityElementCount),
            view_accessibility_element_count as extern "C" fn(&Object, Sel) -> i64,
        );
        decl.add_method(
            sel!(accessibilityElementAtIndex:),
            view_accessibility_element_at_index as extern "C" fn(&Object, Sel, i64) -> *mut Object,
        );
        decl.add_method(
            sel!(indexOfAccessibilityElement:),
            view_index_of_accessibility_element as extern "C" fn(&Object, Sel, *mut Object) -> i64,
        );
    }
}

// ─── Stub when the `a11y` feature is off ──────────────────────────────
//
// Same shape as `linux/x11/accessibility.rs`: the field on `IOSWindow` stays
// unconditional so the struct literal does not need feature gating, but every
// operation is a no-op and `poll_action` yields nothing, so
// `process_accessibility_actions` returns immediately.

#[cfg(not(feature = "a11y"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct IOSAccessibilityAdapter;

#[cfg(not(feature = "a11y"))]
impl IOSAccessibilityAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
    #[must_use]
    pub const fn element_count(&self) -> usize {
        0
    }
}
