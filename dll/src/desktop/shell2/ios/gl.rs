//! OpenGL ES on iOS: the platform layer under the GPU path.
//!
//! `OpenGLES.framework` (EAGL) is deprecated since iOS 12 and still shipped; it is the only API the vendored
//! WebRender can drive (see `briefs/mobile_gpu_ios_design_2026-11-26.md`). Nothing here links it: the framework is
//! opened by path at the first window that asks for the GPU, the GL entry points are resolved by name into the
//! engine's one GL table ([`load_gl_context`]), and `EAGLContext` / `CAEAGLLayer` are found by name in the
//! Objective-C runtime. An app that draws on the CPU never loads the framework, and the build needs no
//! `-framework` flag and imports no symbol newer than the minimum OS.
//!
//! What is here, and what is not: the pieces ([`EaglLibrary`], [`EaglContext`], [`Drawable`]) and the failures they
//! report ([`GpuFallback`] with the stage of `gpu_select`). WHEN each is made, resized, destroyed or abandoned is
//! `shell2/common/gpu_lifecycle.rs`'s decision, performed by `ios/gpu.rs`. Everything runs on the main thread:
//! UIKit's layers and an `EAGLContext` current on one thread are neither of them thread-safe.

use std::{
    ffi::{c_void, CStr, CString},
    mem::ManuallyDrop,
    rc::Rc,
};

use azul_core::glconst;
use gl_context_loader::GenericGlContext;
use objc2::{
    encode::{Encode, Encoding},
    msg_send,
    runtime::{AnyClass, AnyObject, Bool},
};

use crate::desktop::shell2::common::{
    gl_loader::load_gl_context,
    gpu_lifecycle::Surface,
    gpu_select::{DeviceInfo, GpuFallback, GpuStage},
};

/// Where the framework lives on every iOS (and on the Simulator's runtime).
const OPENGLES_PATH: &str = "/System/Library/Frameworks/OpenGLES.framework/OpenGLES";

/// `kEAGLRenderingAPIOpenGLES3`. OpenGL ES 3.0 is what WebRender needs; there is no ES 2.0 fallback.
const EAGL_RENDERING_API_OPENGLES3: usize = 3;

// ----- Core Graphics geometry, as objc2 wants it described -------------------------------------

/// `CGPoint`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

/// `CGSize`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

/// `CGRect`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

// SAFETY: the layouts above are those of Core Graphics' structs on a 64-bit Apple target (CGFloat = double).
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding =
        Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

fn class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

fn fail(stage: GpuStage, cause: impl AsRef<str>) -> GpuFallback {
    GpuFallback::new(stage, cause.as_ref())
}

// ----- the framework and the GL table ----------------------------------------------------------

/// `OpenGLES.framework`, opened. Never closed: the GL function table points into it, and the system's
/// frameworks stay mapped for the life of the process anyway.
pub struct EaglLibrary {
    library: ManuallyDrop<libloading::Library>,
}

impl EaglLibrary {
    /// Open the framework (stage [`GpuStage::Library`]).
    pub fn open() -> Result<Self, GpuFallback> {
        // SAFETY: opening a system framework runs its initialisers, as every app that links it does.
        let library = unsafe { libloading::Library::new(OPENGLES_PATH) }
            .map_err(|e| fail(GpuStage::Library, format!("dlopen {OPENGLES_PATH}: {e}")))?;
        Ok(Self {
            library: ManuallyDrop::new(library),
        })
    }

    /// The address of the function `name` (null when the framework has none: most of the desktop GL names the
    /// table asks for do not exist on OpenGL ES, and WebRender only calls what its capability checks allow).
    fn function(&self, name: &str) -> *mut c_void {
        let Ok(c_name) = CString::new(name) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the symbol is only turned into an address here, never called.
        match unsafe {
            self.library
                .get::<unsafe extern "C" fn()>(c_name.to_bytes_with_nul())
        } {
            Ok(symbol) => *symbol as *mut c_void,
            Err(_) => std::ptr::null_mut(),
        }
    }

