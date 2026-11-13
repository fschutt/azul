//! A node drag that leaves the window becomes the system's drag session, so
//! its files, text or image can be dropped on Finder, the desktop or another
//! application.
//!
//! While the pointer is inside the window the drag is the engine's own: drop
//! targets, `:drag-over`, the drag image the app renders. When it leaves and
//! the drag carries something another application can take
//! (`common::clipboard::drag_data_to_clipboard_content`), the window starts
//! an `NSDraggingSession` from its view with the same content as pasteboard
//! items - a copy of the content, published the way the clipboard publishes
//! it - each file shown with its Finder icon. AppKit tracks the pointer from
//! then on and keeps the button's release; when the session ends, the node
//! drag ends as a release ends it, with what the drop did as its drop effect
//! (`DragEnd`: copy, move, link or none).

use azul_core::{
    drag::{DragEffect, DropEffect},
    events::MouseButton,
};
use objc2::{rc::Retained, runtime::ProtocolObject, AnyThread};
use objc2_app_kit::{NSDragOperation, NSDraggingItem, NSEvent, NSImage, NSWorkspace};
use objc2_foundation::{ns_string, NSArray, NSPoint, NSRect, NSSize, NSString};

use super::{
    events::{self, EventProcessResult},
    MacOSWindow,
};
use crate::desktop::shell2::common::clipboard::{
    clipboard_content_to_payload, drag_data_to_clipboard_content,
};

/// The side of a dragged item's image, points.
const ICON_SIZE: f64 = 32.0;

impl MacOSWindow {
    /// Starts the system's drag session for this window's node drag when the
    /// pointer of `event` (a `mouseDragged:`) has left the window and the
    /// drag carries something another application can take. Once per drag.
    pub(super) fn start_os_drag_if_it_left(&mut self, event: &NSEvent) {
        if self.os_drag_running {
            return;
        }
        let size = self.common.current_window_state().size.dimensions;
        let location = event.locationInWindow();
        let position = events::macos_to_azul_coords(location, size.height);
        let inside = position.x >= 0.0
            && position.y >= 0.0
            && position.x <= size.width
            && position.y <= size.height;
        if inside {
            return;
        }
        let Some(content) = self
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.gesture_drag_manager.get_drag_context())
            .and_then(|ctx| ctx.as_node_drag())
            .and_then(|drag| drag_data_to_clipboard_content(&drag.drag_data))
        else {
            return;
        };
        let Some(payload) = clipboard_content_to_payload(&content) else {
            return;
        };
        let files: Vec<String> = content
            .files
            .as_ref()
            .iter()
            .map(|f| f.as_str().to_owned())
            .collect();
        let items: Vec<Retained<NSDraggingItem>> = super::clipboard::pasteboard_items(&payload)
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let writer = ProtocolObject::from_ref(&**item);
                let dragging = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
                // Fanned out a little under the pointer, as Finder shows a
                // drag of several files.
                #[allow(clippy::cast_precision_loss)] // a handful of items
                let shift = 6.0 * index as f64;
                let frame = NSRect::new(
                    NSPoint::new(
                        location.x - ICON_SIZE / 2.0 + shift,
                        location.y - ICON_SIZE / 2.0 - shift,
                    ),
                    NSSize::new(ICON_SIZE, ICON_SIZE),
                );
                let icon = drag_icon(files.get(index).map(String::as_str));
                let contents: Option<&objc2::runtime::AnyObject> =
                    icon.as_deref().map(AsRef::as_ref);
                // SAFETY: the contents of a dragging frame are an NSImage.
                unsafe {
                    dragging.setDraggingFrame_contents(frame, contents);
                }
                dragging
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let items = NSArray::from_retained_slice(&items);
        let started = if let Some(view) = self.gl_view.as_ref() {
            let _ = view.beginDraggingSessionWithItems_event_source(
                &items,
                event,
                ProtocolObject::from_ref(&**view),
            );
            true
        } else if let Some(view) = self.cpu_view.as_ref() {
            let _ = view.beginDraggingSessionWithItems_event_source(
                &items,
                event,
                ProtocolObject::from_ref(&**view),
            );
            true
        } else {
            false
        };
        self.os_drag_running = started;
    }

    /// What the node drag lets the drop do (`DragData::effect_allowed`), as
    /// the session's operation mask. Not set (`Uninitialized`) is every
    /// operation, as on the web.
    pub(super) fn os_drag_operations(&self) -> NSDragOperation {
        let allowed = self
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.gesture_drag_manager.get_drag_context())
            .and_then(|ctx| ctx.as_node_drag())
            .map(|drag| drag.drag_data.effect_allowed);
        match allowed {
            None | Some(DragEffect::None) => NSDragOperation::None,
            Some(DragEffect::Copy) => NSDragOperation::Copy,
            Some(DragEffect::Link) => NSDragOperation::Link,
            Some(DragEffect::Move) => NSDragOperation::Move,
            Some(DragEffect::CopyLink) => NSDragOperation::Copy | NSDragOperation::Link,
            Some(DragEffect::CopyMove) => NSDragOperation::Copy | NSDragOperation::Move,
            Some(DragEffect::LinkMove) => NSDragOperation::Link | NSDragOperation::Move,
            Some(DragEffect::All | DragEffect::Uninitialized) => {
                NSDragOperation::Copy
                    | NSDragOperation::Link
                    | NSDragOperation::Move
                    | NSDragOperation::Generic
            }
        }
    }

    /// The system's drag session ended with `operation`: the node drag ends
    /// as the button's release ends it - AppKit kept that `mouseUp:` - with
    /// the drop effect the drop chose, so the source's `DragEnd` knows whether
    /// its files were copied, moved, linked or not taken at all.
    pub(super) fn os_drag_ended(&mut self, operation: NSDragOperation) -> EventProcessResult {
        if !self.os_drag_running {
            return EventProcessResult::DoNothing;
        }
        self.os_drag_running = false;
        let effect = drop_effect_of(operation);
        if let Some(drag) = self
            .common
            .layout_window
            .as_mut()
            .and_then(|lw| lw.gesture_drag_manager.get_drag_context_mut())
            .and_then(|ctx| ctx.as_node_drag_mut())
        {
            drag.drop_effect = effect;
            drag.drop_accepted = effect != DropEffect::None;
        }
        // The release AppKit kept, where the pointer is now (outside the
        // window, or back over it after a refused drop).
        let location = self.window.mouseLocationOutsideOfEventStream();
        let height = self.common.current_window_state().size.dimensions.height;
        let position = events::macos_to_azul_coords(location, height);
        self.release_button(position, MouseButton::Left, None)
    }
}

