use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::atomic::{AtomicIsize, Ordering},
};

use azul_layout::{callbacks::MenuCallback, window_state::WindowCreateOptions};
use webrender::{
    api::{DocumentId as WrDocumentId, RenderNotifier as WrRenderNotifier},
    ProgramCache as WrProgramCache, RendererOptions as WrRendererOptions,
    ShaderPrecacheFlags as WrShaderPrecacheFlags, Shaders as WrShaders,
};

// ID sent by WM_TIMER to re-generate the DOM
const AZ_TICK_REGENERATE_DOM: usize = 1;
// ID sent by WM_TIMER to check the thread results
const AZ_THREAD_TICK: usize = 2;

pub mod event;
pub mod process;

#[cfg(target_os = "macos")]
pub mod appkit;
#[cfg(target_os = "windows")]
pub mod win32;
#[cfg(target_os = "linux")]
pub mod x11;

// TODO: Cache compiled shaders between renderers
pub const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

fn default_renderer_options(options: &WindowCreateOptions) -> WrRendererOptions {
    use crate::desktop::wr_translate::wr_translate_debug_flags;
    WrRendererOptions {
        resource_override_path: None,
        use_optimized_shaders: true,
        enable_aa: true,
        enable_subpixel_aa: true,
        force_subpixel_aa: true,
        clear_color: webrender::api::ColorF {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }, // transparent
        panic_on_gl_error: false,
        precache_flags: WrShaderPrecacheFlags::EMPTY,
        cached_programs: Some(WrProgramCache::new(None)),
        enable_multithreading: true,
        debug_flags: wr_translate_debug_flags(&options.state.debug_state),
        ..WrRendererOptions::default()
    }
}

struct Notifier {}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {})
    }
    fn wake_up(&self, composite_needed: bool) {}
    fn new_frame_ready(
        &self,
        _: WrDocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>,
    ) {
    }
}

// We'll store: (tag: i32) => MenuCallback
// (On macOS, `tag` is an `NSInteger` or `i64`. We'll just use `i32` for simplicity.)
pub type CommandMap = BTreeMap<CommandId, MenuCallback>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacOsMenuCommands {
    pub menu_hash: u64,
    pub commands: CommandMap,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MenuTarget {
    App,
    Window(isize),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CommandId(pub isize);

impl CommandId {
    pub fn new() -> Self {
        static NEXT_MENU_TAG: AtomicIsize = AtomicIsize::new(0);
        Self(NEXT_MENU_TAG.fetch_add(1, Ordering::SeqCst))
    }
}
