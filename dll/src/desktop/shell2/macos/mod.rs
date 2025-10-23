//! macOS implementation using AppKit/Cocoa.
//!
//! This module implements the PlatformWindow trait for macOS using:
//! - NSWindow for window management
//! - NSOpenGLContext for GPU rendering (optional)
//! - NSMenu for menu bar and context menus
//! - NSEvent for event handling
//!
//! Note: macOS uses static linking for system frameworks (standard approach).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use azul_core::{dom::DomId, menu::Menu};
use azul_layout::window_state::{FullWindowState, WindowCreateOptions, WindowState};
use objc2::{
    define_class,
    msg_send,
    msg_send_id,
    rc::{Allocated, Retained},
    runtime::{Bool, NSObjectProtocol, ProtocolObject, YES},
    AnyThread, // For alloc() method
    ClassType,
    DeclaredClass,
    MainThreadMarker,
    MainThreadOnly,
};
use objc2_app_kit::{
    NSAppKitVersionNumber, NSAppKitVersionNumber10_12, NSApplication,
    NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType, NSBitmapImageRep,
    NSColor, NSCompositingOperation, NSEvent, NSEventMask, NSEventType, NSImage, NSMenu,
    NSMenuItem, NSOpenGLContext, NSOpenGLPixelFormat, NSOpenGLPixelFormatAttribute, NSOpenGLView,
    NSResponder, NSScreen, NSTextInputClient, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSVisualEffectView, NSWindow, NSWindowDelegate, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{
    ns_string, NSAttributedString, NSData, NSNotification, NSObject, NSPoint, NSRange, NSRect,
    NSSize, NSString,
};

use crate::desktop::{
    shell2::common::{
        Compositor, CompositorError, CompositorMode, PlatformWindow, RenderContext, WindowError,
        WindowProperties,
    },
    wr_translate2::{
        default_renderer_options, translate_document_id_wr, translate_id_namespace_wr,
        wr_translate_document_id, wr_translate_pipeline_id, AsyncHitTester,
        Compositor as WrCompositor, Notifier, WR_SHADER_CACHE,
    },
};

mod events;
mod gl;
mod menu;

use gl::GlFunctions;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RenderBackend {
    OpenGL,
    CPU,
}

// ============================================================================
// GLView - OpenGL rendering view
// ============================================================================

/// Instance variables for GLView
pub struct GLViewIvars {
    gl_functions: RefCell<Option<Rc<gl_context_loader::GenericGlContext>>>,
    needs_reshape: Cell<bool>,
    tracking_area: RefCell<Option<Retained<NSTrackingArea>>>,
    mtm: MainThreadMarker, // Store MainThreadMarker to avoid unsafe new_unchecked
    /// Back-pointer to the owning MacOSWindow (as *mut to avoid forward reference)
    /// This is set after window creation via set_window_ptr()
    window_ptr: RefCell<Option<*mut std::ffi::c_void>>,
}

define_class!(
    #[unsafe(super(NSOpenGLView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulGLView"]
    #[ivars = GLViewIvars]
    pub struct GLView;

    impl GLView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _rect: NSRect) {
            eprintln!("[GLView] drawRect: called - this is where ALL rendering happens");

            // Get the back-pointer to our MacOSWindow
            let window_ptr = match self.get_window_ptr() {
                Some(ptr) => ptr,
                None => {
                    eprintln!("[GLView] drawRect: No window pointer set yet, skipping render");
                    return;
                }
            };

            // SAFETY: We trust that the window pointer is valid and points to a MacOSWindow
            // The window owns the view, so the window outlives the view
            unsafe {
                let macos_window = &mut *(window_ptr as *mut MacOSWindow);

                // Call the rendering method on MacOSWindow
                // This will:
                // 1. Make GL context current
                // 2. Call renderer.update()
                // 3. Call renderer.render() to composite WebRender's scene
                // 4. Call flushBuffer() to swap
                if let Err(e) = macos_window.render_and_present_in_draw_rect() {
                    eprintln!("[GLView] drawRect: Error during rendering: {:?}", e);
                }
            }
        }

        #[unsafe(method(prepareOpenGL))]
        fn prepare_opengl(&self) {
            // Load GL functions via dlopen
            match GlFunctions::initialize() {
                Ok(functions) => {
                    *self.ivars().gl_functions.borrow_mut() = Some(functions.get_context());
                    self.ivars().needs_reshape.set(true);
                }
                Err(e) => {
                    eprintln!("Failed to load GL functions: {}", e);
                }
            }
        }

        #[unsafe(method(reshape))]
        fn reshape(&self) {
            let mtm = self.ivars().mtm;

            // Update context - THIS IS STILL IMPORTANT
            unsafe {
                if let Some(context) = self.openGLContext() {
                    context.update(mtm);
                }
            }

            // NOTE: glViewport is now set in MacOSWindow::present() instead of here
            // This ensures the viewport is synchronized with every frame render,
            // not just when the OS decides to send a reshape event.

            self.ivars().needs_reshape.set(false);
        }

        // ===== Event Handling =====

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Event will be handled by MacOSWindow via NSApplication event loop
            // This method is required for the view to accept mouse events
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method_id(initWithFrame:pixelFormat:))]
        fn init_with_frame_pixel_format(
            this: Allocated<Self>,
            frame: NSRect,
            pixel_format: Option<&NSOpenGLPixelFormat>,
        ) -> Option<Retained<Self>> {
            // Get MainThreadMarker - we're guaranteed to be on main thread in init
            let mtm = MainThreadMarker::new().expect("init must be called on main thread");

            let this = this.set_ivars(GLViewIvars {
                gl_functions: RefCell::new(None),
                needs_reshape: Cell::new(true),
                tracking_area: RefCell::new(None),
                mtm,
                window_ptr: RefCell::new(None),
            });
            unsafe {
                msg_send_id![super(this), initWithFrame: frame, pixelFormat: pixel_format]
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // Remove old tracking area if exists
            if let Some(old_area) = self.ivars().tracking_area.borrow_mut().take() {
                unsafe {
                    self.removeTrackingArea(&old_area);
                }
            }

            // Create new tracking area for mouse enter/exit events
            let bounds = unsafe { self.bounds() };
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveInKeyWindow
                | NSTrackingAreaOptions::InVisibleRect;

            let tracking_area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    bounds,
                    options,
                    Some(self),
                    None,
                )
            };

            unsafe {
                self.addTrackingArea(&tracking_area);
            }

            *self.ivars().tracking_area.borrow_mut() = Some(tracking_area);
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        // ===== NSTextInputClient Protocol =====
        // Basic IME support for Unicode composition (e.g., Japanese, Chinese, accented characters)

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            // For now, we don't track marked text ranges
            false
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            // Return NSNotFound to indicate no marked text
            NSRange {
                location: usize::MAX,
                length: 0,
            }
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            // Return NSNotFound to indicate no selection
            NSRange {
                location: usize::MAX,
                length: 0,
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(
            &self,
            _string: &NSObject,
            _selected_range: NSRange,
            _replacement_range: NSRange,
        ) {
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            // Called when IME composition is finished
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes_for_marked_text(&self) -> Retained<objc2_foundation::NSArray> {
            // Return empty array - no special attributes needed
            unsafe { objc2_foundation::NSArray::new() }
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring_for_proposed_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &NSObject, _replacement_range: NSRange) {
            if let Some(ns_string) = string.downcast_ref::<NSString>() {
                let text = ns_string.to_string();
                eprintln!("[IME] Insert text: {}", text);
            }
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> usize {
            // Return NSNotFound
            usize::MAX
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect_for_character_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> NSRect {
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: 0.0,
                    height: 0.0,
                },
            }
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: objc2::runtime::Sel) {
            // Called for special key commands during IME
        }
    }
);

// ============================================================================
// CPUView - CPU rendering view
// ============================================================================

/// Instance variables for CPUView
pub struct CPUViewIvars {
    framebuffer: RefCell<Vec<u8>>,
    width: Cell<usize>,
    height: Cell<usize>,
    needs_redraw: Cell<bool>,
    tracking_area: RefCell<Option<Retained<NSTrackingArea>>>,
    mtm: MainThreadMarker, // Store MainThreadMarker to avoid unsafe new_unchecked
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulCPUView"]
    #[ivars = CPUViewIvars]
    pub struct CPUView;