    /// An exported `NSString *` constant of the framework (`kEAGLColorFormatRGBA8`), or null.
    fn string_constant(&self, name: &str) -> *mut AnyObject {
        let Ok(c_name) = CString::new(name) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the symbol is a variable holding an `NSString *`; the address of the variable is read as a
        // pointer to one, and the variable's value is read once.
        match unsafe {
            self.library
                .get::<*const *mut AnyObject>(c_name.to_bytes_with_nul())
        } {
            Ok(symbol) => {
                let variable: *const *mut AnyObject = *symbol;
                if variable.is_null() {
                    std::ptr::null_mut()
                } else {
                    // SAFETY: see above.
                    unsafe { *variable }
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }

    /// The engine's GL table, filled from this framework (stage [`GpuStage::Functions`]).
    pub fn functions(&self) -> Result<Rc<GenericGlContext>, GpuFallback> {
        let table = load_gl_context(|name| self.function(name) as *mut _);
        // the entry points everything else stands on
        for (name, address) in [
            ("glGetString", table.glGetString),
            ("glCreateShader", table.glCreateShader),
            ("glGenFramebuffers", table.glGenFramebuffers),
            ("glBindFramebuffer", table.glBindFramebuffer),
            ("glFinish", table.glFinish),
        ] {
            if address.is_null() {
                return Err(fail(
                    GpuStage::Functions,
                    format!("{name} is not exported by {OPENGLES_PATH}"),
                ));
            }
        }
        Ok(Rc::new(table))
    }
}

// ----- the context -----------------------------------------------------------------------------

/// An `EAGLContext` of OpenGL ES 3.0. Owned (+1); current on the thread that made it current.
pub struct EaglContext {
    object: *mut AnyObject,
}

impl EaglContext {
    /// Make the context (stage [`GpuStage::Context`]).
    pub fn new() -> Result<Self, GpuFallback> {
        let Some(cls) = class(c"EAGLContext") else {
            return Err(fail(GpuStage::Context, "the class EAGLContext is not registered"));
        };
        // SAFETY: `+alloc` and `-initWithAPI:` of EAGLContext; the result is owned.
        let object: *mut AnyObject = unsafe {
            let allocated: *mut AnyObject = msg_send![cls, alloc];
            msg_send![allocated, initWithAPI: EAGL_RENDERING_API_OPENGLES3]
        };
        if object.is_null() {
            return Err(fail(
                GpuStage::Context,
                "-[EAGLContext initWithAPI:] gave nil for OpenGL ES 3.0 (no ES 3.0 on this device)",
            ));
        }
        Ok(Self { object })
    }

    /// Is this context the one current on the calling thread?
    #[must_use]
    pub fn is_current(&self) -> bool {
        let Some(cls) = class(c"EAGLContext") else {
            return false;
        };
        // SAFETY: a class method of EAGLContext.
        let current: *mut AnyObject = unsafe { msg_send![cls, currentContext] };
        current == self.object
    }

    /// Make it current on the calling thread (a no-op when it is).
    pub fn make_current(&self) -> Result<(), GpuFallback> {
        if self.is_current() {
            return Ok(());
        }
        let Some(cls) = class(c"EAGLContext") else {
            return Err(fail(GpuStage::MakeCurrent, "the class EAGLContext is not registered"));
        };
        // SAFETY: a class method of EAGLContext, with an object this struct owns.
        let made: Bool = unsafe { msg_send![cls, setCurrentContext: self.object] };
        if made.as_bool() {
            Ok(())
        } else {
            Err(fail(
                GpuStage::MakeCurrent,
                "+[EAGLContext setCurrentContext:] returned NO",
            ))
        }
    }

    /// Nothing is current on the calling thread any more.
    pub fn clear_current() {
        if let Some(cls) = class(c"EAGLContext") {
            // SAFETY: a class method of EAGLContext; nil is the documented way to unset.
            let _: Bool = unsafe { msg_send![cls, setCurrentContext: std::ptr::null_mut::<AnyObject>()] };
        }
    }

    /// Allocate the bound renderbuffer's storage from `layer` (`-renderbufferStorage:fromDrawable:`).
    fn renderbuffer_storage(&self, layer: *mut AnyObject) -> bool {
        // SAFETY: an instance method of EAGLContext; `layer` is a live CAEAGLLayer.
        let stored: Bool = unsafe {
            msg_send![self.object, renderbufferStorage: glconst::RENDERBUFFER as usize, fromDrawable: layer]
        };
        stored.as_bool()
    }

    /// Present the bound renderbuffer (`-presentRenderbuffer:`).
    fn present_renderbuffer(&self) -> bool {
        // SAFETY: an instance method of EAGLContext.
        let presented: Bool = unsafe { msg_send![self.object, presentRenderbuffer: glconst::RENDERBUFFER as usize] };
        presented.as_bool()
    }

    /// What the context says about itself. Needs it current.
    #[must_use]
    pub fn device_info(gl: &GenericGlContext) -> DeviceInfo {
        DeviceInfo {
            vendor: gl.get_string(glconst::VENDOR),
            renderer: gl.get_string(glconst::RENDERER),
            version: gl.get_string(glconst::VERSION),
            glsl: gl.get_string(glconst::SHADING_LANGUAGE_VERSION),
        }
    }

    /// Give the object up WITHOUT releasing it (the app is in the background: nothing may flush a GL command, and
    /// a context's `dealloc` does).
    pub fn leak(self) {
        std::mem::forget(self);
    }
}

impl Drop for EaglContext {
    fn drop(&mut self) {
        if !self.object.is_null() {
            // SAFETY: the object was made by `alloc` / `init` in `new` and is released once.
            let _: () = unsafe { msg_send![self.object, release] };
        }
    }
}

// ----- the drawable: a CAEAGLLayer, a framebuffer, a renderbuffer -------------------------------

/// The surface the GPU path draws on: a `CAEAGLLayer` inserted below the view's other layers, and a framebuffer
/// whose colour attachment is that layer's renderbuffer. WebRender takes the framebuffer bound at the start of a
/// frame as its default one, so [`Drawable::bind`] before `render`, [`Drawable::present`] after it. There is no
/// depth or stencil buffer: WebRender's default target uses none.
pub struct Drawable {
    gl: Rc<GenericGlContext>,
    /// The `CAEAGLLayer`, owned (+1) and held by its superlayer too.
    layer: *mut AnyObject,
    framebuffer: u32,
    renderbuffer: u32,
    width: i32,
    height: i32,
}

impl Drawable {
    /// Make the drawable under `parent` (the render view's layer), `size` big. The context must be current.
    pub fn create(
        library: &EaglLibrary,
        context: &EaglContext,
        gl: &Rc<GenericGlContext>,
        parent: *mut AnyObject,
        size: Surface,
    ) -> Result<Self, GpuFallback> {
        let surface = |cause: &str| fail(GpuStage::Surface, cause);
        let Some(layer_class) = class(c"CAEAGLLayer") else {
            return Err(surface("the class CAEAGLLayer is not registered"));
        };
        if parent.is_null() {
            return Err(surface("the render view has no layer"));
        }
        // the framebuffer cannot be bigger than the GPU's renderbuffers
        let mut limit = [0_i32];
        // SAFETY: one int is written.
        unsafe { gl.get_integer_v(glconst::MAX_RENDERBUFFER_SIZE, &mut limit) };
        let edge = i32::try_from(size.width.max(size.height)).unwrap_or(i32::MAX);
        if limit[0] > 0 && edge > limit[0] {
            return Err(surface(&format!(
                "{}x{} is larger than GL_MAX_RENDERBUFFER_SIZE {}",
                size.width, size.height, limit[0]
            )));
        }

        // SAFETY: `+alloc` / `-init` of CAEAGLLayer; owned.
        let layer: *mut AnyObject = unsafe {
            let allocated: *mut AnyObject = msg_send![layer_class, alloc];
            msg_send![allocated, init]
        };
        if layer.is_null() {
            return Err(surface("-[CAEAGLLayer init] gave nil"));
        }
        let mut drawable = Self {
            gl: gl.clone(),
            layer,
            framebuffer: 0,
            renderbuffer: 0,
            width: 0,
            height: 0,
        };
        // SAFETY: the layer is live and made here; `parent` is the view's layer on the main thread.
        unsafe {
            let _: () = msg_send![layer, setOpaque: Bool::YES];
            drawable.set_drawable_properties(library);
            drawable.place(parent, size.scale);
            let _: () = msg_send![parent, insertSublayer: layer, atIndex: 0_u32];
        }

        let (Some(&framebuffer), Some(&renderbuffer)) =
            (gl.gen_framebuffers(1).first(), gl.gen_renderbuffers(1).first())
        else {
            drawable.destroy();
            return Err(surface("glGenFramebuffers / glGenRenderbuffers gave no name"));
        };
        drawable.framebuffer = framebuffer;
        drawable.renderbuffer = renderbuffer;
        gl.bind_framebuffer(glconst::FRAMEBUFFER, framebuffer);
        gl.bind_renderbuffer(glconst::RENDERBUFFER, renderbuffer);
        if let Err(e) = drawable.attach(context) {
            drawable.destroy();
            return Err(e);
        }
        Ok(drawable)
    }

    /// `drawableProperties`: the contents are not kept after a present (every frame is drawn in full), 8 bits per
    /// channel. Both are the documented defaults, named anyway so that the surface says what the renderer
    /// assumes; the keys and the value are the framework's own exported constants, and when the lookup of one fails
    /// the defaults stand.
    unsafe fn set_drawable_properties(&self, library: &EaglLibrary) {
        let (Some(dictionary_class), Some(number_class)) =
            (class(c"NSMutableDictionary"), class(c"NSNumber"))
        else {
            return;
        };
        let key_backing = library.string_constant("kEAGLDrawablePropertyRetainedBacking");
        let key_format = library.string_constant("kEAGLDrawablePropertyColorFormat");
        let rgba8 = library.string_constant("kEAGLColorFormatRGBA8");
        if key_backing.is_null() || key_format.is_null() || rgba8.is_null() {
            return;
        }
        // SAFETY: Foundation objects made and used on the main thread; the layer retains the dictionary.
        unsafe {
            let dictionary: *mut AnyObject = msg_send![dictionary_class, dictionary];
            let no: *mut AnyObject = msg_send![number_class, numberWithBool: Bool::NO];
            let _: () = msg_send![dictionary, setObject: no, forKey: key_backing];
            let _: () = msg_send![dictionary, setObject: rgba8, forKey: key_format];
            let _: () = msg_send![self.layer, setDrawableProperties: dictionary];
        }
    }

    /// The layer fills its parent and has the screen's scale; no implicit animation (a layer made by hand
    /// animates every change of its frame).
    unsafe fn place(&self, parent: *mut AnyObject, scale: f32) {
        // SAFETY: UIKit / Core Animation on the main thread; both objects are live.
        unsafe {
            if let Some(transaction) = class(c"CATransaction") {
                let _: () = msg_send![transaction, begin];
                let _: () = msg_send![transaction, setDisableActions: Bool::YES];
            }
            let bounds: CGRect = msg_send![parent, bounds];
            let _: () = msg_send![self.layer, setContentsScale: f64::from(scale)];
            let _: () = msg_send![self.layer, setFrame: bounds];
            if let Some(transaction) = class(c"CATransaction") {
                let _: () = msg_send![transaction, commit];
            }
        }
    }

    /// The renderbuffer gets its storage from the layer and becomes the framebuffer's colour attachment; the
    /// size it got is read back (stage [`GpuStage::Surface`]). Both are bound.
    fn attach(&mut self, context: &EaglContext) -> Result<(), GpuFallback> {
        let gl = self.gl.clone();
        if !context.renderbuffer_storage(self.layer) {
            return Err(fail(
                GpuStage::Surface,
                "-[EAGLContext renderbufferStorage:fromDrawable:] returned NO",
            ));
        }
        gl.framebuffer_renderbuffer(
            glconst::FRAMEBUFFER,
            glconst::COLOR_ATTACHMENT0,
            glconst::RENDERBUFFER,
            self.renderbuffer,
        );
        self.width = gl.get_renderbuffer_parameter_iv(glconst::RENDERBUFFER, glconst::RENDERBUFFER_WIDTH);
        self.height = gl.get_renderbuffer_parameter_iv(glconst::RENDERBUFFER, glconst::RENDERBUFFER_HEIGHT);
        let status = gl.check_frame_buffer_status(glconst::FRAMEBUFFER);
        if status != glconst::FRAMEBUFFER_COMPLETE || self.width <= 0 || self.height <= 0 {
            return Err(fail(
                GpuStage::Surface,
                format!(
                    "the framebuffer is not complete (status 0x{status:04x}, renderbuffer {}x{})",
                    self.width, self.height
                ),
            ));
        }
        Ok(())
    }

    /// The drawable has another size or scale: the layer follows its parent, and the renderbuffer's storage is
    /// allocated again from it. The context must be current.
    pub fn resize(
        &mut self,
        context: &EaglContext,
        parent: *mut AnyObject,
        size: Surface,
    ) -> Result<(), GpuFallback> {
        // SAFETY: as in `create`.
        unsafe { self.place(parent, size.scale) };
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, self.framebuffer);
        self.gl.bind_renderbuffer(glconst::RENDERBUFFER, self.renderbuffer);
        self.attach(context)
    }

    /// The framebuffer WebRender draws into (its colour renderbuffer is the layer's).
    pub fn bind(&self) {
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, self.framebuffer);
    }

    /// The size of the renderbuffer in device pixels, as the layer gave it.
    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Show the frame (`-presentRenderbuffer:`).
    pub fn present(&self, context: &EaglContext) -> Result<(), GpuFallback> {
        self.gl.bind_renderbuffer(glconst::RENDERBUFFER, self.renderbuffer);
        if context.present_renderbuffer() {
            Ok(())
        } else {
            Err(fail(GpuStage::Frame, "-[EAGLContext presentRenderbuffer:] returned NO"))
        }
    }

    /// Delete the GL objects (the context current, GL allowed) and take the layer off the view.
    pub fn destroy(&mut self) {
        if self.framebuffer != 0 || self.renderbuffer != 0 {
            self.gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
            self.gl.bind_renderbuffer(glconst::RENDERBUFFER, 0);
            if self.renderbuffer != 0 {
                self.gl.delete_renderbuffers(&[self.renderbuffer]);
            }
            if self.framebuffer != 0 {
                self.gl.delete_framebuffers(&[self.framebuffer]);
            }
            self.framebuffer = 0;
            self.renderbuffer = 0;
        }
        self.remove_layer();
    }

    /// Give the GL objects up WITHOUT a GL command (the app is in the background, or the context is lost); the
    /// layer is taken off the view (Core Animation, not GL).
    pub fn abandon(&mut self) {
        self.framebuffer = 0;
        self.renderbuffer = 0;
        self.remove_layer();
    }

    fn remove_layer(&mut self) {
        if !self.layer.is_null() {
            // SAFETY: the layer is live (owned +1); removed from its superlayer, then released once.
            unsafe {
                let _: () = msg_send![self.layer, removeFromSuperlayer];
                let _: () = msg_send![self.layer, release];
            }
            self.layer = std::ptr::null_mut();
        }
    }
}

impl Drop for Drawable {
    fn drop(&mut self) {
        // no GL here: whoever owns the drawable chose `destroy` (GL allowed) or `abandon` (it is not)
        self.remove_layer();
    }
}
