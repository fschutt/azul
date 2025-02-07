#![cfg(target_os = "macos")]
use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex};
use objc2::declare::ClassDecl;
use objc2::runtime::{AnyObject, Class, Object, Sel};
use objc2::*;
use azul_core::window::MonitorVec;
use azul_core::window::WindowCreateOptions;
use crate::app::App;
use objc2::runtime::YES;
use objc2::rc::{autoreleasepool, AutoreleasePool, Retained};
use objc2_app_kit::{NSView, NSWindowStyleMask};
use objc2_app_kit::NSApp;
use objc2_foundation::MainThreadMarker;
use objc2::ffi::id;
use objc2_foundation::NSRect;
use objc2_foundation::NSString;
use objc2::ffi::nil;
use objc2_app_kit::NSWindow;
use objc2_foundation::NSSize;
use objc2_foundation::NSPoint;
use std::ffi::c_void;
use objc2_app_kit::NSBackingStoreType;
use objc2_app_kit::NSApplicationActivationPolicy;

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
}

const GL_COLOR_BUFFER_BIT: u32 = 0x00004000;


#[derive(Debug, Clone)]
struct AppData {
    test: &'static str,
}

// In your actual code, you have this function to build and send
// the WebRender display list. This is just a minimal placeholder:
fn rebuild_display_list() {
    // In a real application, you'd:
    // 1) Gather your layout results
    // 2) Build your WebRender display list
    // 3) Send it to the WebRender API
    // For demonstration, just a stub:
    println!("(Stub) rebuild_display_list called");
}

pub struct MacApp {
    data: Arc<Mutex<AppData>>,
}

pub fn get_monitors(app: &App) -> MonitorVec {
    azul_core::window::MonitorVec::from_const_slice(&[]) // TODO
}

pub fn run(options: WindowCreateOptions) -> Result<(), String> {

    println!("cocoa run");

    let s = MacApp {
        data: Arc::new(Mutex::new(AppData {
            test: "hello"
        })),
    };

    let mtm = MainThreadMarker::new().unwrap();

    autoreleasepool(|app| {
        let app = NSApp(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        let _window_id = unsafe { create_nswindow(s.data.clone(), options) };
        app.activateIgnoringOtherApps(true);
        app.run();
        Ok(())
    })
}

// -----------------------------------------------------------------------------
// Creating the NSWindow and hooking up an NSOpenGLView
// -----------------------------------------------------------------------------

unsafe fn create_nswindow(data: Arc<Mutex<AppData>>, options: WindowCreateOptions) -> Retained<NSWindow> {
    
    let width = options.state.size.dimensions.width as f64;
    let height = options.state.size.dimensions.height as f64;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));

    let style_mask = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::Miniaturizable;

    let mtm = MainThreadMarker::new().unwrap();
    println!("NSWindow::alloc");
    let window = NSWindow::alloc(mtm);
    let window = NSWindow::initWithContentRect_styleMask_backing_defer(
        window,
        rect,
        style_mask,
        NSBackingStoreType::Buffered,
        false,
    );
    println!("NSWindow::window");

    window.center();

    window.setTitle(&NSString::from_str(&options.state.title));

    // Create the custom view that will handle drawing:
    let gl_view = create_opengl_view(rect, data);

    println!("setting content to GL view");

    // Make gl_view the content view of the window:
    window.setContentView(Some(&*(gl_view as *const _ as *const NSView)));

    // If the user wants the window sized to content, you would measure the
    // "rebuild_display_list" result and resize to that. Skipping for brevity.

    println!("makeKeyAndOrderFront");

    window.makeKeyAndOrderFront(None);

    window
}


/// Override of `[NSOpenGLView initWithFrame:pixelFormat:]`.
///
/// - Chains up to `[super initWithFrame:frame pixelFormat:format]`.
/// - You can do additional initialization if needed.
extern "C" fn init_with_frame_pixel_format(
    this: *mut Object,
    _sel: Sel,
    frame: NSRect,
    format: *mut Object
) -> *mut Object {
    unsafe {
        // Call [super initWithFrame:frame pixelFormat:format]
        let superclass = class!(NSOpenGLView);
        let this: *mut Object = msg_send![super(this, superclass),
            initWithFrame: frame
            pixelFormat: format
        ];
        // You could do more setup logic if `this` != null.
        this
    }
}

