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
    sync::{Arc, Condvar, Mutex},
};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    menu::Menu,
    refany::RefAny,
    resources::{DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        HwAcceleration, MacOSHandle, MouseCursorType, RawWindowHandle, RendererType,
        WindowBackgroundMaterial, WindowDecorations, WindowFrame, WindowPosition, WindowSize,
    },
};
use azul_css::corety::OptionU32;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    hit_test::FullHitTest,
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
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
    NSSize, NSString, NSTimer, NSUndoManager,
};
use rust_fontconfig::FcFontCache;

use crate::desktop::{
    shell2::common::{
        self,
        event_v2::{self, PlatformWindowV2}, // Import event_v2 module AND trait
        Compositor,
        CompositorError,
        CompositorMode,
        PlatformWindow,
        RenderContext,
        WindowError,
        WindowProperties,
    },
    wr_translate2::{
        default_renderer_options, translate_document_id_wr, translate_id_namespace_wr,
        wr_translate_document_id, wr_translate_pipeline_id, AsyncHitTester,
        Compositor as WrCompositor, Notifier, WrRenderApi, WrTransaction, WR_SHADER_CACHE,
    },
};

pub mod accessibility;
pub mod clipboard;
mod coregraphics;
mod corevideo;
mod events;
mod gl;
mod menu;
pub mod registry;
mod tooltip;

use coregraphics::CoreGraphicsFunctions;
use corevideo::CoreVideoFunctions;
use events::HitTestNode;
use gl::GlFunctions;

// IOKit FFI - Power Management (IOPMAssertion)

type IOPMAssertionID = u32;
type IOReturn = i32;

const kIOReturnSuccess: IOReturn = 0;

// IOPMAssertion types
#[allow(non_upper_case_globals)]
const kIOPMAssertionTypeNoDisplaySleep: &str = "PreventUserIdleDisplaySleep";

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: *const objc2_foundation::NSString,
        assertion_level: u32,
        assertion_name: *const objc2_foundation::NSString,
        assertion_id: *mut IOPMAssertionID,
    ) -> IOReturn;

    fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
}

const kIOPMAssertionLevelOn: u32 = 255;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RenderBackend {
    OpenGL,
    CPU,
}

// GLView - OpenGL rendering view

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
                println!("render_and_present_in_draw_rect: THIS IS WHERE macos_window.render_and_present_in_draw_rect() will be called");
                if let Err(e) = macos_window.render_and_present_in_draw_rect() {
                    eprintln!("[GLView] drawRect: Error during rendering: {:?}", e);
                }
            }
        }

        #[unsafe(method(prepareOpenGL))]
        fn prepare_opengl(&self) {
            eprintln!("[GLView] prepareOpenGL called!");
            // Load GL functions via dlopen
            match GlFunctions::initialize() {
                Ok(functions) => {
                    eprintln!("[GLView] prepareOpenGL: GL functions loaded successfully");
                    *self.ivars().gl_functions.borrow_mut() = Some(functions.get_context());
                    self.ivars().needs_reshape.set(true);
                }
                Err(e) => {
                    eprintln!("Failed to load GL functions: {}", e);
                }
            }
            eprintln!("[GLView] prepareOpenGL done");
        }

        #[unsafe(method(reshape))]
        fn reshape(&self) {
            eprintln!("[GLView] reshape called!");
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

        // Event Handling

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

        // NSResponder Undo/Redo Support
        // These methods are called automatically by macOS when Cmd+Z / Cmd+Shift+Z are pressed

        #[unsafe(method(undo:))]
        fn undo(&self, _sender: Option<&NSObject>) {
            // Forward to MacOSWindow for actual undo logic
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.perform_undo();
                }
            }
        }

        #[unsafe(method(redo:))]
        fn redo(&self, _sender: Option<&NSObject>) {
            // Forward to MacOSWindow for actual redo logic
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.perform_redo();
                }
            }
        }

        #[unsafe(method(validateUserInterfaceItem:))]
        fn validate_user_interface_item(&self, item: &ProtocolObject<dyn NSObjectProtocol>) -> Bool {
            // Check if we can undo/redo and enable/disable menu items accordingly
            use objc2::sel;
            use objc2::runtime::{AnyObject, Sel};

            // Try to get the action from the item (if it's an NSMenuItem)
            let action: Option<Sel> = unsafe {
                let obj = item as *const _ as *const AnyObject;
                objc2::msg_send![obj, action]
            };

            if action == Some(sel!(undo:)) {
                if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                    unsafe {
                        let macos_window = &*(window_ptr as *const MacOSWindow);
                        return Bool::from(macos_window.can_undo());
                    }
                }
                return Bool::from(false);
            } else if action == Some(sel!(redo:)) {
                if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                    unsafe {
                        let macos_window = &*(window_ptr as *const MacOSWindow);
                        return Bool::from(macos_window.can_redo());
                    }
                }
                return Bool::from(false);
            }

            Bool::from(true) // Default: enable other items
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

        // NSTextInputClient Protocol
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
            // Phase 2: OnCompositionStart callback - sync IME position
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.sync_ime_position_to_os();
                }
            }
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
            // Get the back-pointer to our MacOSWindow
            let window_ptr = match self.get_window_ptr() {
                Some(ptr) => ptr,
                None => return,
            };

            // SAFETY: We trust that the window pointer is valid.
            unsafe {
                let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                if let Some(ns_string) = string.downcast_ref::<NSString>() {
                    let text = ns_string.to_string();
                    macos_window.handle_text_input(&text);
                }
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
            use azul_core::window::ImePosition;

            // Get ime_position from window state
            let window_ptr = match self.get_window_ptr() {
                Some(ptr) => ptr,
                None => return NSRect::ZERO,
            };

            unsafe {
                let window = &*(window_ptr as *const MacOSWindow);
                if let ImePosition::Initialized(rect) = window.current_window_state.ime_position {
                    // Convert from window-local coordinates to screen coordinates
                    let window_frame = window.window.frame();

                    return NSRect {
                        origin: NSPoint {
                            x: window_frame.origin.x + rect.origin.x as f64,
                            y: window_frame.origin.y + rect.origin.y as f64,
                        },
                        size: NSSize {
                            width: rect.size.width as f64,
                            height: rect.size.height as f64,
                        },
                    };
                }
            }

            NSRect::ZERO
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: objc2::runtime::Sel) {
            // Called for special key commands during IME
        }
    }
);

// CPUView - CPU rendering view

/// Instance variables for CPUView
pub struct CPUViewIvars {
    framebuffer: RefCell<Vec<u8>>,
    width: Cell<usize>,
    height: Cell<usize>,
    needs_redraw: Cell<bool>,
    tracking_area: RefCell<Option<Retained<NSTrackingArea>>>,
    mtm: MainThreadMarker, // Store MainThreadMarker to avoid unsafe new_unchecked
    window_ptr: RefCell<Option<*mut std::ffi::c_void>>, // Back-pointer to MacOSWindow
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

        // Event Handling

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

        // NSResponder Undo/Redo Support
        // These methods are called automatically by macOS when Cmd+Z / Cmd+Shift+Z are pressed

        #[unsafe(method(undo:))]
        fn undo(&self, _sender: Option<&NSObject>) {
            // Forward to MacOSWindow for actual undo logic
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.perform_undo();
                }
            }
        }

        #[unsafe(method(redo:))]
        fn redo(&self, _sender: Option<&NSObject>) {
            // Forward to MacOSWindow for actual redo logic
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.perform_redo();
                }
            }
        }

        #[unsafe(method(validateUserInterfaceItem:))]
        fn validate_user_interface_item(&self, item: &ProtocolObject<dyn NSObjectProtocol>) -> Bool {
            // Check if we can undo/redo and enable/disable menu items accordingly
            use objc2::sel;
            use objc2::runtime::{AnyObject, Sel};

            // Try to get the action from the item (if it's an NSMenuItem)
            let action: Option<Sel> = unsafe {
                let obj = item as *const _ as *const AnyObject;
                objc2::msg_send![obj, action]
            };

            if action == Some(sel!(undo:)) {
                if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                    unsafe {
                        let macos_window = &*(window_ptr as *const MacOSWindow);
                        return Bool::from(macos_window.can_undo());
                    }
                }
                return Bool::from(false);
            } else if action == Some(sel!(redo:)) {
                if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                    unsafe {
                        let macos_window = &*(window_ptr as *const MacOSWindow);
                        return Bool::from(macos_window.can_redo());
                    }
                }
                return Bool::from(false);
            }

            Bool::from(true) // Default: enable other items
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
                window_ptr: RefCell::new(None),
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

        // NSTextInputClient Protocol
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
            // Phase 2: OnCompositionStart callback - sync IME position
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.sync_ime_position_to_os();
                }
            }
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
            // Get the back-pointer to our MacOSWindow
            let window_ptr = match self.get_window_ptr() {
                Some(ptr) => ptr,
                None => return,
            };

            // SAFETY: We trust that the window pointer is valid.
            unsafe {
                let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                if let Some(ns_string) = string.downcast_ref::<NSString>() {
                    let text = ns_string.to_string();
                    macos_window.handle_text_input(&text);
                }
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
            use azul_core::window::ImePosition;

            // Get ime_position from window state
            let window_ptr = match self.get_window_ptr() {
                Some(ptr) => ptr,
                None => return NSRect::ZERO,
            };

            unsafe {
                let window = &*(window_ptr as *const MacOSWindow);
                if let ImePosition::Initialized(rect) = window.current_window_state.ime_position {
                    // Convert from window-local coordinates to screen coordinates
                    let window_frame = window.window.frame();

                    return NSRect {
                        origin: NSPoint {
                            x: window_frame.origin.x + rect.origin.x as f64,
                            y: window_frame.origin.y + rect.origin.y as f64,
                        },
                        size: NSSize {
                            width: rect.size.width as f64,
                            height: rect.size.height as f64,
                        },
                    };
                }
            }

            NSRect::ZERO
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: objc2::runtime::Sel) {
        }
    }
);