    impl CPUView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty_rect: NSRect) {
            let bounds = unsafe { self.bounds() };
            let width = bounds.size.width as usize;
            let height = bounds.size.height as usize;

            let ivars = self.ivars();

            // Resize framebuffer if needed
            let current_width = ivars.width.get();
            let current_height = ivars.height.get();

            if current_width != width || current_height != height {
                ivars.width.set(width);
                ivars.height.set(height);
                ivars.framebuffer.borrow_mut().resize(width * height * 4, 0);
            }

            // Render blue gradient to framebuffer
            {
                let mut framebuffer = ivars.framebuffer.borrow_mut();
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) * 4;
                        framebuffer[idx] = (x * 128 / width.max(1)) as u8; // R
                        framebuffer[idx + 1] = (y * 128 / height.max(1)) as u8; // G
                        framebuffer[idx + 2] = 255; // B - Blue
                        framebuffer[idx + 3] = 255; // A
                    }
                }
            }

            // Blit framebuffer to window
            unsafe {
                let mtm = ivars.mtm; // Get mtm from ivars
                let framebuffer = ivars.framebuffer.borrow();

                // Use NSData::with_bytes to wrap our framebuffer
                let data = NSData::with_bytes(&framebuffer[..]);

                if let Some(bitmap) = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    width as isize,
                    height as isize,
                    8,
                    4,
                    true,
                    false,
                    ns_string!("NSCalibratedRGBColorSpace"),
                    (width * 4) as isize,
                    32,
                ) {
                    // Copy framebuffer data to bitmap
                    std::ptr::copy_nonoverlapping(
                        framebuffer.as_ptr(),
                        bitmap.bitmapData(),
                        framebuffer.len(),
                    );

                    // Create image and draw
                    let image = NSImage::initWithSize(NSImage::alloc(), bounds.size);
                    image.addRepresentation(&bitmap);
                    image.drawInRect(bounds);
                }
            }
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        // ===== Event Handling =====

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(
            this: Allocated<Self>,
            frame: NSRect,
        ) -> Option<Retained<Self>> {
            // Get MainThreadMarker - we're guaranteed to be on main thread in init
            let mtm = MainThreadMarker::new().expect("init must be called on main thread");

            let this = this.set_ivars(CPUViewIvars {
                framebuffer: RefCell::new(Vec::new()),
                width: Cell::new(0),
                height: Cell::new(0),
                needs_redraw: Cell::new(true),
                tracking_area: RefCell::new(None),
                mtm,
            });
            unsafe {
                msg_send_id![super(this), initWithFrame: frame]
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // Remove old tracking area if exists
            if let Some(old_area) = self.ivars().tracking_area.borrow_mut().take() {
                unsafe {
                    self.removeTrackingArea(&old_area);
                }
            }

            // Create new tracking area for mouse enter/exit events
            let bounds = unsafe { self.bounds() };
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveInKeyWindow
                | NSTrackingAreaOptions::InVisibleRect;

            let tracking_area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    bounds,
                    options,
                    Some(self),
                    None,
                )
            };

            unsafe {
                self.addTrackingArea(&tracking_area);
            }

            *self.ivars().tracking_area.borrow_mut() = Some(tracking_area);
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        // ===== NSTextInputClient Protocol =====
        // Same IME implementation as GLView

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            false
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            NSRange {
                location: usize::MAX,
                length: 0,
            }
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            NSRange {
                location: usize::MAX,
                length: 0,
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(
            &self,
            _string: &NSObject,
            _selected_range: NSRange,
            _replacement_range: NSRange,
        ) {
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes_for_marked_text(&self) -> Retained<objc2_foundation::NSArray> {
            unsafe { objc2_foundation::NSArray::new() }
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring_for_proposed_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &NSObject, _replacement_range: NSRange) {
            if let Some(ns_string) = string.downcast_ref::<NSString>() {
                let text = ns_string.to_string();
                eprintln!("[IME] Insert text: {}", text);
            }
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> usize {
            usize::MAX
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect_for_character_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> NSRect {
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: 0.0,
                    height: 0.0,
                },
            }
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: objc2::runtime::Sel) {
        }
    }
);

// ============================================================================
// GLView Helper Methods (outside define_class!)
// ============================================================================

impl GLView {
    /// Set the back-pointer to the owning MacOSWindow
    /// SAFETY: Caller must ensure the pointer remains valid for the lifetime of the view
    pub unsafe fn set_window_ptr(&self, window_ptr: *mut std::ffi::c_void) {
        *self.ivars().window_ptr.borrow_mut() = Some(window_ptr);
    }

    /// Get the back-pointer to the owning MacOSWindow
    fn get_window_ptr(&self) -> Option<*mut std::ffi::c_void> {
        *self.ivars().window_ptr.borrow()
    }
}

// ============================================================================
// WindowDelegate - Handles window lifecycle events (close, resize, etc.)
// ============================================================================

/// Instance variables for WindowDelegate
pub struct WindowDelegateIvars {
    /// Back-pointer to the owning MacOSWindow for handling close callbacks
    /// This is set after window creation via set_window_ptr()
    window_ptr: RefCell<Option<*mut std::ffi::c_void>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulWindowDelegate"]
    #[ivars = WindowDelegateIvars]
    pub struct WindowDelegate;

    impl WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: Option<&NSWindow>) -> Bool {
            let window_ptr = *self.ivars().window_ptr.borrow();

            if let Some(window_ptr) = window_ptr {
                eprintln!("[WindowDelegate] Close requested, invoking callback");

                // SAFETY: window_ptr points to MacOSWindow which owns this delegate
                // The window outlives the delegate, so this pointer is always valid
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);

                    // Call the MacOSWindow method to handle close
                    // This will invoke callbacks and determine if close should proceed
                    match macos_window.handle_window_should_close() {
                        Ok(should_close) => {
                            if should_close {
                                eprintln!("[WindowDelegate] Allowing close");
                                Bool::YES
                            } else {
                                eprintln!("[WindowDelegate] Preventing close (callback cancelled)");
                                Bool::NO
                            }
                        }
                        Err(e) => {
                            eprintln!("[WindowDelegate] Error handling close: {}, allowing close", e);
                            Bool::YES // Allow close on error to avoid stuck window
                        }
                    }
                }
            } else {
                // No window pointer, allow close by default
                eprintln!("[WindowDelegate] No window pointer, allowing close");
                Bool::YES
            }
        }

        /// Called when the window is minimized to the Dock
        #[unsafe(method(windowDidMiniaturize:))]
        fn window_did_miniaturize(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.flags.frame = azul_core::window::WindowFrame::Minimized;
                }
                eprintln!("[WindowDelegate] Window minimized");
            }
        }

        /// Called when the window is restored from the Dock
        #[unsafe(method(windowDidDeminiaturize:))]
        fn window_did_deminiaturize(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.flags.frame = azul_core::window::WindowFrame::Normal;
                }
                eprintln!("[WindowDelegate] Window deminiaturized");
            }
        }

        /// Called when the window enters fullscreen mode
        #[unsafe(method(windowDidEnterFullScreen:))]
        fn window_did_enter_fullscreen(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.flags.frame = azul_core::window::WindowFrame::Fullscreen;
                }
                eprintln!("[WindowDelegate] Window entered fullscreen");
            }
        }

        /// Called when the window exits fullscreen mode
        #[unsafe(method(windowDidExitFullScreen:))]
        fn window_did_exit_fullscreen(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    // Return to normal frame, will be updated by resize check if maximized
                    macos_window.current_window_state.flags.frame = azul_core::window::WindowFrame::Normal;
                }
                eprintln!("[WindowDelegate] Window exited fullscreen");
            }
        }

        /// Called when the window is resized
        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    let frame = macos_window.current_window_state.flags.frame;
                    // Only check for maximized state if not in fullscreen
                    if frame != azul_core::window::WindowFrame::Fullscreen {
                        // Set flag to check maximized state in event loop
                        // The event loop will compare window.frame() to screen.visibleFrame()
                        eprintln!("[WindowDelegate] Window resized");
                    }
                }
            }
        }

        /// Called when the window becomes the key window (receives focus)
        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.window_focused = true;
                }
            }
        }

        /// Called when the window resigns key window status (loses focus)
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.window_focused = false;
                }
            }
        }

        /// Called when the window is moved
        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, _notification: &NSNotification) {
            // Window position is tracked in the main event loop
            // No need to update state here, just for consistency
        }

        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(WindowDelegateIvars {
                window_ptr: RefCell::new(None),
            });
            unsafe { msg_send_id![super(this), init] }
        }
    }
);

// SAFETY: NSObjectProtocol has no safety requirements
unsafe impl NSObjectProtocol for WindowDelegate {}

// SAFETY: NSWindowDelegate has no safety requirements, and WindowDelegate is MainThreadOnly
unsafe impl NSWindowDelegate for WindowDelegate {}