/// Creates a custom subclass of `NSOpenGLView` and returns an instance of it,
/// storing `data` in an ivar so you can use it during drawing.
pub fn create_opengl_view(frame: NSRect, data: Arc<Mutex<AppData>>) -> id { // Retained<NSOpenGLView>
    unsafe {
        //
        // 1) Declare and register a new Objective-C class: "MyOpenGLView"
        //
        let superclass = class!(NSOpenGLView);

        let c = CString::new("MyOpenGLView").unwrap();
        let mut decl = ClassDecl::new(&c.as_c_str(), superclass)
        .expect("Failed to create ClassDecl for MyOpenGLView");

        // We add an ivar to store an Arc<Mutex<AppData>> pointer. We can store any user data here.
        let i = CString::new("myAppDataPointer").unwrap();
        decl.add_ivar::<*mut core::ffi::c_void>(&i.as_c_str());

        // Add method overrides
        decl.add_method(
            sel!(initWithFrame:pixelFormat:),
            init_with_frame_pixel_format as extern "C" fn(*mut Object, Sel, NSRect, *mut Object) -> *mut Object
        );
        decl.add_method(sel!(drawRect:), 
            draw_rect as extern "C" fn(*mut Object, Sel, NSRect)
        );

        // Register the new class
        let cls = decl.register();

        //
        // 2) Create an NSOpenGLPixelFormat for your desired settings
        //
        let attrs = [
            objc2_app_kit::NSOpenGLPFAAccelerated,
            objc2_app_kit::NSOpenGLPFADoubleBuffer,
            objc2_app_kit::NSOpenGLPFAColorSize,
            24,
            objc2_app_kit::NSOpenGLPFADepthSize,
            24,
            objc2_app_kit::NSOpenGLPFAStencilSize,
            8,
            objc2_app_kit::NSOpenGLPFAOpenGLProfile,
            objc2_app_kit::NSOpenGLProfileVersion3_2Core,
            0, // terminator
        ];

        let pixel_format: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
        let pixel_format: *mut Object = msg_send![
            pixel_format,
            initWithAttributes: attrs.as_ptr()
        ];
        assert!(!pixel_format.is_null(), "Failed to create NSOpenGLPixelFormat");

        //
        // 3) Allocate and init our "MyOpenGLView" instance
        //
        let view: *mut Object = msg_send![cls, alloc];
        let view: *mut Object = msg_send![view, initWithFrame: frame pixelFormat: pixel_format];
        assert!(!view.is_null(), "Failed to init MyOpenGLView");

        // Store Arc<Mutex<AppData>> pointer in the ivar
        let ptr_to_appdata = Arc::into_raw(data) as *mut c_void;
        *((*view).get_mut_ivar("myAppDataPointer")) = ptr_to_appdata;

        // Tell the view to use best resolution (Retina)
        let _: () = msg_send![view, setWantsBestResolutionOpenGLSurface: YES];

        view
    }
}

/// Override of `[NSOpenGLView drawRect:]`.
///
/// 1. Grab the pointer to `myAppDataPointer` and cast it back to `Arc<Mutex<AppData>>`.
/// 2. Make the current OpenGL context current.
/// 3. Perform any OpenGL calls.
/// 4. Flush buffers.
extern "C" fn draw_rect(this: *mut AnyObject, _sel: Sel, _dirty_rect: NSRect) {
    unsafe {
        // Retrieve the pointer to our Arc<Mutex<AppData>> from the ivar
        let ptr: &(*mut c_void) = (&*this).get_ivar("myAppDataPointer");
        let app_data: Arc<Mutex<AppData>> = Arc::from_raw(*ptr as *const _);
        // Immediately turn it back into a raw pointer so we don't drop it
        let _ = Arc::into_raw(app_data.clone());

        println!("draw rect: {}", app_data.lock().unwrap().test);

        // Obtain the OpenGL context
        let gl_context: *mut Object = msg_send![this, openGLContext];
        // Make it current
        let _: () = msg_send![gl_context, makeCurrentContext];

        // ---- Perform your GL calls here ----
        // e.g. clearing the screen, drawing geometry, etc.
        // Clear the screen to green
        glClearColor(0.0, 1.0, 0.0, 1.0);
        glClear(GL_COLOR_BUFFER_BIT);

        // Swap buffers
        let _: () = msg_send![gl_context, flushBuffer];
    }
}