// GLView Helper Methods (outside define_class!)

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

// CPUView Helper Methods (outside define_class!)

impl CPUView {
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

// WindowDelegate - Handles window lifecycle events (close, resize, etc.)

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

        /// Called when the window is about to close
        /// This is where we unregister the window from the global registry
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            eprintln!("[WindowDelegate] Window will close, unregistering from registry");

            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    let ns_window = macos_window.get_ns_window_ptr();

                    // Unregister from global window registry
                    registry::unregister_window(ns_window);
                    eprintln!("[WindowDelegate] Window unregistered, remaining windows: {}", registry::window_count());
                }
            }
        }

        /// Called when the window is minimized to the Dock
        #[unsafe(method(windowDidMiniaturize:))]
        fn window_did_miniaturize(&self, _notification: &NSNotification) {
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let macos_window = &mut *(window_ptr as *mut MacOSWindow);
                    macos_window.current_window_state.flags.frame = WindowFrame::Minimized;
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
                    macos_window.current_window_state.flags.frame = WindowFrame::Normal;
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
                    macos_window.current_window_state.flags.frame = WindowFrame::Fullscreen;
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
                    macos_window.current_window_state.flags.frame = WindowFrame::Normal;
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
                    if frame != WindowFrame::Fullscreen {
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

                    // Phase 2: OnFocus callback - sync IME position after focus
                    macos_window.sync_ime_position_to_os();
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

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            // DPI/scale factor changed (e.g., moved to different display)
            if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
                unsafe {
                    let window = &mut *(window_ptr as *mut MacOSWindow);
                    if let Err(e) = window.handle_dpi_change() {
                        eprintln!("[macOS] DPI change error: {}", e);
                    }
                }
            }
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
    ///
    /// SAFETY: Caller must ensure the pointer remains valid for the lifetime of the delegate
    pub unsafe fn set_window_ptr(&self, window_ptr: *mut std::ffi::c_void) {
        *self.ivars().window_ptr.borrow_mut() = Some(window_ptr);
    }
}

// Helper Functions

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

// MacOSWindow - Main window implementation

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
    /// OpenGL context
    gl_context: Option<Retained<NSOpenGLContext>>,
    /// OpenGL function loader
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
    last_hovered_node: Option<HitTestNode>,
    /// LayoutWindow integration (for UI callbacks and display list)
    layout_window: Option<LayoutWindow>,
    /// Menu state (for hash-based diff updates)
    menu_state: menu::MenuState,

    // Resource caches for LayoutWindow
    /// Image cache for texture management
    image_cache: ImageCache,
    /// Renderer resources (GPU textures, etc.)
    renderer_resources: RendererResources,

    // WebRender infrastructure for proper hit-testing and rendering
    /// Main render API for registering fonts, images, display lists
    pub render_api: webrender::RenderApi,
    /// WebRender renderer (software or hardware depending on backend)
    pub renderer: Option<webrender::Renderer>,
    /// Hit-tester for fast asynchronous hit-testing (updated on layout changes)
    pub hit_tester: crate::desktop::wr_translate2::AsyncHitTester,
    /// WebRender document ID
    pub document_id: DocumentId,
    /// WebRender ID namespace
    pub id_namespace: IdNamespace,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub gl_context_ptr: OptionGlContextPtr,

    // Application-level shared state
    /// Shared application data (used by callbacks, shared across windows)
    app_data: Arc<RefCell<RefAny>>,
    /// Shared font cache (shared across windows to cache font loading)
    fc_cache: Arc<rust_fontconfig::FcFontCache>,
    /// System style (shared across windows)
    system_style: Arc<azul_css::system::SystemStyle>,
    /// Track if frame needs regeneration (to avoid multiple generate_frame calls)
    frame_needs_regeneration: bool,
    /// Current scrollbar drag state (if dragging a scrollbar thumb)
    scrollbar_drag_state: Option<ScrollbarDragState>,
    /// Synchronization for frame readiness (signals when WebRender has a new frame ready)
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,

    // Accessibility support
    /// Accessibility adapter for NSAccessibility integration (macOS screen readers)
    #[cfg(feature = "a11y")]
    accessibility_adapter: Option<accessibility::MacOSAccessibilityAdapter>,

    // Multi-window support
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    /// Processed in Phase 3 of the event loop
    pub pending_window_creates: Vec<WindowCreateOptions>,

    // Tooltip
    /// Tooltip panel (for programmatic tooltip display)
    tooltip: Option<tooltip::TooltipWindow>,

    // Power Management
    /// IOPMAssertion ID for preventing system sleep (video playback)
    pm_assertion_id: Option<IOPMAssertionID>,

    // Timers and threads
    /// Active timers (TimerId -> NSTimer object)
    timers: std::collections::HashMap<usize, Retained<objc2_foundation::NSTimer>>,
    /// Thread timer (for polling thread messages every 16ms)
    thread_timer_running: Option<Retained<objc2_foundation::NSTimer>>,

    // VSYNC and Display Management
    /// CVDisplayLink for proper VSYNC synchronization (optional, loaded via dlopen)
    display_link: Option<corevideo::DisplayLink>,
    /// CoreVideo functions (loaded via dlopen for backward compatibility)
    cv_functions: Option<Arc<CoreVideoFunctions>>,
    /// Core Graphics functions (for display enumeration)
    cg_functions: Option<Arc<CoreGraphicsFunctions>>,
    /// Current display ID (CGDirectDisplayID) for this window
    current_display_id: Option<u32>,
}

// Implement PlatformWindowV2 trait for cross-platform event processing