impl WindowDelegate {
    /// Create a new WindowDelegate
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let result: Option<Retained<Self>> = unsafe { msg_send_id![Self::alloc(mtm), init] };
        result.expect("Failed to initialize WindowDelegate")
    }

    /// Set the window pointer for this delegate
    /// SAFETY: Caller must ensure the pointer remains valid for the lifetime of the delegate
    pub unsafe fn set_window_ptr(&self, window_ptr: *mut std::ffi::c_void) {
        *self.ivars().window_ptr.borrow_mut() = Some(window_ptr);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create OpenGL pixel format with proper attributes
fn create_opengl_pixel_format(
    mtm: MainThreadMarker,
) -> Result<Retained<NSOpenGLPixelFormat>, WindowError> {
    // OpenGL 3.2 Core Profile attributes
    let attrs: Vec<u32> = vec![
        5, // NSOpenGLPFADoubleBuffer
        12, 24, // NSOpenGLPFADepthSize(24)
        99, 0x3200, // NSOpenGLPFAOpenGLProfile(3.2 Core)
        8, 24, // NSOpenGLPFAColorSize(24)
        11, 8,  // NSOpenGLPFAAlphaSize(8)
        73, // NSOpenGLPFAAccelerated
        0,  // Null terminator
    ];

    // Note: NSOpenGLPixelFormat::initWithAttributes expects NonNull<u32> in objc2-app-kit 0.3.2
    unsafe {
        let attrs_ptr = std::ptr::NonNull::new_unchecked(attrs.as_ptr() as *mut u32);
        NSOpenGLPixelFormat::initWithAttributes(NSOpenGLPixelFormat::alloc(), attrs_ptr)
            .ok_or_else(|| WindowError::ContextCreationFailed)
    }
}

// ============================================================================
// MacOSWindow - Main window implementation
// ============================================================================

/// macOS window implementation with dual rendering backend support
pub struct MacOSWindow {
    /// The NSWindow instance
    window: Retained<NSWindow>,

    /// Window delegate for handling window events
    window_delegate: Retained<WindowDelegate>,

    /// Selected rendering backend
    backend: RenderBackend,

    /// OpenGL rendering components (if backend == OpenGL)
    gl_view: Option<Retained<GLView>>,
    gl_context: Option<Retained<NSOpenGLContext>>,
    gl_functions: Option<Rc<GlFunctions>>,

    /// CPU rendering components (if backend == CPU)
    cpu_view: Option<Retained<CPUView>>,

    /// Window is open flag
    is_open: bool,

    /// Main thread marker (required for AppKit)
    mtm: MainThreadMarker,

    /// Window state from previous frame (for diff detection)
    previous_window_state: Option<FullWindowState>,

    /// Current window state
    current_window_state: FullWindowState,

    /// Last hovered node (for hover state tracking)
    last_hovered_node: Option<events::HitTestNode>,

    /// LayoutWindow integration (for UI callbacks and display list)
    layout_window: Option<azul_layout::window::LayoutWindow>,

    /// Menu state (for hash-based diff updates)
    menu_state: menu::MenuState,

    // Resource caches for LayoutWindow
    /// Image cache for texture management
    image_cache: azul_core::resources::ImageCache,

    /// Renderer resources (GPU textures, etc.)
    renderer_resources: azul_core::resources::RendererResources,

    // WebRender infrastructure for proper hit-testing and rendering
    /// Main render API for registering fonts, images, display lists
    pub(crate) render_api: webrender::RenderApi,

    /// WebRender renderer (software or hardware depending on backend)
    pub(crate) renderer: Option<webrender::Renderer>,

    /// Hit-tester for fast asynchronous hit-testing (updated on layout changes)
    pub(crate) hit_tester: crate::desktop::wr_translate2::AsyncHitTester,

    /// WebRender document ID
    pub(crate) document_id: azul_core::hit_test::DocumentId,

    /// WebRender ID namespace
    pub(crate) id_namespace: azul_core::resources::IdNamespace,

    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub(crate) gl_context_ptr: azul_core::gl::OptionGlContextPtr,

    // Application-level shared state
    /// Shared application data (used by callbacks, shared across windows)
    app_data: std::sync::Arc<std::cell::RefCell<azul_core::refany::RefAny>>,

    /// Shared font cache (shared across windows to cache font loading)
    fc_cache: std::sync::Arc<rust_fontconfig::FcFontCache>,

    /// Track if frame needs regeneration (to avoid multiple generate_frame calls)
    frame_needs_regeneration: bool,

    /// Current scrollbar drag state (if dragging a scrollbar thumb)
    scrollbar_drag_state: Option<azul_layout::ScrollbarDragState>,

    /// Synchronization for frame readiness (signals when WebRender has a new frame ready)
    new_frame_ready: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl MacOSWindow {
    /// Determine which rendering backend to use
    fn determine_backend(options: &WindowCreateOptions) -> RenderBackend {
        // 1. Check environment variable override
        if let Ok(val) = std::env::var("AZUL_RENDERER") {
            match val.to_lowercase().as_str() {
                "cpu" => return RenderBackend::CPU,
                "opengl" | "gl" => return RenderBackend::OpenGL,
                _ => {}
            }
        }

        // 2. Check options.renderer - if it's Some, check hw_accel field
        use azul_core::window::{HwAcceleration, OptionRendererOptions};
        if let Some(renderer) = options.renderer.as_option() {
            match renderer.hw_accel {
                HwAcceleration::Disabled => return RenderBackend::CPU,
                HwAcceleration::Enabled => return RenderBackend::OpenGL,
                HwAcceleration::DontCare => {} // Continue to default
            }
        }

        // 3. Default: Try OpenGL
        RenderBackend::OpenGL
    }

    /// Create OpenGL view with context and functions
    fn create_gl_view(
        frame: NSRect,
        mtm: MainThreadMarker,
    ) -> Result<(Retained<GLView>, Retained<NSOpenGLContext>, Rc<GlFunctions>), WindowError> {
        // Create pixel format
        let pixel_format = create_opengl_pixel_format(mtm)?;

        // Create GLView
        let gl_view: Option<Retained<GLView>> = unsafe {
            msg_send_id![
                GLView::alloc(mtm),
                initWithFrame: frame,
                pixelFormat: &*pixel_format,
            ]
        };

        let gl_view =
            gl_view.ok_or_else(|| WindowError::PlatformError("Failed to create GLView".into()))?;

        // Enable high-resolution backing store for Retina displays
        unsafe {
            let _: () = msg_send![&*gl_view, setWantsBestResolutionOpenGLSurface: YES];
        }

        // On macOS 10.13+, views automatically become layer-backed shortly after being added to
        // a window. Changing the layer-backedness of a view breaks the association between
        // the view and its associated OpenGL context. To work around this, we explicitly make
        // the view layer-backed up front so that AppKit doesn't do it itself and break the
        // association with its context.
        if unsafe { NSAppKitVersionNumber }.floor() > NSAppKitVersionNumber10_12 {
            let _: () = unsafe { msg_send![&*gl_view, setWantsLayer: YES] };
        }

        // Get OpenGL context
        let gl_context =
            unsafe { gl_view.openGLContext() }.ok_or_else(|| WindowError::ContextCreationFailed)?;

        // Load GL functions
        let gl_functions = GlFunctions::initialize()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load GL: {}", e).into()))?;

        Ok((gl_view, gl_context, Rc::new(gl_functions)))
    }

    /// Create CPU view
    fn create_cpu_view(frame: NSRect, mtm: MainThreadMarker) -> Retained<CPUView> {
        let view: Option<Retained<CPUView>> =
            unsafe { msg_send_id![CPUView::alloc(mtm), initWithFrame: frame] };
        view.expect("Failed to create CPUView")
    }

    /// Create a new macOS window with given options and shared font cache.
    pub fn new_with_fc_cache(
        options: WindowCreateOptions,
        fc_cache: std::sync::Arc<rust_fontconfig::FcFontCache>,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        Self::new_with_options_internal(options, Some(fc_cache), mtm)
    }

    /// Create a new macOS window with given options.
    pub fn new_with_options(
        options: WindowCreateOptions,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        Self::new_with_options_internal(options, None, mtm)
    }

    /// Internal constructor with optional fc_cache parameter
    fn new_with_options_internal(
        options: WindowCreateOptions,
        fc_cache_opt: Option<std::sync::Arc<rust_fontconfig::FcFontCache>>,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        // Initialize NSApplication if needed
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

        // Get screen dimensions for window positioning
        let screen = NSScreen::mainScreen(mtm)
            .ok_or_else(|| WindowError::PlatformError("No main screen".into()))?;
        let screen_frame = screen.frame();

        // Determine window size from options
        let window_size = options.state.size.dimensions;
        let width = window_size.width as f64;
        let height = window_size.height as f64;

        // Center window on screen
        let x = (screen_frame.size.width - width) / 2.0;
        let y = (screen_frame.size.height - height) / 2.0;

        let content_rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));

        // Determine rendering backend
        let requested_backend = Self::determine_backend(&options);

        // Create content view based on backend
        let (backend, gl_view, gl_context, gl_functions, cpu_view) = match requested_backend {
            RenderBackend::OpenGL => match Self::create_gl_view(content_rect, mtm) {
                Ok((view, ctx, funcs)) => (
                    RenderBackend::OpenGL,
                    Some(view),
                    Some(ctx),
                    Some(funcs),
                    None,
                ),
                Err(e) => {
                    eprintln!("OpenGL initialization failed: {}, falling back to CPU", e);
                    let view = Self::create_cpu_view(content_rect, mtm);
                    (RenderBackend::CPU, None, None, None, Some(view))
                }
            },
            RenderBackend::CPU => {
                let view = Self::create_cpu_view(content_rect, mtm);
                (RenderBackend::CPU, None, None, None, Some(view))
            }
        };

        // Create window style mask
        let style_mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        // Create the window
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                content_rect,
                style_mask,
                NSBackingStoreType::Buffered,
                false,
            )
        };

        // Set window title
        let title = NSString::from_str(&options.state.title);
        window.setTitle(&title);

        // Set content view (either GL or CPU)
        // SAFE: Both GLView and CPUView inherit from NSView, so we can upcast safely
        if let Some(ref gl) = gl_view {
            unsafe {
                // GLView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(gl) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
        } else if let Some(ref cpu) = cpu_view {
            unsafe {
                // CPUView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(cpu) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
        } else {
            return Err(WindowError::PlatformError("No content view created".into()));
        }

        // DO NOT show the window yet - we will show it after the first frame is ready
        // to prevent white flash
        unsafe {
            window.center();
            // REMOVED: makeKeyAndOrderFront - will be called after first frame is ready
        }

        // Apply initial window state based on options.state.flags.frame
        // Note: These will be applied before window is visible
        unsafe {
            match options.state.flags.frame {
                azul_core::window::WindowFrame::Fullscreen => {
                    window.toggleFullScreen(None);
                }
                azul_core::window::WindowFrame::Maximized => {
                    window.performZoom(None);
                }
                azul_core::window::WindowFrame::Minimized => {
                    window.miniaturize(None);
                }
                azul_core::window::WindowFrame::Normal => {
                    // Window is already in normal state
                }
            }
        }

        // Create and set window delegate for handling window events
        let window_delegate = WindowDelegate::new(mtm);
        unsafe {
            let delegate_obj = ProtocolObject::from_ref(&*window_delegate);
            window.setDelegate(Some(delegate_obj));
        }

        // Query actual HiDPI factor from NSWindow's screen
        let actual_hidpi_factor = unsafe {
            window
                .screen()
                .map(|screen| screen.backingScaleFactor() as f32)
                .unwrap_or(1.0)
        };

        // TEMP DEBUG: Override HiDPI factor to test DPI scaling issue
        let actual_hidpi_factor = 3.0;
        eprintln!(
            "[Window Init] HiDPI factor: {} (OVERRIDDEN FOR TESTING!)",
            actual_hidpi_factor
        );

        // Make OpenGL context current before initializing WebRender
        if let Some(ref ctx) = gl_context {
            unsafe {
                ctx.makeCurrentContext();
            }
        }

        // Initialize WebRender renderer
        use azul_core::window::{HwAcceleration, RendererType};

        let renderer_type = match backend {
            RenderBackend::OpenGL => RendererType::Hardware,
            RenderBackend::CPU => RendererType::Software,
        };

        eprintln!("[Window Init] Renderer type: {:?}", renderer_type);

        let gl_funcs = if let Some(ref f) = gl_functions {
            eprintln!("[Window Init] Using GL functions from context");
            f.functions.clone()
        } else {
            eprintln!("[Window Init] Loading GL functions for CPU fallback");
            // Fallback for CPU backend - initialize GL functions or fail gracefully
            match gl::GlFunctions::initialize() {
                Ok(f) => f.functions.clone(),
                Err(e) => {
                    return Err(WindowError::PlatformError(format!(
                        "Failed to initialize GL functions: {}",
                        e
                    )));
                }
            }
        };

        eprintln!("[Window Init] Creating WebRender instance");

        // Create synchronization primitives for frame readiness
        let new_frame_ready =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let notifier = Notifier {
            new_frame_ready: new_frame_ready.clone(),
        };

        let (mut renderer, sender) = webrender::create_webrender_instance(
            gl_funcs.clone(),
            Box::new(notifier),
            default_renderer_options(&options),
            None, // shaders cache
        )
        .map_err(|e| {
            WindowError::PlatformError(format!("WebRender initialization failed: {:?}", e))
        })?;

        renderer.set_external_image_handler(Box::new(WrCompositor::default()));

        let mut render_api = sender.create_api();

        // Get physical size for framebuffer (using actual HiDPI factor from screen)
        let physical_size = azul_core::geom::PhysicalSize {
            width: (options.state.size.dimensions.width * actual_hidpi_factor) as u32,
            height: (options.state.size.dimensions.height * actual_hidpi_factor) as u32,
        };

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            physical_size.width as i32,
            physical_size.height as i32,
        );

        // Create WebRender document (one per window)
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // Request hit tester for this document
        let hit_tester = render_api
            .request_hit_tester(wr_translate_document_id(document_id))
            .resolve();

        // Create GlContextPtr for LayoutWindow
        let gl_context_ptr: azul_core::gl::OptionGlContextPtr = gl_context
            .as_ref()
            .map(|_| azul_core::gl::GlContextPtr::new(renderer_type, gl_funcs.clone()))
            .into();

        // Initialize window state with actual HiDPI factor from screen
        let actual_dpi = (actual_hidpi_factor * 96.0) as u32; // Convert scale factor to DPI
        let mut current_window_state = FullWindowState {
            title: options.state.title.clone(),
            size: azul_core::window::WindowSize {
                dimensions: options.state.size.dimensions,
                dpi: actual_dpi, // Use actual DPI from screen
                min_dimensions: options.state.size.min_dimensions,
                max_dimensions: options.state.size.max_dimensions,
            },
            position: options.state.position,
            flags: options.state.flags,
            theme: options.state.theme,
            debug_state: options.state.debug_state,
            keyboard_state: Default::default(),
            mouse_state: Default::default(),
            touch_state: Default::default(),
            ime_position: options.state.ime_position,
            platform_specific_options: options.state.platform_specific_options.clone(),
            renderer_options: options.state.renderer_options,
            background_color: options.state.background_color,
            layout_callback: options.state.layout_callback,
            close_callback: options.state.close_callback.clone(),
            monitor: options.state.monitor,
            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            last_hit_test: azul_layout::hit_test::FullHitTest::empty(None),
            selections: Default::default(),
            window_focused: true,
        };

        // Initialize resource caches
        let image_cache = azul_core::resources::ImageCache::default();
        let renderer_resources = azul_core::resources::RendererResources::default();

        // Initialize LayoutWindow with shared fc_cache or build a new one
        let fc_cache = fc_cache_opt
            .unwrap_or_else(|| std::sync::Arc::new(rust_fontconfig::FcFontCache::build()));
        let mut layout_window = azul_layout::window::LayoutWindow::new((*fc_cache).clone())
            .map_err(|e| {
                WindowError::PlatformError(format!("Failed to create LayoutWindow: {:?}", e))
            })?;

        // Set document_id and id_namespace for this window
        layout_window.document_id = document_id;
        layout_window.id_namespace = id_namespace;
        layout_window.current_window_state = current_window_state.clone();
        layout_window.renderer_type = Some(renderer_type);

        eprintln!(
            "[Window Init] LayoutWindow configured with document_id: {:?}",
            document_id
        );

        // NOTE: Keep OpenGL context current - WebRender needs it for rendering
        // Do NOT call clearCurrentContext() here

        // Initialize shared application data (will be replaced by App later)
        let app_data =
            std::sync::Arc::new(std::cell::RefCell::new(azul_core::refany::RefAny::new(())));

        // NOTE: We will set the window state pointer AFTER creating the MacOSWindow struct
        // because current_window_state will be moved into the struct, invalidating any pointer
        // we create now.

        eprintln!("[Window Init] Window created successfully");
        eprintln!("[Window Init] Backend: {:?}", backend);
        eprintln!("[Window Init] Renderer initialized: true");
        eprintln!(
            "[Window Init] GL Context: {}",
            if gl_context.is_some() { "Some" } else { "None" }
        );

        let mut window = Self {
            window,
            window_delegate,
            backend,
            gl_view,
            gl_context,
            gl_functions,
            cpu_view,
            is_open: true,
            mtm,
            previous_window_state: None,
            current_window_state,
            last_hovered_node: None,
            layout_window: Some(layout_window),
            menu_state: menu::MenuState::new(),
            image_cache,
            renderer_resources,
            render_api,
            renderer: Some(renderer),
            hit_tester: AsyncHitTester::Resolved(hit_tester),
            document_id,
            id_namespace,
            gl_context_ptr,
            app_data,
            fc_cache,
            frame_needs_regeneration: false,
            scrollbar_drag_state: None,
            new_frame_ready,
        };

        // NOTE: Do NOT set the delegate pointer here!
        // The window will be moved out of this function (returned by value),
        // so any pointer we set here will become invalid.
        // Instead, call finalize_delegate_pointer() AFTER the window is in its final location.

        // Set up WebRender document with root pipeline and viewport
        // This only needs to be done once at initialization
        {
            use azul_core::hit_test::PipelineId;
            use webrender::{
                api::units::{
                    DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
                    DeviceIntSize as WrDeviceIntSize,
                },
                render_api::Transaction as WrTransaction,
            };

            let physical_size = window.current_window_state.size.get_physical_size();
            let framebuffer_size =
                WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

            let mut txn = WrTransaction::new();

            // Set root pipeline to root DOM (DomId 0)
            let root_pipeline_id = PipelineId(0, document_id.id);
            txn.set_root_pipeline(wr_translate_pipeline_id(root_pipeline_id));

            // Set document view (viewport size)
            txn.set_document_view(WrDeviceIntRect::from_origin_and_size(
                WrDeviceIntPoint::new(0, 0),
                framebuffer_size,
            ));

            eprintln!("[Window Init] Setting root pipeline and document view");
            window
                .render_api
                .send_transaction(wr_translate_document_id(document_id), txn);
            window.render_api.flush_scene_builder();
        }

        // Perform initial layout and render
        eprintln!("[Window Init] Performing initial layout");
        if let Err(e) = window.regenerate_layout() {
            eprintln!("[Window Init] WARNING: Initial layout failed: {}", e);
        }

        // CRITICAL: Flush scene builder to ensure display list transaction is processed
        // WebRender runs in multi-threaded mode with scene builder on separate thread
        eprintln!("[Window Init] Flushing scene builder after rebuild_display_list");
        window.render_api.flush_scene_builder();

        eprintln!("[Window Init] Generating initial frame");
        window.generate_frame_if_needed();

        // CRITICAL: Flush scene builder again after generate_frame transaction
        eprintln!("[Window Init] Flushing scene builder after generate_frame");
        window.render_api.flush_scene_builder();

        // SOLUTION TO FIRST-FRAME RACE CONDITION:
        // Block this thread until the Notifier signals that the first frame is ready.
        // This prevents showing the window before WebRender has rendered the first frame.
        eprintln!("[Window Init] Waiting for first frame from WebRender...");
        {
            let &(ref lock, ref cvar) = &*window.new_frame_ready;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = cvar.wait(ready).unwrap();
            }
            *ready = false; // Consume the signal
            eprintln!("[Window Init] ✓ First frame is ready!");
        }

        // Now that the first frame is ready in WebRender's backbuffer,
        // we can safely show the window without a white flash.
        eprintln!("[Window Init] Making window visible...");
        unsafe {
            window.window.makeKeyAndOrderFront(None);
        }

        eprintln!("[Window Init] Window initialization complete");
        Ok(window)
    }

    /// Synchronize window state with the OS based on diff between previous and current state
    /// Regenerate layout and display list for the current window.
    ///
    /// This should be called when:
    /// - The window is resized
    /// - The DOM changes (via callbacks)
    /// - Layout callback changes
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        use azul_core::callbacks::LayoutCallback;

        eprintln!("[regenerate_layout] START");

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Borrow app_data from Arc<RefCell<>>
        let mut app_data_borrowed = self.app_data.borrow_mut();

        // Update layout_window's fc_cache with the shared one from App
        layout_window.font_manager.fc_cache = self.fc_cache.clone();

        // 1. Call layout_callback to get styled_dom
        // Use window's cached image_cache and gl_context_ptr instead of creating empty ones
        let mut callback_info = azul_core::callbacks::LayoutCallbackInfo::new(
            self.current_window_state.size.clone(),
            self.current_window_state.theme,
            &self.image_cache,
            &self.gl_context_ptr,
            &*self.fc_cache,
        );

        eprintln!("[regenerate_layout] Calling layout_callback");
        use std::io::Write;
        let _ = std::io::stderr().flush();

        let styled_dom = match &self.current_window_state.layout_callback {
            LayoutCallback::Raw(inner) => (inner.cb)(&mut *app_data_borrowed, &mut callback_info),
            LayoutCallback::Marshaled(marshaled) => (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                &mut *app_data_borrowed,
                &mut callback_info,
            ),
        };

        eprintln!(
            "[regenerate_layout] StyledDom received: {} nodes",
            styled_dom.styled_nodes.len()
        );
        eprintln!(
            "[regenerate_layout] StyledDom hierarchy length: {}",
            styled_dom.node_hierarchy.len()
        );
        let _ = std::io::stderr().flush();

        // 2. Perform layout with solver3
        eprintln!("[regenerate_layout] Calling layout_and_generate_display_list");
        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &self.current_window_state,
                &self.renderer_resources,
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &mut None, // No debug messages for now
            )
            .map_err(|e| format!("Layout error: {:?}", e))?;

        eprintln!(
            "[regenerate_layout] Layout completed, {} DOMs",
            layout_window.layout_results.len()
        );

        // 3. Calculate scrollbar states based on new layout
        // This updates scrollbar geometry (thumb position/size ratios, visibility)
        layout_window.scroll_states.calculate_scrollbar_states();

        // 4. Rebuild display list and send to WebRender
        let dpi = self.current_window_state.size.get_hidpi_factor();
        crate::desktop::wr_translate2::rebuild_display_list(
            layout_window,
            &mut self.render_api,
            &self.image_cache,
            Vec::new(),
            &self.renderer_resources,
            dpi,
        );

        // 5. Synchronize scrollbar opacity with GPU cache AFTER display list submission
        // This enables smooth fade-in/fade-out without display list rebuild
        let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        for (dom_id, layout_result) in &layout_window.layout_results {
            azul_layout::window::LayoutWindow::synchronize_scrollbar_opacity(
                &mut layout_window.gpu_state_manager,
                &layout_window.scroll_states,
                *dom_id,
                &layout_result.layout_tree,
                &system_callbacks,
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    500,
                )), // fade_delay
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    200,
                )), // fade_duration
            );
        }

        // 6. Mark that frame needs regeneration (will be called once at event processing end)
        self.frame_needs_regeneration = true;

        Ok(())
    }

    /// Generate frame if needed and reset flag
    pub fn generate_frame_if_needed(&mut self) {
        if !self.frame_needs_regeneration {
            return;
        }

        if let Some(ref mut layout_window) = self.layout_window {
            crate::desktop::wr_translate2::generate_frame(
                layout_window,
                &mut self.render_api,
                true, // Display list was rebuilt
            );
        }

        self.frame_needs_regeneration = false;
    }

    /// Get the current HiDPI scale factor from the NSWindow's screen
    ///
    /// This queries the actual backing scale factor from the screen,
    /// which can change when the window moves between displays.
    pub fn get_hidpi_factor(&self) -> f32 {
        unsafe {
            self.window
                .screen()
                .map(|screen| screen.backingScaleFactor() as f32)
                .unwrap_or(1.0)
        }
    }

    /// Get the raw window handle for this window
    pub fn get_raw_window_handle(&self) -> azul_core::window::RawWindowHandle {
        use azul_core::window::{MacOSHandle, RawWindowHandle};

        let ns_window_ptr = &*self.window as *const NSWindow as *mut std::ffi::c_void;
        let ns_view_ptr = if let Some(ref gl_view) = self.gl_view {
            &**gl_view as *const GLView as *mut std::ffi::c_void
        } else if let Some(ref cpu_view) = self.cpu_view {
            &**cpu_view as *const CPUView as *mut std::ffi::c_void
        } else {
            std::ptr::null_mut()
        };

        RawWindowHandle::MacOS(MacOSHandle {
            ns_window: ns_window_ptr,
            ns_view: ns_view_ptr,
        })
    }

    /// Handle DPI change notification
    ///
    /// This is called when NSWindowDidChangeBackingPropertiesNotification is received,
    /// indicating the window moved to a display with different DPI.
    pub fn handle_dpi_change(&mut self) -> Result<(), String> {
        let new_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.current_window_state.size.get_hidpi_factor();

        // Only process if DPI actually changed
        if (new_hidpi - old_hidpi).abs() < 0.001 {
            return Ok(());
        }

        eprintln!("[DPI Change] {} -> {}", old_hidpi, new_hidpi);

        // Update window state with new DPI
        self.current_window_state.size.dpi = (new_hidpi * 96.0) as u32;

        // Regenerate layout with new DPI
        self.regenerate_layout()?;

        Ok(())
    }

    /// Perform GPU scrolling - updates scroll transforms without full relayout
    pub fn gpu_scroll(
        &mut self,
        dom_id: u64,
        node_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use std::time::Duration;

        use azul_core::{
            dom::{DomId, NodeId},
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::scroll::ScrollEvent;

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        let dom_id_typed = DomId {
            inner: dom_id as usize,
        };
        let node_id_typed = node_id as u32; // NodeId is u32 in scroll system

        // 1. Create scroll event and process it
        let scroll_event = ScrollEvent {
            dom_id: dom_id_typed,
            node_id: NodeId::new(node_id_typed as usize),
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll using scroll_by instead of apply_scroll_event
        layout_window.scroll_states.scroll_by(
            scroll_event.dom_id,
            scroll_event.node_id,
            scroll_event.delta,
            scroll_event
                .duration
                .unwrap_or(azul_core::task::Duration::System(
                    azul_core::task::SystemTimeDiff { secs: 0, nanos: 0 },
                )),
            scroll_event.easing,
            (external.get_system_time_fn.cb)(),
        );

        // 2. Recalculate scrollbar states after scroll update
        // This updates scrollbar thumb positions based on new scroll offsets
        layout_window.scroll_states.calculate_scrollbar_states();

        // 3. Update WebRender scroll layers and GPU transforms
        let mut txn = crate::desktop::wr_translate2::WrTransaction::new();

        // Scroll all nodes in the scroll manager to WebRender
        // This updates external scroll IDs with new offsets
        crate::desktop::wr_translate2::scroll_all_nodes(layout_window, &mut txn);

        // Synchronize GPU-animated values (transforms, opacities, scrollbar positions)
        // Note: We need mutable access for gpu_state_manager updates
        crate::desktop::wr_translate2::synchronize_gpu_values(layout_window, &mut txn);

        // Send transaction and generate frame (without rebuilding display list)
        self.render_api.send_transaction(
            crate::desktop::wr_translate2::wr_translate_document_id(self.document_id),
            txn,
        );

        crate::desktop::wr_translate2::generate_frame(
            layout_window,
            &mut self.render_api,
            false, // Display list not rebuilt, just transforms updated
        );

        Ok(())
    }

    fn sync_window_state(&mut self) {
        // Get copies of previous and current state to avoid borrow checker issues
        let (previous, current) = match &self.previous_window_state {
            Some(prev) => (prev.clone(), self.current_window_state.clone()),
            None => return, // First frame, nothing to sync
        };

        // Title changed?
        if previous.title != current.title {
            let title = NSString::from_str(&current.title);
            self.window.setTitle(&title);
        }

        // Size changed?
        if previous.size.dimensions != current.size.dimensions {
            let size = NSSize::new(
                current.size.dimensions.width as f64,
                current.size.dimensions.height as f64,
            );
            unsafe {
                self.window.setContentSize(size);
            }
        }

        // Position changed?
        if previous.position != current.position {
            use azul_core::window::WindowPosition;
            match current.position {
                WindowPosition::Initialized(pos) => {
                    let origin = NSPoint::new(pos.x as f64, pos.y as f64);
                    unsafe {
                        self.window.setFrameTopLeftPoint(origin);
                    }
                }
                WindowPosition::Uninitialized => {}
            }
        }

        // Window flags changed?
        if previous.flags != current.flags {
            // Check decorations
            if previous.flags.decorations != current.flags.decorations {
                self.apply_decorations(current.flags.decorations);
            }

            // Check resizable
            if previous.flags.is_resizable != current.flags.is_resizable {
                self.apply_resizable(current.flags.is_resizable);
            }

            // Check background material
            if previous.flags.background_material != current.flags.background_material {
                self.apply_background_material(current.flags.background_material);
            }
        }

        // Visibility changed?
        if previous.flags.is_visible != current.flags.is_visible {
            if current.flags.is_visible {
                self.window.makeKeyAndOrderFront(None);
            } else {
                self.window.orderOut(None);
            }
        }

        // Mouse cursor synchronization - compute from current hit test
        if let Some(layout_window) = self.layout_window.as_ref() {
            let cursor_test = layout_window.compute_cursor_type_hit_test(&current.last_hit_test);
            let cursor_name = self.map_cursor_type_to_macos(cursor_test.cursor_icon);
            self.set_cursor(cursor_name);
        }
    }

    /// Map MouseCursorType to macOS cursor name
    fn map_cursor_type_to_macos(
        &self,
        cursor_type: azul_core::window::MouseCursorType,
    ) -> &'static str {
        use azul_core::window::MouseCursorType;
        match cursor_type {
            MouseCursorType::Default | MouseCursorType::Arrow => "arrow",
            MouseCursorType::Crosshair => "crosshair",
            MouseCursorType::Hand => "pointing_hand",
            MouseCursorType::Move => "open_hand",
            MouseCursorType::Text => "ibeam",
            MouseCursorType::Wait | MouseCursorType::Progress => "arrow",
            MouseCursorType::Help => "arrow",
            MouseCursorType::NotAllowed | MouseCursorType::NoDrop => "operation_not_allowed",
            MouseCursorType::ContextMenu => "arrow",
            MouseCursorType::Cell => "crosshair",
            MouseCursorType::VerticalText => "ibeam",
            MouseCursorType::Alias => "drag_link",
            MouseCursorType::Copy => "drag_copy",
            MouseCursorType::Grab => "open_hand",
            MouseCursorType::Grabbing => "closed_hand",
            MouseCursorType::AllScroll => "open_hand",
            MouseCursorType::ZoomIn | MouseCursorType::ZoomOut => "arrow",
            MouseCursorType::EResize
            | MouseCursorType::WResize
            | MouseCursorType::EwResize
            | MouseCursorType::ColResize => "resize_left_right",
            MouseCursorType::NResize
            | MouseCursorType::SResize
            | MouseCursorType::NsResize
            | MouseCursorType::RowResize => "resize_up_down",
            MouseCursorType::NeResize
            | MouseCursorType::NwResize
            | MouseCursorType::SeResize
            | MouseCursorType::SwResize
            | MouseCursorType::NeswResize
            | MouseCursorType::NwseResize => "arrow",
        }
    }

    /// Update window state at the end of each frame (before rendering)
    ///
    /// This should be called after all callbacks have been processed but before
    /// `present()` is called. It prepares for the next frame by moving current
    /// state to previous state.
    pub fn update_window_state(&mut self, new_state: WindowState) {
        // Save current state as previous for next frame's diff
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update current state from new WindowState
        self.current_window_state.title = new_state.title;
        self.current_window_state.size = new_state.size;
        self.current_window_state.position = new_state.position;
        self.current_window_state.flags = new_state.flags;
        self.current_window_state.theme = new_state.theme;
        self.current_window_state.debug_state = new_state.debug_state;
        self.current_window_state.keyboard_state = new_state.keyboard_state;
        self.current_window_state.mouse_state = new_state.mouse_state;
        self.current_window_state.touch_state = new_state.touch_state;
        self.current_window_state.ime_position = new_state.ime_position;
        self.current_window_state.platform_specific_options = new_state.platform_specific_options;
        self.current_window_state.renderer_options = new_state.renderer_options;
        self.current_window_state.background_color = new_state.background_color;
        self.current_window_state.layout_callback = new_state.layout_callback;
        self.current_window_state.close_callback = new_state.close_callback;
        self.current_window_state.monitor = new_state.monitor;

        // Synchronize with OS
        self.sync_window_state();
    }

    /// Handle windowShouldClose delegate callback
    ///
    /// This is called synchronously when the user clicks the close button.
    /// It invokes the close callback and returns whether the window should close.
    ///
    /// Returns: Ok(true) if window should close, Ok(false) if close was prevented
    fn handle_window_should_close(&mut self) -> Result<bool, String> {
        eprintln!("[handle_window_should_close] START");

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Set close_requested flag
        self.current_window_state.flags.close_requested = true;

        // Invoke close callback if it exists
        // This uses the V2 event system to detect CloseRequested and dispatch callbacks
        let result = self.process_window_events_v2();

        // Process the result - regenerate layout if callback modified DOM
        match result {
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                eprintln!("[handle_window_should_close] Callback requested DOM regeneration");
                if let Err(e) = self.regenerate_layout() {
                    eprintln!(
                        "[handle_window_should_close] Layout regeneration failed: {}",
                        e
                    );
                    // Continue anyway - don't block close on layout errors
                }
            }
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                eprintln!("[handle_window_should_close] Callback requested re-render");
                self.frame_needs_regeneration = true;
            }
            _ => {}
        }

        // Check if callback cleared the flag (preventing close)
        let should_close = self.current_window_state.flags.close_requested;

        if should_close {
            eprintln!("[handle_window_should_close] Close confirmed");
            // Mark window as closed so is_open() returns false
            self.is_open = false;
        } else {
            eprintln!("[handle_window_should_close] Close prevented by callback");
        }

        Ok(should_close)
    }

    /// Handle close request from WindowDelegate
    fn handle_close_request(&mut self) {
        eprintln!("[MacOSWindow] Processing close request");

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Set close_requested flag in current state
        self.current_window_state.flags.close_requested = true;

        // Use V2 event system to detect CloseRequested and dispatch callbacks
        // This allows callbacks to modify DOM or prevent close by clearing the flag
        let result = self.process_window_events_v2();

        // Process the result - regenerate layout if needed
        match result {
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                if let Err(e) = self.regenerate_layout() {
                    eprintln!(
                        "[MacOSWindow] Layout regeneration failed after close callback: {}",
                        e
                    );
                }
            }
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.frame_needs_regeneration = true;
            }
            _ => {}
        }

        // Check if callback cleared the flag (preventing close)
        if self.current_window_state.flags.close_requested {
            eprintln!("[MacOSWindow] Close confirmed, closing window");
            self.close_window();
        } else {
            eprintln!("[MacOSWindow] Close cancelled by callback");
        }
    }

    /// Actually close the window
    fn close_window(&mut self) {
        unsafe {
            self.window.close();
        }
        self.is_open = false;
    }

    /// Check if the window is still open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Apply window decorations changes
    fn apply_decorations(&mut self, decorations: azul_core::window::WindowDecorations) {
        use azul_core::window::WindowDecorations;

        let mut style_mask = self.window.styleMask();

        match decorations {
            WindowDecorations::Normal => {
                // Full decorations with title and controls
                style_mask.insert(NSWindowStyleMask::Titled);
                style_mask.insert(NSWindowStyleMask::Closable);
                style_mask.insert(NSWindowStyleMask::Miniaturizable);
                style_mask.insert(NSWindowStyleMask::Resizable);
                unsafe {
                    self.window.setTitlebarAppearsTransparent(false);
                    self.window
                        .setTitleVisibility(NSWindowTitleVisibility::Visible);
                }
            }
            WindowDecorations::NoTitle => {
                // Extended frame: controls visible but no title
                style_mask.insert(NSWindowStyleMask::Titled);
                style_mask.insert(NSWindowStyleMask::Closable);
                style_mask.insert(NSWindowStyleMask::Miniaturizable);
                style_mask.insert(NSWindowStyleMask::Resizable);
                style_mask.insert(NSWindowStyleMask::FullSizeContentView);
                unsafe {
                    self.window.setTitlebarAppearsTransparent(true);
                    self.window
                        .setTitleVisibility(NSWindowTitleVisibility::Hidden);
                }
            }
            WindowDecorations::NoControls => {
                // Title bar but no controls
                style_mask.insert(NSWindowStyleMask::Titled);
                style_mask.remove(NSWindowStyleMask::Closable);
                style_mask.remove(NSWindowStyleMask::Miniaturizable);
                unsafe {
                    self.window.setTitlebarAppearsTransparent(false);
                    self.window
                        .setTitleVisibility(NSWindowTitleVisibility::Visible);
                }
            }
            WindowDecorations::None => {
                // Borderless window
                style_mask.remove(NSWindowStyleMask::Titled);
                style_mask.remove(NSWindowStyleMask::Closable);
                style_mask.remove(NSWindowStyleMask::Miniaturizable);
                style_mask.remove(NSWindowStyleMask::Resizable);
            }
        }

        self.window.setStyleMask(style_mask);
    }

    /// Apply window visibility
    fn apply_visibility(&mut self, visible: bool) {
        if visible {
            unsafe {
                self.window.makeKeyAndOrderFront(None);
            }
        } else {
            unsafe {
                self.window.orderOut(None);
            }
        }
    }

    /// Apply window resizable state
    fn apply_resizable(&mut self, resizable: bool) {
        let mut style_mask = self.window.styleMask();
        if resizable {
            style_mask.insert(NSWindowStyleMask::Resizable);
        } else {
            style_mask.remove(NSWindowStyleMask::Resizable);
        }
        self.window.setStyleMask(style_mask);
    }

    /// Apply window background material
    fn apply_background_material(&mut self, material: azul_core::window::WindowBackgroundMaterial) {
        use azul_core::window::WindowBackgroundMaterial;
        use objc2_app_kit::{
            NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
            NSVisualEffectView,
        };

        match material {
            WindowBackgroundMaterial::Opaque => {
                // Remove any effect view and restore normal window
                if let Some(content_view) = self.window.contentView() {
                    // Check if content view is an effect view
                    unsafe {
                        let content_ptr = Retained::as_ptr(&content_view);
                        let is_effect_view: bool =
                            msg_send![content_ptr, isKindOfClass: NSVisualEffectView::class()];

                        if is_effect_view {
                            // Get the original view (first subview)
                            let subviews = content_view.subviews();
                            if subviews.count() > 0 {
                                let original_view = subviews.objectAtIndex(0);
                                self.window.setContentView(Some(&original_view));
                            }
                        }

                        self.window.setOpaque(true);
                        self.window.setBackgroundColor(None);
                        self.window.setTitlebarAppearsTransparent(false);
                    }
                }
            }
            WindowBackgroundMaterial::Transparent => {
                // Transparent window without blur
                unsafe {
                    self.window.setOpaque(false);
                    self.window.setBackgroundColor(Some(&NSColor::clearColor()));
                }
            }
            WindowBackgroundMaterial::Sidebar
            | WindowBackgroundMaterial::Menu
            | WindowBackgroundMaterial::HUD
            | WindowBackgroundMaterial::Titlebar
            | WindowBackgroundMaterial::MicaAlt => {
                // Create or update NSVisualEffectView
                let content_view = match self.window.contentView() {
                    Some(view) => view,
                    None => return,
                };

                let ns_material = match material {
                    WindowBackgroundMaterial::Sidebar => NSVisualEffectMaterial::Sidebar,
                    WindowBackgroundMaterial::Menu => NSVisualEffectMaterial::Menu,
                    WindowBackgroundMaterial::HUD => NSVisualEffectMaterial::HUDWindow,
                    WindowBackgroundMaterial::Titlebar => NSVisualEffectMaterial::Titlebar,
                    WindowBackgroundMaterial::MicaAlt => NSVisualEffectMaterial::Titlebar, /* Closest match on macOS */
                    _ => unreachable!(),
                };

                unsafe {
                    let content_ptr = Retained::as_ptr(&content_view);
                    let is_effect_view: bool =
                        msg_send![content_ptr, isKindOfClass: NSVisualEffectView::class()];

                    if is_effect_view {
                        // Update existing effect view
                        let effect_view: *const NSVisualEffectView =
                            content_ptr as *const NSVisualEffectView;
                        (*effect_view).setMaterial(ns_material);
                    } else {
                        // Create new effect view
                        let frame = content_view.frame();
                        let effect_view: Option<Retained<NSVisualEffectView>> =
                            msg_send_id![NSVisualEffectView::alloc(self.mtm), initWithFrame: frame];

                        if let Some(effect_view) = effect_view {
                            effect_view.setMaterial(ns_material);
                            effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                            effect_view.setState(NSVisualEffectState::Active);

                            // Add original view as subview
                            effect_view.addSubview(&content_view);

                            // Set effect view as content view
                            let effect_view_ptr = Retained::as_ptr(&effect_view) as *const NSView;
                            let effect_view_ref = &*effect_view_ptr;
                            self.window.setContentView(Some(effect_view_ref));
                        }
                    }

                    self.window.setOpaque(false);
                    self.window.setBackgroundColor(Some(&NSColor::clearColor()));
                    self.window.setTitlebarAppearsTransparent(true);
                }
            }
        }
    }

    /// Handle a menu action from a menu item click
    fn handle_menu_action(&mut self, tag: isize) {
        eprintln!("[MacOSWindow] Handling menu action for tag: {}", tag);

        // Look up callback index from tag
        if let Some(callback_index) = self.menu_state.get_callback_for_tag(tag as i64) {
            eprintln!(
                "[MacOSWindow] Menu item {} clicked (tag {})",
                callback_index, tag
            );
        } else {
            eprintln!("[MacOSWindow] No callback found for tag: {}", tag);
        }
    }

    /// Check if window is maximized by comparing frame to screen size
    ///
    /// Updates the window frame state based on the actual window and screen dimensions.
    /// Should be called after resize events.
    fn check_maximized_state(&mut self) {
        // Skip check if in fullscreen mode
        if self.current_window_state.flags.frame == azul_core::window::WindowFrame::Fullscreen {
            return;
        }

        let window_frame = self.window.frame();

        // Get the visible frame of the screen (excludes menu bar and dock)
        let screen_frame = unsafe {
            if let Some(screen) = self.window.screen() {
                screen.visibleFrame()
            } else {
                // No screen available, can't determine maximized state
                return;
            }
        };

        // Consider window maximized if it matches the screen's visible frame
        // Allow small tolerance for rounding errors
        let tolerance = 5.0;
        let is_maximized = (window_frame.origin.x - screen_frame.origin.x).abs() < tolerance
            && (window_frame.origin.y - screen_frame.origin.y).abs() < tolerance
            && (window_frame.size.width - screen_frame.size.width).abs() < tolerance
            && (window_frame.size.height - screen_frame.size.height).abs() < tolerance;

        let new_frame = if is_maximized {
            azul_core::window::WindowFrame::Maximized
        } else {
            azul_core::window::WindowFrame::Normal
        };

        if new_frame != self.current_window_state.flags.frame {
            self.current_window_state.flags.frame = new_frame;
            eprintln!("[MacOSWindow] Window frame changed to: {:?}", new_frame);
        }
    }

    /// Set the application menu
    ///
    /// Updates the macOS menu bar with the provided menu structure.
    /// Uses hash-based diffing to avoid unnecessary menu recreation.
    pub fn set_application_menu(&mut self, menu: &azul_core::menu::Menu) {
        if self.menu_state.update_if_changed(menu, self.mtm) {
            eprintln!("[MacOSWindow] Application menu updated");
            if let Some(ns_menu) = self.menu_state.get_nsmenu() {
                let app = NSApplication::sharedApplication(self.mtm);
                app.setMainMenu(Some(ns_menu));
            }
        }
    }

    /// Process an NSEvent and dispatch to appropriate handler
    fn process_event(&mut self, event: &NSEvent, macos_event: &MacOSEvent) {
        use azul_core::events::MouseButton;

        match event.r#type() {
            NSEventType::LeftMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Left);
            }
            NSEventType::LeftMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Left);
            }
            NSEventType::RightMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Right);
            }
            NSEventType::RightMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Right);
            }
            NSEventType::OtherMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Middle);
            }
            NSEventType::OtherMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Middle);
            }
            NSEventType::MouseMoved
            | NSEventType::LeftMouseDragged
            | NSEventType::RightMouseDragged => {
                let _ = self.handle_mouse_move(event);
            }
            NSEventType::MouseEntered => {
                let _ = self.handle_mouse_entered(event);
            }
            NSEventType::MouseExited => {
                let _ = self.handle_mouse_exited(event);
            }
            NSEventType::ScrollWheel => {
                let _ = self.handle_scroll_wheel(event);
            }
            NSEventType::KeyDown => {
                let _ = self.handle_key_down(event);
            }
            NSEventType::KeyUp => {
                let _ = self.handle_key_up(event);
            }
            NSEventType::FlagsChanged => {
                let _ = self.handle_flags_changed(event);
            }
            _ => {
                // Other events not handled yet
            }
        }

        // After processing event, regenerate frame if needed
        let needs_flush = self.frame_needs_regeneration;
        self.generate_frame_if_needed();

        // Flush scene builder if frame was regenerated (WebRender multi-threading)
        if needs_flush {
            self.render_api.flush_scene_builder();
            // Request a redraw to display the new frame
            // This tells macOS to schedule a drawRect: call
            self.request_redraw();
        }
    }

    /// Set the mouse cursor to a specific system cursor
    ///
    /// # Cursor Types (macOS)
    /// - "arrow" - Standard arrow
    /// - "ibeam" - I-beam text cursor
    /// - "crosshair" - Crosshair
    /// - "pointing_hand" - Pointing hand (link cursor)
    /// - "resize_left_right" - Horizontal resize
    /// - "resize_up_down" - Vertical resize
    /// - "open_hand" - Open hand (grab)
    /// - "closed_hand" - Closed hand (grabbing)
    /// - "disappearing_item" - Disappearing item (poof)
    pub fn set_cursor(&self, cursor_type: &str) {
        use objc2_app_kit::NSCursor;

        unsafe {
            let cursor = match cursor_type {
                "arrow" => NSCursor::arrowCursor(),
                "ibeam" | "text" => NSCursor::IBeamCursor(),
                "crosshair" => NSCursor::crosshairCursor(),
                "pointing_hand" | "pointer" | "hand" => NSCursor::pointingHandCursor(),
                "resize_left_right" | "ew-resize" => NSCursor::resizeLeftRightCursor(),
                "resize_up_down" | "ns-resize" => NSCursor::resizeUpDownCursor(),
                "open_hand" | "grab" => NSCursor::openHandCursor(),
                "closed_hand" | "grabbing" => NSCursor::closedHandCursor(),
                "disappearing_item" | "no-drop" => NSCursor::disappearingItemCursor(),
                "drag_copy" | "copy" => NSCursor::dragCopyCursor(),
                "drag_link" | "alias" => NSCursor::dragLinkCursor(),
                "operation_not_allowed" | "not-allowed" => NSCursor::operationNotAllowedCursor(),
                _ => NSCursor::arrowCursor(), // Default fallback
            };
            cursor.set();
        }
    }

    /// Hide the mouse cursor
    pub fn hide_cursor(&self) {
        use objc2_app_kit::NSCursor;
        unsafe {
            NSCursor::hide();
        }
    }

    /// Show the mouse cursor
    pub fn show_cursor(&self) {
        use objc2_app_kit::NSCursor;
        unsafe {
            NSCursor::unhide();
        }
    }

    /// Reset cursor to default arrow
    pub fn reset_cursor(&self) {
        self.set_cursor("arrow");
    }

    // =========================================================================
    // RENDERING METHODS - macOS Drawing Model Integration
    // =========================================================================

    /// Set up the GLView's back-pointer to this MacOSWindow.
    ///
    /// This MUST be called after window construction to enable drawRect: to find
    /// the window and call render_and_present_in_draw_rect().
    ///
    /// SAFETY: This creates a self-referential pointer. The caller must ensure:
    /// - The window is not moved in memory (use Box/Arc or keep it on the stack)
    /// - The view is owned by the window and doesn't outlive it
    pub unsafe fn setup_gl_view_back_pointer(&mut self) {
        // Get the window pointer first, before borrowing gl_view
        let window_ptr = self as *mut MacOSWindow as *mut std::ffi::c_void;

        if let Some(ref gl_view) = self.gl_view {
            gl_view.set_window_ptr(window_ptr);
            eprintln!("[setup_gl_view_back_pointer] ✓ GLView can now call back to MacOSWindow");
        }
    }

    /// Finalize the delegate's back-pointer to this window.
    ///
    /// MUST be called AFTER the window is in its final memory location.
    /// Do NOT call this from the constructor, as the window will be moved after creation.
    ///
    /// SAFETY:
    /// - The window must not be moved in memory after this call
    /// - The delegate is owned by the window and doesn't outlive it
    pub unsafe fn finalize_delegate_pointer(&mut self) {
        let window_ptr = self as *mut MacOSWindow as *mut std::ffi::c_void;
        let delegate_ptr = &*self.window_delegate as *const WindowDelegate;
        (*delegate_ptr).set_window_ptr(window_ptr);
        eprintln!("[finalize_delegate_pointer] ✓ WindowDelegate can now call back to MacOSWindow");
    }

    /// This is the MAIN rendering entry point, called ONLY from GLView::drawRect:
    ///
    /// This method follows the idiomatic macOS drawing pattern where all rendering
    /// happens inside drawRect:. It:
    /// 1. Makes the GL context current and updates it
    /// 2. Sets the viewport
    /// 3. Calls renderer.update() and renderer.render() to composite WebRender's scene
    /// 4. Swaps buffers via flushBuffer()
    ///
    /// IMPORTANT: This should NEVER be called directly from Rust code. It's only
    /// called by the Objective-C drawRect: method when macOS schedules a redraw.
    pub fn render_and_present_in_draw_rect(&mut self) -> Result<(), WindowError> {
        eprintln!("[render_and_present_in_draw_rect] START");

        // Step 1: Prepare OpenGL context (if using OpenGL backend)
        if self.backend == RenderBackend::OpenGL {
            let gl_context = self
                .gl_context
                .as_ref()
                .ok_or_else(|| WindowError::PlatformError("OpenGL context is missing".into()))?;

            let gl_fns = self
                .gl_functions
                .as_ref()
                .ok_or_else(|| WindowError::PlatformError("OpenGL functions are missing".into()))?;

            unsafe {
                // Make context current before any GL operations
                gl_context.makeCurrentContext();

                // CRITICAL: Synchronize context with the view's drawable surface
                // This must be called every frame to handle window moves/resizes
                gl_context.update(self.mtm);

                // CRITICAL: Set the viewport to the physical size of the window
                let physical_size = self.current_window_state.size.get_physical_size();
                eprintln!(
                    "[render_and_present_in_draw_rect] Setting glViewport to: {}x{}",
                    physical_size.width, physical_size.height
                );
                gl_fns.functions.viewport(
                    0,
                    0,
                    physical_size.width as i32,
                    physical_size.height as i32,
                );
            }
        }

        // Step 2: Call WebRender to composite the scene
        if let Some(ref mut renderer) = self.renderer {
            eprintln!("[render_and_present_in_draw_rect] Calling renderer.update()");
            renderer.update();

            let physical_size = self.current_window_state.size.get_physical_size();
            let device_size = webrender::api::units::DeviceIntSize::new(
                physical_size.width as i32,
                physical_size.height as i32,
            );

            eprintln!(
                "[render_and_present_in_draw_rect] Calling renderer.render() with size: {:?}",
                device_size
            );

            match renderer.render(device_size, 0) {
                Ok(results) => {
                    eprintln!(
                        "[render_and_present_in_draw_rect] ✓ Render successful! Stats: {:?}",
                        results.stats
                    );
                }
                Err(errors) => {
                    eprintln!(
                        "[render_and_present_in_draw_rect] ✗ Render errors: {:?}",
                        errors
                    );
                    return Err(WindowError::PlatformError(
                        format!("WebRender render failed: {:?}", errors).into(),
                    ));
                }
            }
        } else {
            eprintln!("[render_and_present_in_draw_rect] WARNING: No renderer available!");
            return Ok(());
        }

        // Step 3: Swap buffers to show the rendered frame
        match self.backend {
            RenderBackend::OpenGL => {
                if let Some(ref gl_context) = self.gl_context {
                    eprintln!("[render_and_present_in_draw_rect] Flushing OpenGL buffer");
                    unsafe {
                        gl_context.flushBuffer();
                    }
                }
            }
            RenderBackend::CPU => {
                // CPU backend doesn't need explicit buffer swap
                // The drawRect: itself updates the view
            }
        }

        eprintln!("[render_and_present_in_draw_rect] DONE");
        Ok(())
    }
}