/// The drop effect of a finished session's operation: a move when the drop
/// moved, a copy for a copy (or Finder's generic one), a link, else none.
pub(super) fn drop_effect_of(operation: NSDragOperation) -> DropEffect {
    if operation.contains(NSDragOperation::Move) {
        DropEffect::Move
    } else if operation.contains(NSDragOperation::Copy)
        || operation.contains(NSDragOperation::Generic)
    {
        DropEffect::Copy
    } else if operation.contains(NSDragOperation::Link) {
        DropEffect::Link
    } else {
        DropEffect::None
    }
}

/// The image a dragged item shows: a file's Finder icon, else the generic
/// documents icon.
fn drag_icon(file: Option<&str>) -> Option<Retained<NSImage>> {
    match file {
        Some(path) => Some(NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path))),
        None => NSImage::imageNamed(ns_string!("NSMultipleDocuments")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Finder answers is what the source's `DragEnd` reads: a move (the
    /// same volume), a copy (another volume, or Option held), a link (an
    /// alias), nothing (dropped where nothing takes it, or Escape).
    #[test]
    fn the_drop_effect_is_what_the_session_says_the_drop_did() {
        assert_eq!(drop_effect_of(NSDragOperation::Move), DropEffect::Move);
        assert_eq!(drop_effect_of(NSDragOperation::Copy), DropEffect::Copy);
        assert_eq!(drop_effect_of(NSDragOperation::Generic), DropEffect::Copy);
        assert_eq!(drop_effect_of(NSDragOperation::Link), DropEffect::Link);
        assert_eq!(drop_effect_of(NSDragOperation::None), DropEffect::None);
    }
}