impl event_v2::PlatformWindowV2 for MacOSWindow {
    // REQUIRED: Simple Getter Methods

    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }

    fn get_layout_window(&self) -> Option<&LayoutWindow> {
        self.layout_window.as_ref()
    }

    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }

    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState {
        &mut self.current_window_state
    }

    fn get_previous_window_state(&self) -> &Option<FullWindowState> {
        &self.previous_window_state
    }

    fn set_previous_window_state(&mut self, state: FullWindowState) {
        self.previous_window_state = Some(state);
    }

    fn get_image_cache_mut(&mut self) -> &mut ImageCache {
        &mut self.image_cache
    }

    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources {
        &mut self.renderer_resources
    }

    fn get_fc_cache(&self) -> &Arc<FcFontCache> {
        &self.fc_cache
    }

    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
        &self.gl_context_ptr
    }

    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
        &self.system_style
    }

    fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
        &self.app_data
    }

    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState> {
        self.scrollbar_drag_state.as_ref()
    }

    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState> {
        &mut self.scrollbar_drag_state
    }

    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>) {
        self.scrollbar_drag_state = state;
    }

    fn get_hit_tester(&self) -> &AsyncHitTester {
        &self.hit_tester
    }

    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
        &mut self.hit_tester
    }

    fn get_last_hovered_node(&self) -> Option<&event_v2::HitTestNode> {
        self.last_hovered_node.as_ref()
    }

    fn set_last_hovered_node(&mut self, node: Option<event_v2::HitTestNode>) {
        self.last_hovered_node = node;
    }

    fn get_document_id(&self) -> DocumentId {
        self.document_id
    }

    fn get_id_namespace(&self) -> IdNamespace {
        self.id_namespace
    }

    fn get_render_api(&self) -> &WrRenderApi {
        &self.render_api
    }

    fn get_render_api_mut(&mut self) -> &mut WrRenderApi {
        &mut self.render_api
    }

    fn get_renderer(&self) -> Option<&webrender::Renderer> {
        self.renderer.as_ref()
    }

    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer> {
        self.renderer.as_mut()
    }

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::MacOS(MacOSHandle {
            ns_window: &*self.window as *const NSWindow as *mut std::ffi::c_void,
            ns_view: std::ptr::null_mut(), // Not used in current implementation
        })
    }

    fn needs_frame_regeneration(&self) -> bool {
        self.frame_needs_regeneration
    }

    fn mark_frame_needs_regeneration(&mut self) {
        self.frame_needs_regeneration = true;
    }

    fn clear_frame_regeneration_flag(&mut self) {
        self.frame_needs_regeneration = false;
    }

    fn prepare_callback_invocation(&mut self) -> event_v2::InvokeSingleCallbackBorrows {
        let layout_window = self
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event_v2::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::MacOS(MacOSHandle {
                ns_window: &*self.window as *const NSWindow as *mut std::ffi::c_void,
                ns_view: std::ptr::null_mut(),
            }),
            gl_context_ptr: &self.gl_context_ptr,
            image_cache: &mut self.image_cache,
            fc_cache_clone: (*self.fc_cache).clone(),
            system_style: self.system_style.clone(),
            previous_window_state: &self.previous_window_state,
            current_window_state: &self.current_window_state,
            renderer_resources: &mut self.renderer_resources,
        }
    }

    // Timer Management (macOS/NSTimer Implementation)

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        use block2::RcBlock;

        let interval: f64 = timer.tick_millis() as f64 / 1000.0;

        // Store the timer in layout_window first
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }

        // Create NSTimer that marks frame for regeneration when fired
        let ns_window = self.window.clone();
        let timer_obj: Retained<NSTimer> = unsafe {
            msg_send_id![
                NSTimer::class(),
                scheduledTimerWithTimeInterval: interval,
                repeats: true,
                block: &*RcBlock::new(move || {
                    // Timer fired - request redraw to process timer callbacks
                    let _: () = msg_send![&*ns_window, setViewsNeedDisplay: true];
                })
            ]
        };

        self.timers.insert(timer_id, timer_obj);
    }

    fn stop_timer(&mut self, timer_id: usize) {
        // Invalidate NSTimer
        if let Some(timer) = self.timers.remove(&timer_id) {
            unsafe {
                timer.invalidate();
            }
        }

        // Remove from layout_window
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    // Thread Management (macOS/NSTimer Implementation)

    fn start_thread_poll_timer(&mut self) {
        use block2::RcBlock;

        if self.thread_timer_running.is_some() {
            return; // Already running
        }

        // Create a timer that fires every 16ms (~60 FPS) to poll threads
        let ns_window = self.window.clone();
        let timer: Retained<NSTimer> = unsafe {
            let interval: f64 = 0.016; // 16ms
            msg_send_id![
                NSTimer::class(),
                scheduledTimerWithTimeInterval: interval,
                repeats: true,
                block: &*RcBlock::new(move || {
                    // Thread polling - request redraw to check threads
                    let _: () = msg_send![&*ns_window, setViewsNeedDisplay: true];
                })
            ]
        };

        self.thread_timer_running = Some(timer);
    }

    fn stop_thread_poll_timer(&mut self) {
        if let Some(timer) = self.thread_timer_running.take() {
            unsafe {
                timer.invalidate();
            }
        }
    }

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            for (thread_id, thread) in threads {
                layout_window.threads.insert(thread_id, thread);
            }
        }
    }

    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            for thread_id in thread_ids {
                layout_window.threads.remove(thread_id);
            }
        }
    }

    // REQUIRED: Menu Display

    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Check if native menus are enabled
        if self.current_window_state.flags.use_native_context_menus {
            // Show native NSMenu
            self.show_native_menu_at_position(menu, position);
        } else {
            // Show fallback DOM-based menu
            // Make show_window_based_context_menu public or inline its logic
            self.show_fallback_menu(menu, position);
        }
    }

    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    ) {
        if let Err(e) = self.show_tooltip(text, position) {
            eprintln!("[macOS] Failed to show tooltip: {}", e);
        }
    }

    fn hide_tooltip_from_callback(&mut self) {
        if let Err(e) = self.hide_tooltip() {
            eprintln!("[macOS] Failed to hide tooltip: {}", e);
        }
    }
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

    /// Configure VSync on an OpenGL context
    ///
    /// NOTE: NSOpenGLContext setValues:forParameter: is deprecated on macOS 10.14+
    /// CVDisplayLink is the preferred approach for frame synchronization.
    /// This function is kept as a fallback but currently disabled due to
    /// type encoding issues with objc2's msg_send! macro.
    fn configure_vsync(_gl_context: &NSOpenGLContext, vsync: azul_core::window::Vsync) {
        use azul_core::window::Vsync;

        // TODO: Re-enable once objc2-open-gl feature is properly configured
        // The issue is that msg_send! expects specific type encodings:
        // - vals: *const GLint (i32)
        // - param: NSOpenGLContextParameter (wraps NSInteger = isize)
        // Using raw msg_send! with incorrect types causes runtime panics.
        //
        // For now, we rely on CVDisplayLink for vsync (see initialize_display_link)

        let swap_interval = match vsync {
            Vsync::Enabled => 1,
            Vsync::Disabled => 0,
            Vsync::DontCare => 1,
        };

        eprintln!(
            "[MacOSWindow::configure_vsync] VSync {} requested (swap interval: {}), using \
             CVDisplayLink instead",
            if swap_interval == 1 {
                "enabled"
            } else {
                "disabled"
            },
            swap_interval
        );
    }

    /// Detect the current monitor the window is on and update monitor_id
    ///
    /// This uses NSScreen's deviceDescription to extract CGDirectDisplayID,
    /// then computes a stable hash for the MonitorId.
    fn detect_current_monitor(&mut self) {
        use azul_core::window::MonitorId;

        // Get the screen the window is currently on
        let screen = unsafe { self.window.screen() };

        if let Some(screen) = screen {
            // Try to get CGDirectDisplayID from screen
            if let Some(display_id) = coregraphics::get_display_id_from_screen(&screen) {
                self.current_display_id = Some(display_id);

                // Get display bounds for hash computation
                let bounds = unsafe { screen.frame() };

                // Compute stable hash
                let hash = coregraphics::compute_monitor_hash(display_id, bounds);

                // For now, use display_id as index (not perfect but reasonable)
                // In a full implementation, we would enumerate all displays and assign indices
                let monitor_id = MonitorId {
                    index: display_id as usize,
                    hash,
                };

                self.current_window_state.monitor_id = OptionU32::Some(monitor_id.index as u32);

                eprintln!(
                    "[MacOSWindow] Monitor detected: display_id={}, index={}, hash={:x}",
                    display_id, monitor_id.index, hash
                );
            } else {
                eprintln!("[MacOSWindow] Failed to get CGDirectDisplayID from screen");
                // Fallback: Use index 0 (main display)
                self.current_window_state.monitor_id = OptionU32::Some(0);
            }
        } else {
            eprintln!("[MacOSWindow] No screen associated with window");
            // Fallback: Use index 0 (main display)
            self.current_window_state.monitor_id = OptionU32::Some(0);
        }
    }

    /// Initialize CVDisplayLink for VSYNC synchronization
    ///
    /// This is called during window creation if VSYNC is enabled.
    /// CVDisplayLink provides smooth frame pacing synchronized to the display refresh rate.
    fn initialize_display_link(&mut self) -> Result<(), String> {
        use azul_core::window::Vsync;

        // Check if VSYNC is enabled
        let vsync = self.current_window_state.renderer_options.vsync;
        if vsync == Vsync::Disabled {
            eprintln!("[CVDisplayLink] VSYNC disabled, skipping CVDisplayLink");
            return Ok(());
        }

        // Check if CoreVideo functions are available
        let cv_functions = match &self.cv_functions {
            Some(funcs) => funcs.clone(),
            None => {
                eprintln!("[CVDisplayLink] CoreVideo not available, using fallback VSync");
                // Try traditional VSync as fallback
                if let Some(ref gl_context) = self.gl_context {
                    Self::configure_vsync(gl_context, vsync);
                }
                return Ok(());
            }
        };

        // Get the display ID for this window
        let display_id = self.current_display_id.unwrap_or_else(|| {
            // Fallback to main display
            if let Some(ref cg_funcs) = self.cg_functions {
                cg_funcs.main_display_id()
            } else {
                0 // CG_MAIN_DISPLAY_ID constant
            }
        });

        eprintln!(
            "[CVDisplayLink] Creating display link for display {}",
            display_id
        );

        // Create CVDisplayLink for this display
        let display_link = corevideo::DisplayLink::new(display_id, cv_functions.clone())
            .map_err(|code| format!("CVDisplayLinkCreateWithCGDisplays failed: {}", code))?;

        // Set output callback
        // For now, we'll use a simple callback that just marks the window for redraw
        extern "C" fn display_link_callback(
            _display_link: corevideo::CVDisplayLinkRef,
            _in_now: *const corevideo::CVTimeStamp,
            _in_output_time: *const corevideo::CVTimeStamp,
            _flags_in: u64,
            _flags_out: *mut u64,
            display_link_context: *mut std::ffi::c_void,
        ) -> corevideo::CVReturn {
            // SAFETY: display_link_context is a pointer to NSWindow
            unsafe {
                if !display_link_context.is_null() {
                    let ns_window = display_link_context as *const NSWindow;
                    // Request display (setNeedsDisplay equivalent)
                    // This will trigger drawRect on the next runloop iteration
                    use objc2::msg_send;
                    let _: () = msg_send![ns_window, setViewsNeedDisplay: true];
                }
            }
            corevideo::K_CV_RETURN_SUCCESS
        }

        // Pass NSWindow pointer as context
        let window_ptr = &*self.window as *const NSWindow as *mut std::ffi::c_void;
        let result = display_link.set_output_callback(display_link_callback, window_ptr);

        if result != corevideo::K_CV_RETURN_SUCCESS {
            return Err(format!("CVDisplayLinkSetOutputCallback failed: {}", result));
        }

        // Start the display link
        let result = display_link.start();
        if result != corevideo::K_CV_RETURN_SUCCESS {
            return Err(format!("CVDisplayLinkStart failed: {}", result));
        }

        eprintln!("[CVDisplayLink] Display link started successfully");
        self.display_link = Some(display_link);

        Ok(())
    }

    /// Create a new macOS window with given options and shared font cache.
    pub fn new_with_fc_cache(
        options: WindowCreateOptions,
        app_data: RefAny,
        fc_cache: Arc<rust_fontconfig::FcFontCache>,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        Self::new_with_options_internal(options, app_data, Some(fc_cache), mtm)
    }

    /// Create a new macOS window with given options.
    pub fn new_with_options(
        options: WindowCreateOptions,
        app_data: RefAny,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        Self::new_with_options_internal(options, app_data, None, mtm)
    }

    /// Internal constructor with optional fc_cache parameter
    fn new_with_options_internal(
        options: WindowCreateOptions,
        app_data: RefAny,
        fc_cache_opt: Option<Arc<rust_fontconfig::FcFontCache>>,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        eprintln!("[MacOSWindow::new] Starting window creation");

        // Initialize NSApplication if needed
        eprintln!("[MacOSWindow::new] Getting NSApplication...");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        eprintln!("[MacOSWindow::new] NSApplication configured");

        // Get screen dimensions for window positioning
        eprintln!("[MacOSWindow::new] Getting main screen...");
        let screen = NSScreen::mainScreen(mtm)
            .ok_or_else(|| WindowError::PlatformError("No main screen".into()))?;

        let screen_frame = screen.frame();
        eprintln!(
            "[MacOSWindow::new] Screen frame: {}x{}",
            screen_frame.size.width, screen_frame.size.height
        );

        // Determine window size from options
        let window_size = options.window_state.size.dimensions;
        let width = window_size.width as f64;
        let height = window_size.height as f64;

        // Center window on screen
        let x = (screen_frame.size.width - width) / 2.0;
        let y = (screen_frame.size.height - height) / 2.0;

        let content_rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));

        // Determine rendering backend
        eprintln!("[MacOSWindow::new] Determining rendering backend...");
        let requested_backend = Self::determine_backend(&options);
        eprintln!("[MacOSWindow::new] Backend: {:?}", requested_backend);

        // Create content view based on backend
        eprintln!("[MacOSWindow::new] Creating content view...");
        let (backend, gl_view, gl_context, gl_functions, cpu_view) = match requested_backend {
            RenderBackend::OpenGL => match Self::create_gl_view(content_rect, mtm) {
                Ok((view, ctx, funcs)) => {
                    eprintln!("[MacOSWindow::new] OpenGL view created successfully");
                    eprintln!("[MacOSWindow::new] Configuring VSync...");
                    let vsync = options.window_state.renderer_options.vsync;
                    Self::configure_vsync(&ctx, vsync);
                    eprintln!("[MacOSWindow::new] VSync configured, returning from match...");
                    (
                        RenderBackend::OpenGL,
                        Some(view),
                        Some(ctx),
                        Some(funcs),
                        None,
                    )
                }
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
        eprintln!(
            "[MacOSWindow::new] Content view created, backend: {:?}",
            backend
        );

        // Create window style mask
        eprintln!("[MacOSWindow::new] Creating window with style mask...");
        let style_mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        // Create the window
        eprintln!("[MacOSWindow::new] Allocating NSWindow...");
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                content_rect,
                style_mask,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        eprintln!("[MacOSWindow::new] NSWindow created");

        // Set window title
        eprintln!("[MacOSWindow::new] Setting window title...");
        let title = NSString::from_str(&options.window_state.title);
        window.setTitle(&title);
        eprintln!("[MacOSWindow::new] Window title set");

        // Set content view (either GL or CPU)
        // SAFE: Both GLView and CPUView inherit from NSView, so we can upcast safely
        eprintln!("[MacOSWindow::new] Setting content view...");
        if let Some(ref gl) = gl_view {
            eprintln!("[MacOSWindow::new] Setting GL view as content view...");
            unsafe {
                // GLView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(gl) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
            eprintln!("[MacOSWindow::new] GL view set");
        } else if let Some(ref cpu) = cpu_view {
            eprintln!("[MacOSWindow::new] Setting CPU view as content view...");
            unsafe {
                // CPUView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(cpu) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
        } else {
            return Err(WindowError::PlatformError("No content view created".into()));
        }
        eprintln!("[MacOSWindow::new] Content view configured");

        // DO NOT show the window yet - we will show it after the first frame
        // is ready to prevent white flash
        eprintln!("[MacOSWindow::new] Positioning window...");
        unsafe {
            // Simplified positioning: just center the window
            // Complex monitor enumeration can hang before event loop starts
            window.center();

            // TODO: Implement proper multi-monitor positioning after event loop starts
            // For now, user can move window manually or we can position it later

            // REMOVED: makeKeyAndOrderFront - will be called after first frame is ready
        }
        eprintln!("[MacOSWindow::new] Window centered");

        // Apply initial window state based on options.window_state.flags.frame
        // Note: These will be applied before window is visible
        eprintln!("[MacOSWindow::new] Applying window frame state...");
        unsafe {
            match options.window_state.flags.frame {
                WindowFrame::Fullscreen => {
                    window.toggleFullScreen(None);
                }
                WindowFrame::Maximized => {
                    window.performZoom(None);
                }
                WindowFrame::Minimized => {
                    window.miniaturize(None);
                }
                WindowFrame::Normal => {
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

        // Make OpenGL context current before initializing WebRender
        if let Some(ref ctx) = gl_context {
            unsafe {
                ctx.makeCurrentContext();
            }
        }

        // Initialize WebRender renderer
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
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));

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
            width: (options.window_state.size.dimensions.width * actual_hidpi_factor) as u32,
            height: (options.window_state.size.dimensions.height * actual_hidpi_factor) as u32,
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
        let gl_context_ptr: OptionGlContextPtr = gl_context
            .as_ref()
            .map(|_| GlContextPtr::new(renderer_type, gl_funcs.clone()))
            .into();

        // Initialize window state with actual HiDPI factor from screen
        let actual_dpi = (actual_hidpi_factor * 96.0) as u32; // Convert scale factor to DPI
        let mut current_window_state = FullWindowState {
            title: options.window_state.title.clone(),
            size: WindowSize {
                dimensions: options.window_state.size.dimensions,
                dpi: actual_dpi, // Use actual DPI from screen
                min_dimensions: options.window_state.size.min_dimensions,
                max_dimensions: options.window_state.size.max_dimensions,
            },
            position: options.window_state.position,
            flags: options.window_state.flags,
            theme: options.window_state.theme,
            debug_state: options.window_state.debug_state,
            keyboard_state: Default::default(),
            mouse_state: Default::default(),
            touch_state: Default::default(),
            ime_position: options.window_state.ime_position,
            platform_specific_options: options.window_state.platform_specific_options.clone(),
            renderer_options: options.window_state.renderer_options,
            background_color: options.window_state.background_color,
            layout_callback: options.window_state.layout_callback,
            close_callback: options.window_state.close_callback.clone(),
            monitor_id: OptionU32::None, // Monitor ID will be set when we detect the actual monitor
            window_focused: true,
        };

        // Initialize resource caches
        let image_cache = ImageCache::default();
        let renderer_resources = RendererResources::default();

        // Initialize LayoutWindow with shared fc_cache or build a new one
        let fc_cache =
            fc_cache_opt.unwrap_or_else(|| Arc::new(rust_fontconfig::FcFontCache::build()));
        let mut layout_window = LayoutWindow::new((*fc_cache).clone()).map_err(|e| {
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

        // Initialize shared application data from the provided app_data
        let app_data_arc = Arc::new(RefCell::new(app_data));

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

        // Load CoreVideo and Core Graphics functions for VSYNC and monitor detection
        let cv_functions = match CoreVideoFunctions::load() {
            Ok(funcs) => {
                eprintln!("[Window Init] CoreVideo loaded successfully");
                Some(funcs)
            }
            Err(e) => {
                eprintln!(
                    "[Window Init] CoreVideo not available: {} - VSYNC will use fallback",
                    e
                );
                None
            }
        };

        let cg_functions = match CoreGraphicsFunctions::load() {
            Ok(funcs) => {
                eprintln!("[Window Init] Core Graphics loaded successfully");
                Some(funcs)
            }
            Err(e) => {
                eprintln!(
                    "[Window Init] Core Graphics not available: {} - monitor detection will use \
                     fallback",
                    e
                );
                None
            }
        };

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
            menu_state: menu::MenuState::new(), // TODO: build initial menu state from layout_window
            image_cache,
            renderer_resources,
            render_api,
            renderer: Some(renderer),
            hit_tester: AsyncHitTester::Resolved(hit_tester),
            document_id,
            id_namespace,
            gl_context_ptr,
            app_data: app_data_arc,
            fc_cache,
            system_style: Arc::new(azul_css::system::SystemStyle::new()),
            frame_needs_regeneration: false,
            scrollbar_drag_state: None,
            new_frame_ready,
            #[cfg(feature = "a11y")]
            accessibility_adapter: None, // Will be initialized after first layout
            pending_window_creates: Vec::new(),
            tooltip: None,         // Created lazily when first needed
            pm_assertion_id: None, // No sleep prevention by default
            timers: std::collections::HashMap::new(),
            thread_timer_running: None,
            display_link: None, // Will be initialized when VSYNC is enabled
            cv_functions,
            cg_functions,
            current_display_id: None, // Will be set after monitor detection
        };

        // NOTE: Do NOT set the delegate pointer here!
        // The window will be moved out of this function (returned by value),
        // so any pointer we set here will become invalid.
        // Instead, call finalize_delegate_pointer() AFTER the window is in its final location.

        // Set up WebRender document with root pipeline and viewport
        // This only needs to be done once at initialization

        // NOTE: Don't send any transaction during initialization!
        // The first transaction will be sent in drawRect
        // when drawRect is called by macOS.

        // Perform initial layout
        eprintln!("[Window Init] Performing initial layout");
        if let Err(e) = window.regenerate_layout() {
            eprintln!("[Window Init] WARNING: Initial layout failed: {}", e);
        }

        // Initialize accessibility adapter after first layout
        #[cfg(feature = "a11y")]
        {
            eprintln!("[Window Init] Initializing accessibility support");
            window.init_accessibility();
        }

        // Set frame_needs_regeneration to true so drawRect will build and send transaction
        window.frame_needs_regeneration = true;

        // Detect current monitor and set monitor_id
        window.detect_current_monitor();

        // Initialize CVDisplayLink for VSYNC (if enabled and available)
        if let Err(e) = window.initialize_display_link() {
            eprintln!("[Window Init] CVDisplayLink initialization failed: {}", e);
            // Not a fatal error - window will still work, just without VSYNC
        }

        // Show window immediately - drawRect will handle the first frame rendering
        eprintln!(
            "[Window Init] Making window visible (first frame will be rendered in drawRect)..."
        );
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
        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Call unified regenerate_layout from common module
        crate::desktop::shell2::common::layout_v2::regenerate_layout(
            layout_window,
            &self.app_data,
            &self.current_window_state,
            &mut self.renderer_resources,
            &mut self.render_api,
            &self.image_cache,
            &self.gl_context_ptr,
            &self.fc_cache,
            &self.system_style,
            self.document_id,
        )?;

        // Mark that frame needs regeneration (will be called once at event processing end)
        self.frame_needs_regeneration = true;

        // Update accessibility tree after layout
        #[cfg(feature = "a11y")]
        self.update_accessibility();

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.update_ime_position_from_cursor();
        self.sync_ime_position_to_os();

        Ok(())
    }

    /// Update ime_position in window state from focused text cursor
    /// Called after layout to ensure IME window appears at correct position
    fn update_ime_position_from_cursor(&mut self) {
        use azul_core::window::ImePosition;

        if let Some(layout_window) = &self.layout_window {
            if let Some(cursor_rect) = layout_window.get_focused_cursor_rect_viewport() {
                // Successfully calculated cursor position from text layout
                self.current_window_state.ime_position = ImePosition::Initialized(cursor_rect);
            }
        }
    }

    /// Generate frame if needed and reset flag
    pub fn generate_frame_if_needed(&mut self) {
        if !self.frame_needs_regeneration {
            return;
        }

        if let Some(ref mut layout_window) = self.layout_window {
            crate::desktop::shell2::common::layout_v2::generate_frame(
                layout_window,
                &mut self.render_api,
                self.document_id,
            );
        }

        self.frame_needs_regeneration = false;
    }

    /// Get the current HiDPI scale factor from the NSWindow's screen
    ///
    /// This queries the actual backing scale factor from the screen,
    /// which can change when the window moves between displays.
    pub fn get_hidpi_factor(&self) -> DpiScaleFactor {
        use azul_css::props::basic::FloatValue;
        DpiScaleFactor {
            inner: FloatValue::new(unsafe {
                self.window
                    .screen()
                    .map(|screen| screen.backingScaleFactor() as f32)
                    .unwrap_or(1.0)
            }),
        }
    }

    /// Get the raw window handle for this window
    pub fn get_raw_window_handle(&self) -> RawWindowHandle {
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
    /// indicating the window moved to a display with different DPI or monitor.
    pub fn handle_dpi_change(&mut self) -> Result<(), String> {
        let new_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.current_window_state.size.get_hidpi_factor();

        // Check if monitor changed (detect current monitor)
        let old_display_id = self.current_display_id;
        self.detect_current_monitor();
        let new_display_id = self.current_display_id;

        // If display changed, we may need to recreate CVDisplayLink
        if old_display_id != new_display_id {
            eprintln!(
                "[DPI Change] Display changed: {:?} -> {:?}",
                old_display_id, new_display_id
            );

            // Stop old display link
            if let Some(old_link) = self.display_link.take() {
                if old_link.is_running() {
                    old_link.stop();
                }
                // DisplayLink will be dropped here
            }

            // Recreate display link for new display
            if let Err(e) = self.initialize_display_link() {
                eprintln!("[DPI Change] Failed to recreate CVDisplayLink: {}", e);
                // Not fatal - continue without display link
            }
        }

        // Only process if DPI actually changed
        if (new_hidpi.inner.get() - old_hidpi.inner.get()).abs() < 0.001 {
            return Ok(());
        }

        eprintln!(
            "[DPI Change] {} -> {}",
            old_hidpi.inner.get(),
            new_hidpi.inner.get()
        );

        // Update window state with new DPI
        self.current_window_state.size.dpi = (new_hidpi.inner.get() * 96.0) as u32;

        // Regenerate layout with new DPI
        self.regenerate_layout()?;

        Ok(())
    }

    /// Perform GPU scrolling - updates scroll transforms without full relayout
    pub fn gpu_scroll(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use std::time::Duration;

        use azul_core::{
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::managers::scroll_state::ScrollEvent;

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // 1. Create scroll event and process it
        let scroll_event = ScrollEvent {
            dom_id,
            node_id,
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll using scroll_by instead of apply_scroll_event
        layout_window.scroll_manager.scroll_by(
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
        layout_window.scroll_manager.calculate_scrollbar_states();

        // 3. Update WebRender scroll layers and GPU transforms
        let mut txn = crate::desktop::wr_translate2::WrTransaction::new();

        // Scroll all nodes in the scroll manager to WebRender
        // This updates external scroll IDs with new offsets
        crate::desktop::wr_translate2::scroll_all_nodes(layout_window, &mut txn);

        // Synchronize GPU-animated values (transforms, opacities, scrollbar positions)
        // Note: We need mutable access for gpu_state_manager updates
        crate::desktop::wr_translate2::synchronize_gpu_values(layout_window, &mut txn);

        // Generate frame (without rebuilding display list)
        crate::desktop::wr_translate2::generate_frame(
            &mut txn,
            layout_window,
            &mut self.render_api,
            false, // Display list not rebuilt, just transforms updated
        );

        // Send transaction
        self.render_api.send_transaction(
            crate::desktop::wr_translate2::wr_translate_document_id(self.document_id),
            txn,
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

        // is_top_level flag changed?
        if previous.flags.is_top_level != current.flags.is_top_level {
            if let Err(e) = self.set_is_top_level(current.flags.is_top_level) {
                eprintln!("[macOS] Failed to set is_top_level: {}", e);
            }
        }

        // prevent_system_sleep flag changed?
        if previous.flags.prevent_system_sleep != current.flags.prevent_system_sleep {
            if let Err(e) = self.set_prevent_system_sleep(current.flags.prevent_system_sleep) {
                eprintln!("[macOS] Failed to set prevent_system_sleep: {}", e);
            }
        }

        // Mouse cursor synchronization - compute from current hit test
        use azul_layout::managers::hover::InputPointId;
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                let cursor_name = self.map_cursor_type_to_macos(cursor_test.cursor_icon);
                self.set_cursor(cursor_name);
            }
        }
    }

    /// Map MouseCursorType to macOS cursor name
    fn map_cursor_type_to_macos(&self, cursor_type: MouseCursorType) -> &'static str {
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
    pub fn update_window_state(&mut self, new_state: FullWindowState) {
        // Save current state as previous for next frame's diff
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update current state with the new full state
        self.current_window_state = new_state;

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
        let result = self.process_window_events_recursive_v2(0);

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
        let result = self.process_window_events_recursive_v2(0);

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
    /// Start the thread polling timer (16ms interval for ~60 FPS)
    pub fn start_thread_tick_timer(&mut self) {
        use block2::RcBlock;
        if self.thread_timer_running.is_none() {
            // Create a timer that fires every 16ms (60 FPS)
            // Using scheduledTimerWithTimeInterval for simplicity
            let timer: Retained<NSTimer> = unsafe {
                let interval: f64 = 0.016; // 16ms
                msg_send_id![
                    NSTimer::class(),
                    scheduledTimerWithTimeInterval: interval,
                    repeats: true,
                    block: &*RcBlock::new(|| {
                        // Thread tick callback - poll thread messages
                        // This will be called every 16ms
                    })
                ]
            };

            self.thread_timer_running = Some(timer);
        }
    }

    /// Stop the thread polling timer
    pub fn stop_thread_tick_timer(&mut self) {
        if let Some(timer) = self.thread_timer_running.take() {
            unsafe {
                timer.invalidate();
            }
        }
    }

    fn close_window(&mut self) {
        // Unregister from global window registry before closing
        let ns_window = self.get_ns_window_ptr();
        registry::unregister_window(ns_window);

        unsafe {
            self.window.close();
        }
        self.is_open = false;
    }

    /// Check if the window is still open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Get the NSWindow pointer for registry identification
    ///
    /// Returns a raw pointer to the NSWindow object, which is used as a unique
    /// identifier in the window registry for multi-window support.
    pub fn get_ns_window_ptr(&self) -> *mut objc2::runtime::AnyObject {
        Retained::as_ptr(&self.window) as *mut objc2::runtime::AnyObject
    }

    /// Apply window decorations changes
    fn apply_decorations(&mut self, decorations: WindowDecorations) {
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
    fn apply_background_material(&mut self, material: WindowBackgroundMaterial) {
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

        // Look up callback from tag
        let callback = match self.menu_state.get_callback_for_tag(tag as i64) {
            Some(cb) => cb.clone(),
            None => {
                eprintln!("[MacOSWindow] No callback found for tag: {}", tag);
                return;
            }
        };

        eprintln!("[MacOSWindow] Menu item clicked (tag {})", tag);

        // Convert CoreMenuCallback to layout MenuCallback
        use azul_layout::callbacks::{Callback, MenuCallback};

        let layout_callback = Callback::from_core(callback.callback);
        let mut menu_callback = MenuCallback {
            callback: layout_callback,
            refany: callback.refany,
        };

        // Get layout window to create callback info
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => {
                eprintln!("[MacOSWindow] No layout window available");
                return;
            }
        };

        use std::ptr;

        use azul_core::window::RawWindowHandle;

        let raw_handle = RawWindowHandle::MacOS(azul_core::window::MacOSHandle {
            ns_window: Retained::as_ptr(&self.window) as *mut _,
            ns_view: ptr::null_mut(), // Not needed for menu callbacks
        });

        // Clone fc_cache (cheap Arc clone) since invoke_single_callback needs &mut
        let mut fc_cache_clone = (*self.fc_cache).clone();

        // Use LayoutWindow::invoke_single_callback which handles all the borrow complexity
        let callback_result = layout_window.invoke_single_callback(
            &mut menu_callback.callback,
            &mut menu_callback.refany,
            &raw_handle,
            &self.gl_context_ptr,
            &mut self.image_cache,
            &mut fc_cache_clone,
            self.system_style.clone(),
            &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
            &self.previous_window_state,
            &self.current_window_state,
            &self.renderer_resources,
        );

        // Process callback result using the V2 unified system
        // This handles timers, threads, window state changes, and Update
        use crate::desktop::shell2::common::event_v2::PlatformWindowV2;
        let event_result = self.process_callback_result_v2(&callback_result);

        // Handle the event result
        use azul_core::events::ProcessEventResult;
        match event_result {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::ShouldReRenderCurrentWindow
            | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                self.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {
                // No action needed
            }
        }
    }

    /// Check if window is maximized by comparing frame to screen size
    ///
    /// Updates the window frame state based on the actual window and screen dimensions.
    /// Should be called after resize events.
    fn check_maximized_state(&mut self) {
        // Skip check if in fullscreen mode
        if self.current_window_state.flags.frame == WindowFrame::Fullscreen {
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
            WindowFrame::Maximized
        } else {
            WindowFrame::Normal
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

    /// Show a tooltip with the given text at the specified position
    ///
    /// Position is in logical coordinates. The tooltip will be created on first use.
    pub fn show_tooltip(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    ) -> Result<(), String> {
        // Lazily create tooltip if needed
        if self.tooltip.is_none() {
            self.tooltip = Some(tooltip::TooltipWindow::new(self.mtm)?);
        }

        let dpi_factor = DpiScaleFactor::new(unsafe {
            self.window
                .screen()
                .map(|screen| screen.backingScaleFactor() as f32)
                .unwrap_or(1.0)
        });

        if let Some(ref mut tooltip) = self.tooltip {
            tooltip.show(text, position, dpi_factor)?;
        }

        Ok(())
    }

    /// Hide the currently displayed tooltip
    ///
    /// Does nothing if no tooltip is shown.
    pub fn hide_tooltip(&mut self) -> Result<(), String> {
        if let Some(ref mut tooltip) = self.tooltip {
            tooltip.hide()?;
        }
        Ok(())
    }

    /// Set the window to be always on top (or not)
    ///
    /// Uses setLevel with NSFloatingWindowLevel or NSNormalWindowLevel.
    pub fn set_is_top_level(&mut self, is_top_level: bool) -> Result<(), String> {
        unsafe {
            if is_top_level {
                self.window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
            } else {
                self.window.setLevel(objc2_app_kit::NSNormalWindowLevel);
            }
        }
        Ok(())
    }

    /// Prevent the system from sleeping (or allow it to sleep)
    ///
    /// Uses IOPMAssertionCreateWithName to create a power assertion.
    pub fn set_prevent_system_sleep(&mut self, prevent: bool) -> Result<(), String> {
        unsafe {
            if prevent {
                // Already have an assertion?
                if self.pm_assertion_id.is_some() {
                    return Ok(());
                }

                // Create assertion
                let assertion_type = NSString::from_str(kIOPMAssertionTypeNoDisplaySleep);
                let assertion_name = NSString::from_str("Azul GUI - Video Playback");
                let mut assertion_id: IOPMAssertionID = 0;

                let result = IOPMAssertionCreateWithName(
                    assertion_type.as_ref(),
                    kIOPMAssertionLevelOn,
                    assertion_name.as_ref(),
                    &mut assertion_id,
                );

                if result == kIOReturnSuccess {
                    self.pm_assertion_id = Some(assertion_id);
                    eprintln!(
                        "[macOS] System sleep prevented (assertion: {})",
                        assertion_id
                    );
                    Ok(())
                } else {
                    Err(format!("IOPMAssertionCreateWithName failed: {}", result))
                }
            } else {
                // Release assertion
                if let Some(assertion_id) = self.pm_assertion_id.take() {
                    let result = IOPMAssertionRelease(assertion_id);
                    if result == kIOReturnSuccess {
                        eprintln!("[macOS] System sleep allowed (assertion: {})", assertion_id);
                        Ok(())
                    } else {
                        Err(format!("IOPMAssertionRelease failed: {}", result))
                    }
                } else {
                    Ok(()) // No assertion to release
                }
            }
        }
    }

    /// Show a native NSMenu at the given position (without NSEvent)
    ///
    /// This is used for menus opened from callbacks (info.open_menu()).
    /// Unlike context menus which need the NSEvent for proper positioning,
    /// this version shows the menu at an absolute position.
    fn show_native_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        use objc2_app_kit::{NSMenu, NSMenuItem};
        use objc2_foundation::{MainThreadMarker, NSPoint, NSString};

        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => {
                eprintln!("[Menu] Not on main thread, cannot show menu");
                return;
            }
        };

        let ns_menu = NSMenu::new(mtm);

        // Build menu items recursively from Azul menu structure
        // Call the public(crate) associated function
        MacOSWindow::recursive_build_nsmenu(
            &ns_menu,
            menu.items.as_slice(),
            &mtm,
            &mut self.menu_state,
        );

        // Show the menu at the specified position
        let view_point = NSPoint {
            x: position.x as f64,
            y: position.y as f64,
        };

        let view = if let Some(ref gl_view) = self.gl_view {
            Some(&**gl_view as &objc2::runtime::AnyObject)
        } else if let Some(ref cpu_view) = self.cpu_view {
            Some(&**cpu_view as &objc2::runtime::AnyObject)
        } else {
            None
        };

        if let Some(view) = view {
            eprintln!(
                "[Menu] Showing native menu at position ({}, {}) with {} items",
                position.x,
                position.y,
                menu.items.as_slice().len()
            );

            unsafe {
                use objc2::{msg_send_id, rc::Retained, runtime::AnyObject, sel};

                let _: () = msg_send_id![
                    &ns_menu,
                    popUpMenuPositioningItem: Option::<&AnyObject>::None,
                    atLocation: view_point,
                    inView: view
                ];
            }
        }
    }

    /// Show a fallback window-based menu at the given position
    ///
    /// This uses the same unified menu system as regular menus but for callback-triggered menus.
    fn show_fallback_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for callback menus
            Some(position), // Position for menu
            None,           // No parent menu
        );

        // Queue window creation request
        eprintln!(
            "[macOS] Queuing fallback menu window at screen ({}, {}) - will be created in event \
             loop",
            position.x, position.y
        );

        self.pending_window_creates.push(menu_options);
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

        // After processing event, just request a redraw if needed
        // The atomic transaction will be built in drawRect
        if self.frame_needs_regeneration {
            eprintln!("[handle_event] Frame needs regeneration, requesting redraw");
            self.request_redraw();
            self.frame_needs_regeneration = false;
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

    // RENDERING METHODS - macOS Drawing Model Integration

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
                eprintln!(
                    "[MACOS NATIVE] >>>>> Calling NSOpenGLContext.makeCurrentContext() <<<<<"
                );
                // Make context current before any GL operations
                gl_context.makeCurrentContext();
                eprintln!("[MACOS NATIVE] >>>>> makeCurrentContext() RETURNED <<<<<");

                eprintln!("[MACOS NATIVE] >>>>> Calling NSOpenGLContext.update() <<<<<");
                // CRITICAL: Synchronize context with the view's drawable surface
                // This must be called every frame to handle window moves/resizes
                gl_context.update(self.mtm);
                eprintln!("[MACOS NATIVE] >>>>> update() RETURNED <<<<<");

                // CRITICAL: Set the viewport to the physical size of the window
                let physical_size = self.current_window_state.size.get_physical_size();
                eprintln!(
                    "[render_and_present_in_draw_rect] Setting glViewport to: {}x{}",
                    physical_size.width, physical_size.height
                );
                eprintln!(
                    "[MACOS NATIVE] >>>>> Calling glViewport({}, {}, {}, {}) <<<<<",
                    0, 0, physical_size.width, physical_size.height
                );
                gl_fns.functions.viewport(
                    0,
                    0,
                    physical_size.width as i32,
                    physical_size.height as i32,
                );
                eprintln!("[MACOS NATIVE] >>>>> glViewport() RETURNED <<<<<");
            }
        }

        // Step 1.5: CRITICAL - Create, build, and send WebRender transaction ATOMICALLY
        // This is the ONLY place where Transaction::new() should be called!
        // This matches the working WebRender example pattern: ONE transaction per frame
        eprintln!(
            "[render_and_present_in_draw_rect] >>>>> Creating ONE atomic WebRender transaction \
             <<<<<"
        );

        let mut txn = WrTransaction::new();

        // Build everything into this transaction (resources, display lists, etc.)

        eprintln!("[build_atomic_txn] START ");

        // CRITICAL: Regenerate layout FIRST if needed
        // Layout must be current before building display lists
        if self.frame_needs_regeneration {
            eprintln!("[build_atomic_txn] Frame needs regeneration, calling regenerate_layout");
            if let Err(e) = self.regenerate_layout() {
                eprintln!("[build_atomic_txn] Layout failed: {}", e);
                return Err(WindowError::PlatformError(
                    format!("Layout failed: {}", e).into(),
                ));
            }
            self.frame_needs_regeneration = false;
        }

        // Get layout_window
        let layout_window = self
            .layout_window
            .as_mut()
            .ok_or_else(|| WindowError::PlatformError("No layout window".into()))?;

        eprintln!("[build_atomic_txn] Building into provided transaction");

        eprintln!("[build_atomic_txn] Calling build_webrender_transaction()");
        // Build everything into this transaction using helper functions
        crate::desktop::wr_translate2::build_webrender_transaction(
            &mut txn,
            layout_window,
            &mut self.render_api,
            &self.image_cache,
        )
        .map_err(|e| {
            WindowError::PlatformError(format!("Failed to build transaction: {}", e).into())
        })?;

        eprintln!("[build_atomic_txn] BUILD COMPLETE ");

        // Send the complete atomic transaction
        if let Some(layout_window) = self.layout_window.as_ref() {
            let doc_id = wr_translate_document_id(layout_window.document_id);
            eprintln!(
                "[render_and_present_in_draw_rect] >>>>> Calling \
                 render_api.send_transaction({:?}, txn) <<<<<",
                doc_id
            );
            self.render_api.send_transaction(doc_id, txn);
            eprintln!(
                "[render_and_present_in_draw_rect] >>>>> Calling render_api.flush_scene_builder() \
                 <<<<<"
            );
            self.render_api.flush_scene_builder();
            eprintln!(
                "[render_and_present_in_draw_rect] >>>>> flush_scene_builder() RETURNED <<<<<"
            );
        }
        eprintln!("[render_and_present_in_draw_rect] >>>>> Transaction sent and flushed <<<<<");

        // Step 2: Call WebRender to composite the scene
        if let Some(ref mut renderer) = self.renderer {
            eprintln!("[WEBRENDER] >>>>> Calling renderer.update() <<<<<");
            renderer.update();
            eprintln!("[WEBRENDER] >>>>> renderer.update() RETURNED <<<<<");

            let physical_size = self.current_window_state.size.get_physical_size();
            let device_size = webrender::api::units::DeviceIntSize::new(
                physical_size.width as i32,
                physical_size.height as i32,
            );

            eprintln!(
                "[WEBRENDER] >>>>> Calling renderer.render({:?}, 0) <<<<<",
                device_size
            );

            match renderer.render(device_size, 0) {
                Ok(results) => {
                    eprintln!("[WEBRENDER] >>>>> renderer.render() RETURNED <<<<<");
                    eprintln!(
                        "[WEBRENDER] ✓ Render successful! Stats: {:?}",
                        results.stats
                    );
                }
                Err(errors) => {
                    eprintln!("[WEBRENDER] >>>>> renderer.render() RETURNED WITH ERROR <<<<<");
                    eprintln!("[WEBRENDER] ✗ Render errors: {:?}", errors);
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
                    eprintln!(
                        "[MACOS NATIVE] >>>>> Calling NSOpenGLContext.flushBuffer() (SWAP \
                         BUFFERS) <<<<<"
                    );
                    unsafe {
                        gl_context.flushBuffer();
                    }
                    eprintln!(
                        "[MACOS NATIVE] >>>>> flushBuffer() RETURNED - SCREEN NOW UPDATED <<<<<"
                    );
                }
            }
            RenderBackend::CPU => {
                // CPU backend doesn't need explicit buffer swap
                // The drawRect: itself updates the view
            }
        }

        eprintln!("[render_and_present_in_draw_rect] FRAME COMPLETE \n");
        Ok(())
    }
}

impl Drop for MacOSWindow {
    fn drop(&mut self) {
        eprintln!("[MacOSWindow::drop] Cleaning up window resources");

        // Stop and release CVDisplayLink if active
        if let Some(ref display_link) = self.display_link {
            if display_link.is_running() {
                eprintln!("[MacOSWindow::drop] Stopping CVDisplayLink");
                display_link.stop();
            }
            // DisplayLink will be dropped automatically, calling release
        }

        // Release power management assertion if active
        if let Some(assertion_id) = self.pm_assertion_id.take() {
            eprintln!("[MacOSWindow::drop] Releasing IOPMAssertion");
            unsafe {
                IOPMAssertionRelease(assertion_id);
            }
        }

        // Invalidate all active timers
        for (_, timer) in self.timers.drain() {
            unsafe {
                timer.invalidate();
            }
        }

        // Invalidate thread poll timer
        if let Some(timer) = self.thread_timer_running.take() {
            unsafe {
                timer.invalidate();
            }
        }

        eprintln!("[MacOSWindow::drop] Window cleanup complete");
    }
}

impl PlatformWindow for MacOSWindow {
    type EventType = MacOSEvent;

    fn new(options: WindowCreateOptions, app_data: RefAny) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;

        Self::new_with_options(options, app_data, mtm)
    }

    fn get_state(&self) -> FullWindowState {
        // Return the stored current_window_state (which is kept in sync with OS)
        // instead of creating a new WindowState from scratch
        self.current_window_state.clone()
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

            // Atomic transaction will be built in drawRect if needed
            // Just request redraw here if layout changed
            if self.frame_needs_regeneration {
                self.request_redraw();
                self.frame_needs_regeneration = false;
            }

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

    fn present(&mut self) -> Result<(), WindowError> {
        // For macOS, presentation is handled by the compositor/NSOpenGLContext
        // The present() method is called by the rendering backend (WebRender)
        // or directly after CPU rendering
        match &self.backend {
            RenderBackend::OpenGL => {
                // For GPU rendering, flush the OpenGL context
                if let Some(ref gl_context) = self.gl_context {
                    unsafe {
                        let _: () = msg_send![gl_context, flushBuffer];
                    }
                }
            }
            RenderBackend::CPU => {
                // For CPU rendering, present is handled by drawRect:
                // Nothing to do here as the bitmap was already drawn
            }
        }
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn close(&mut self) {
        // Release power management assertion if active
        if let Some(assertion_id) = self.pm_assertion_id.take() {
            unsafe {
                IOPMAssertionRelease(assertion_id);
            }
            eprintln!("[macOS] Released power assertion on window close");
        }

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

    fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        clipboard::sync_clipboard(clipboard_manager);
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

impl MacOSWindow {
    // NSResponder Undo/Redo Integration (macOS Native)

    /// Perform undo operation (called by NSResponder undo: selector)
    pub fn perform_undo(&mut self) {
        // Get focused node for undo context
        let focused_node = if let Some(layout_window) = self.layout_window.as_ref() {
            layout_window.focus_manager.get_focused_node().copied()
        } else {
            return;
        };

        let target = match focused_node {
            Some(node) => node,
            None => return, // No focused node
        };

        // Get layout window
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return,
        };

        // Convert DomNodeId to NodeId
        let node_id = azul_core::id::NodeId::new(target.node.inner as usize);

        // Pop from undo stack
        if let Some(operation) = layout_window.undo_redo_manager.pop_undo(node_id) {
            // Apply the revert - restore pre-state text
            let node_id_internal = target.node.into_crate_internal();
            if let Some(node_id_internal) = node_id_internal {
                // Create InlineContent from pre-state text
                use std::sync::Arc;

                use azul_layout::text3::cache::{InlineContent, StyleProperties, StyledRun};

                let new_content = vec![InlineContent::Text(StyledRun {
                    text: operation.pre_state.text_content.clone(),
                    style: Arc::new(StyleProperties::default()),
                    logical_start_byte: 0,
                })];

                // Update text cache with pre-state content
                layout_window.update_text_cache_after_edit(
                    target.dom,
                    node_id_internal,
                    new_content,
                );

                // Restore cursor position
                if let Some(cursor) = operation.pre_state.cursor_position {
                    layout_window.cursor_manager.move_cursor_to(
                        cursor,
                        target.dom,
                        node_id_internal,
                    );
                }
            }

            // Push to redo stack after successful undo
            layout_window.undo_redo_manager.push_redo(operation);

            // Mark window for redraw
            unsafe {
                use objc2::msg_send;
                let _: () = msg_send![&*self.window, setViewsNeedDisplay: true];
            }
        }
    }

    /// Perform redo operation (called by NSResponder redo: selector)
    pub fn perform_redo(&mut self) {
        // Get focused node for redo context
        let focused_node = if let Some(layout_window) = self.layout_window.as_ref() {
            layout_window.focus_manager.get_focused_node().copied()
        } else {
            return;
        };

        let target = match focused_node {
            Some(node) => node,
            None => return, // No focused node
        };

        // Get layout window
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return,
        };

        // Convert DomNodeId to NodeId
        let node_id = azul_core::id::NodeId::new(target.node.inner as usize);

        // Pop from redo stack
        if let Some(operation) = layout_window.undo_redo_manager.pop_redo(node_id) {
            // Re-apply the original operation via text input
            let node_id_internal = target.node.into_crate_internal();
            if let Some(_node_id_internal) = node_id_internal {
                use azul_layout::managers::changeset::TextOperation;

                match &operation.changeset.operation {
                    TextOperation::InsertText { text, .. } => {
                        // Re-insert the text
                        let _ = layout_window.process_text_input(text);
                    }
                    _ => {
                        // Other operations not yet fully supported
                    }
                }
            }

            // Push to undo stack after successful redo
            layout_window.undo_redo_manager.push_undo(operation);

            // Mark window for redraw
            unsafe {
                use objc2::msg_send;
                let _: () = msg_send![&*self.window, setViewsNeedDisplay: true];
            }
        }
    }

    /// Check if undo is available (for menu validation)
    pub fn can_undo(&self) -> bool {
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(focused_node) = layout_window.focus_manager.get_focused_node() {
                let node_id = azul_core::id::NodeId::new(focused_node.node.inner as usize);
                return layout_window.undo_redo_manager.can_undo(node_id);
            }
        }
        false
    }

    /// Check if redo is available (for menu validation)
    pub fn can_redo(&self) -> bool {
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(focused_node) = layout_window.focus_manager.get_focused_node() {
                let node_id = azul_core::id::NodeId::new(focused_node.node.inner as usize);
                return layout_window.undo_redo_manager.can_redo(node_id);
            }
        }
        false
    }

    // Accessibility Support

    /// Initialize accessibility support for the window
    ///
    /// This should be called once after the first layout pass to set up
    /// the accesskit adapter with the initial accessibility tree.
    #[cfg(feature = "a11y")]
    fn init_accessibility(&mut self) {
        if self.accessibility_adapter.is_some() {
            return; // Already initialized
        }

        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => {
                eprintln!("[a11y] Cannot initialize: no layout window");
                return;
            }
        };

        // Get the root NSView (either GL or CPU view)
        let view_ptr = if let Some(gl_view) = self.gl_view.as_ref() {
            Retained::<GLView>::as_ptr(gl_view) as *mut std::ffi::c_void
        } else if let Some(cpu_view) = self.cpu_view.as_ref() {
            Retained::<CPUView>::as_ptr(cpu_view) as *mut std::ffi::c_void
        } else {
            eprintln!("[a11y] Cannot initialize: no view available");
            return;
        };

        // Create the adapter
        let adapter = accessibility::MacOSAccessibilityAdapter::new(view_ptr);
        self.accessibility_adapter = Some(adapter);

        // Update with initial tree
        self.update_accessibility();

        eprintln!("[a11y] Accessibility adapter initialized");
    }

    /// Update accessibility tree after layout changes
    ///
    /// This should be called after regenerate_layout() to keep the
    /// accessibility tree synchronized with the visual representation.
    #[cfg(feature = "a11y")]
    fn update_accessibility(&mut self) {
        let adapter = match self.accessibility_adapter.as_mut() {
            Some(a) => a,
            None => return, // Not initialized yet
        };

        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return,
        };

        // Generate tree update from current layout
        let tree_update = azul_layout::managers::a11y::A11yManager::update_tree(
            layout_window.a11y_manager.root_id,
            &layout_window.layout_results,
            &self.current_window_state.title,
            self.current_window_state.size.dimensions,
        );

        // Submit to OS
        adapter.update_tree(tree_update);
    }

    /// Poll for accessibility action requests from assistive technologies
    ///
    /// This should be called in the event loop to check if screen readers
    /// have requested any actions (focus, click, scroll, etc.)
    #[cfg(feature = "a11y")]
    pub fn poll_accessibility_actions(
        &mut self,
    ) -> Vec<(
        DomId,
        azul_core::dom::NodeId,
        azul_core::dom::AccessibilityAction,
    )> {
        let adapter = match self.accessibility_adapter.as_ref() {
            Some(a) => a,
            None => return Vec::new(),
        };

        let mut actions = Vec::new();
        while let Some(action) = adapter.poll_action() {
            actions.push(action);
        }
        actions
    }

    /// Inject a menu bar into the window
    ///
    /// On macOS, this creates a native NSMenu hierarchy attached to the application.
    /// Menu callbacks are wired up to trigger when menu items are clicked.
    ///
    /// # Implementation
    /// This method is deprecated in favor of `set_application_menu()` which provides
    /// a complete NSMenu implementation with callback integration.
    ///
    /// # Returns
    /// * `Ok(())` if menu injection succeeded
    /// * `Err(String)` if menu injection failed
    pub fn inject_menu_bar(&mut self) -> Result<(), String> {
        // Native macOS menu integration is fully implemented via set_application_menu()
        // See menu.rs for AzulMenuTarget bridge and MenuState implementation
        eprintln!("[inject_menu_bar] Use set_application_menu() for native macOS menus");
        Ok(())
    }

    /// Gets information about the screen the window is currently on.
    pub fn get_screen_info(&self) -> Option<objc2::rc::Retained<objc2_app_kit::NSScreen>> {
        self.window.screen()
    }

    /// Returns the frame of the window in screen coordinates.
    pub fn get_window_frame(&self) -> objc2_foundation::NSRect {
        self.window.frame()
    }

    /// Returns the DPI scale factor for the window.
    pub fn get_backing_scale_factor(&self) -> f64 {
        self.window.backingScaleFactor()
    }

    /// Get display information for the screen this window is on
    pub fn get_window_display_info(&self) -> Option<crate::desktop::display::DisplayInfo> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let screen = self.get_screen_info()?;
        let frame = screen.frame();
        let visible_frame = screen.visibleFrame();
        let scale = screen.backingScaleFactor();

        let bounds = LogicalRect::new(
            LogicalPosition::new(frame.origin.x as f32, frame.origin.y as f32),
            LogicalSize::new(frame.size.width as f32, frame.size.height as f32),
        );

        let work_area = LogicalRect::new(
            LogicalPosition::new(visible_frame.origin.x as f32, visible_frame.origin.y as f32),
            LogicalSize::new(
                visible_frame.size.width as f32,
                visible_frame.size.height as f32,
            ),
        );

        // Get refresh rate from NSScreen (macOS 10.15+)
        let refresh_rate = unsafe {
            use objc2::msg_send;
            let fps: f64 = msg_send![&**screen, maximumFramesPerSecond];
            if fps > 0.0 {
                fps as u16
            } else {
                60
            }
        };

        Some(crate::desktop::display::DisplayInfo {
            name: screen.localizedName().to_string(),
            bounds,
            work_area,
            scale_factor: scale as f32,
            is_primary: false, // Would need to check if this is the main screen
            video_modes: vec![azul_core::window::VideoMode {
                size: azul_css::props::basic::LayoutSize::new(
                    bounds.size.width as isize,
                    bounds.size.height as isize,
                ),
                bit_depth: 32,
                refresh_rate,
            }],
        })
    }
}

/// Position window on requested monitor, or center on primary monitor
fn position_window_on_monitor(
    window: &Retained<NSWindow>,
    monitor_id: azul_core::window::MonitorId,
    position: azul_core::window::WindowPosition,
    size: azul_core::window::WindowSize,
    mtm: MainThreadMarker,
) {
    use azul_core::window::WindowPosition;
    use objc2_app_kit::NSScreen;

    use crate::desktop::display::get_monitors;

    // Get all available monitors
    let monitors = get_monitors();
    if monitors.len() == 0 {
        unsafe {
            window.center();
        }
        return; // No monitors available, use default centering
    }

    // Get all NSScreens
    let screens = unsafe { NSScreen::screens(mtm) };
    if screens.len() == 0 {
        unsafe {
            window.center();
        }
        return;
    }

    // Determine target monitor
    let target_monitor = monitors
        .as_slice()
        .iter()
        .find(|m| m.monitor_id.index == monitor_id.index)
        .or_else(|| {
            monitors
                .as_slice()
                .iter()
                .find(|m| m.monitor_id.hash == monitor_id.hash && monitor_id.hash != 0)
        })
        .unwrap_or(&monitors.as_slice()[0]); // Fallback to primary

    // Find matching NSScreen by bounds
    let target_screen = unsafe {
        screens
            .iter()
            .find(|screen| {
                let frame = screen.frame();
                (frame.origin.x as isize - target_monitor.position.x).abs() < 10
                    && (frame.origin.y as isize - target_monitor.position.y).abs() < 10
            })
            .unwrap_or_else(|| screens.objectAtIndex(0))
    };

    // Calculate window position
    let screen_frame = unsafe { target_screen.frame() };
    let window_frame = unsafe { window.frame() };

    let (x, y) = match position {
        WindowPosition::Initialized(pos) => {
            // Explicit position requested - use it relative to monitor
            // Note: macOS y-axis is flipped (0 at bottom)
            (
                screen_frame.origin.x + pos.x as f64,
                screen_frame.origin.y + pos.y as f64,
            )
        }
        WindowPosition::Uninitialized => {
            // No explicit position - center on target monitor
            let center_x =
                screen_frame.origin.x + (screen_frame.size.width - window_frame.size.width) / 2.0;
            let center_y =
                screen_frame.origin.y + (screen_frame.size.height - window_frame.size.height) / 2.0;
            (center_x, center_y)
        }
    };

    // Set window frame with new position
    use objc2_foundation::NSRect;
    let new_frame = NSRect {
        origin: objc2_foundation::NSPoint { x, y },
        size: window_frame.size,
    };

    unsafe {
        window.setFrame_display(new_frame, false);
    }
}

// IME Position Management

impl MacOSWindow {
    /// Sync ime_position from window state to OS
    /// On macOS, the IME position is provided via firstRectForCharacterRange,
    /// which is called by the system when needed. We just need to ensure
    /// ime_position is set in window state, and the NSTextInputClient
    /// protocol implementation will return it.
    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;

        // On macOS, no explicit API call needed
        // The system will call firstRectForCharacterRange: when it needs
        // the IME candidate window position, and we return ime_position there

        // However, we can invalidate the marked text to trigger a refresh
        // if we want to force the IME window to update immediately
        if matches!(
            self.current_window_state.ime_position,
            ImePosition::Initialized(_)
        ) {
            // TODO: Could call invalidateMarkable or similar if needed
            // For now, passive approach is sufficient
        }
    }
}