impl PlatformWindow for MacOSWindow {
    type EventType = MacOSEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;
        Self::new_with_options(options, mtm)
    }

    fn get_state(&self) -> WindowState {
        let frame = self.window.frame();
        let mut state = WindowState::default();

        // Update size (dimensions is LogicalSize)
        state.size.dimensions.width = frame.size.width as f32;
        state.size.dimensions.height = frame.size.height as f32;

        // Update title
        state.title = self.window.title().to_string().into();

        state
    }

    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        // Update current_window_state based on properties
        if let Some(title) = props.title {
            self.current_window_state.title = title.into();
        }

        if let Some(size) = props.size {
            use azul_core::geom::LogicalSize;
            // Get actual DPI scale from window
            let scale_factor = unsafe {
                self.window
                    .screen()
                    .map(|screen| screen.backingScaleFactor())
                    .unwrap_or(1.0)
            };

            // Convert PhysicalSize to LogicalSize using actual DPI
            self.current_window_state.size.dimensions = LogicalSize {
                width: (size.width as f64 / scale_factor) as f32,
                height: (size.height as f64 / scale_factor) as f32,
            };
        }

        if let Some(visible) = props.visible {
            self.current_window_state.flags.is_visible = visible;
        }

        // Synchronize changes with the OS
        self.sync_window_state();

        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        // Check if a frame is ready without blocking
        let frame_ready = {
            let &(ref lock, _) = &*self.new_frame_ready;
            let mut ready_guard = lock.lock().unwrap();
            if *ready_guard {
                *ready_guard = false; // Consume the signal
                true
            } else {
                false
            }
        };

        if frame_ready {
            eprintln!("[poll_event] Frame ready signal received - requesting redraw");
            // A frame is ready in WebRender's backbuffer.
            // Tell macOS to schedule a drawRect: call, which will display it.
            self.request_redraw();
        }

        // Check for close request from WindowDelegate
        if self.current_window_state.flags.close_requested {
            self.current_window_state.flags.close_requested = false;
            self.handle_close_request();
        }

        // Process pending menu actions
        let pending_actions = menu::take_pending_menu_actions();
        for tag in pending_actions {
            self.handle_menu_action(tag);
        }

        let app = NSApplication::sharedApplication(self.mtm);

        // Poll event (non-blocking)
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                None, // No wait time = non-blocking
                objc2_foundation::NSDefaultRunLoopMode,
                true,
            )
        };

        if let Some(event) = event {
            // Convert and process event
            let macos_event = MacOSEvent::from_nsevent(&event);

            // Dispatch event to handlers
            self.process_event(&event, &macos_event);

            // Check for maximized state after processing events
            // This handles window resize/zoom events
            self.check_maximized_state();

            // Forward event to system
            unsafe {
                app.sendEvent(&event);
            }

            // Generate frame if needed after event processing
            self.generate_frame_if_needed();

            Some(macos_event)
        } else {
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match self.backend {
            RenderBackend::OpenGL => {
                let context_ptr = self
                    .gl_context
                    .as_ref()
                    .map(|ctx| Retained::as_ptr(ctx) as *mut _)
                    .unwrap_or(std::ptr::null_mut());

                RenderContext::OpenGL {
                    context: context_ptr,
                }
            }
            RenderBackend::CPU => RenderContext::CPU,
        }
    }

    // In macos/mod.rs

    /// Present the rendered frame to the screen.
    ///
    /// NOTE: In the macOS drawing model, this is now a NO-OP for the OpenGL backend.
    /// All rendering is driven by drawRect:, which is triggered by calling request_redraw().
    ///
    /// This method exists only to satisfy the PlatformWindow trait.
    fn present(&mut self) -> Result<(), WindowError> {
        // Rendering is now handled by drawRect: -> render_and_present_in_draw_rect()
        // This method is effectively deprecated for the macOS backend.
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn close(&mut self) {
        self.window.close();
        self.is_open = false;
    }

    /// Request a redraw of the window.
    ///
    /// This is the idiomatic macOS way to trigger rendering: we call setNeedsDisplay(true)
    /// on the content view, which tells macOS to schedule a drawRect: call on the next
    /// display refresh cycle.
    ///
    /// This decouples our asynchronous rendering backend (WebRender) from the synchronous
    /// OS drawing model.
    fn request_redraw(&mut self) {
        eprintln!("[request_redraw] Marking view as needing display");

        // Tell macOS to schedule a drawRect: call
        if let Some(view) = unsafe { self.window.contentView() } {
            unsafe {
                view.setNeedsDisplay(true);
            }
        }

        self.frame_needs_regeneration = true;
    }
}

/// macOS event type.
#[derive(Debug, Clone, Copy)]
pub enum MacOSEvent {
    /// Window close requested
    Close,
    /// Window resized
    Resize { width: u32, height: u32 },
    /// Mouse moved
    MouseMove { x: f64, y: f64 },
    /// Mouse button pressed
    MouseDown { button: u8, x: f64, y: f64 },
    /// Mouse button released
    MouseUp { button: u8, x: f64, y: f64 },
    /// Key pressed
    KeyDown { key_code: u16 },
    /// Key released
    KeyUp { key_code: u16 },
    /// Other event
    Other,
}

impl MacOSEvent {
    /// Convert NSEvent to MacOSEvent.
    fn from_nsevent(event: &NSEvent) -> Self {
        match event.r#type() {
            NSEventType::LeftMouseDown => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseDown {
                    button: 0,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::LeftMouseUp => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseUp {
                    button: 0,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::RightMouseDown => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseDown {
                    button: 1,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::RightMouseUp => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseUp {
                    button: 1,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::MouseMoved => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseMove { x: loc.x, y: loc.y }
            }
            NSEventType::KeyDown => MacOSEvent::KeyDown {
                key_code: event.keyCode(),
            },
            NSEventType::KeyUp => MacOSEvent::KeyUp {
                key_code: event.keyCode(),
            },
            _ => MacOSEvent::Other,
        }
    }
}
