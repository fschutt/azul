use crate::{
    app::{App, LazyFcCache},
    wr_translate::{
        rebuild_display_list,
        generate_frame,
        synchronize_gpu_values,
        scroll_all_nodes,
        wr_synchronize_updated_images,
        AsyncHitTester,
    }
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc
};
use azul_core::{
    FastBTreeSet, FastHashMap,
    app_resources::{
        ImageMask, ImageRef, Epoch,
        AppConfig, ImageCache, ResourceUpdate,
        RendererResources, GlTextureCache,
    },
    callbacks::{
        RefAny, UpdateImageType,
        DomNodeId, DocumentId
    },
    gl::OptionGlContextPtr,
    task::{Thread, ThreadId, Timer, TimerId},
    ui_solver::LayoutResult,
    styled_dom::DomId,
    dom::NodeId,
    display_list::RenderCallbacks,
    window::{
        LogicalSize, Menu, MenuCallback, MenuItem,
        MonitorVec, WindowCreateOptions, WindowInternal,
        WindowState, FullWindowState, ScrollResult,
        MouseCursorType, CallCallbacksResult
    },
    window_state::NodesToCheck,
};
use core::{
    fmt,
    convert::TryInto,
    cell::{BorrowError, BorrowMutError, RefCell},
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use gl_context_loader::GenericGlContext;
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize,
            LayoutSize as WrLayoutSize,
        },
        HitTesterRequest as WrHitTesterRequest,
        ApiHitTester as WrApiHitTester, DocumentId as WrDocumentId,
        RenderNotifier as WrRenderNotifier,
    },
    render_api::RenderApi as WrRenderApi,
    PipelineInfo as WrPipelineInfo, Renderer as WrRenderer, RendererError as WrRendererError,
    RendererOptions as WrRendererOptions, ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders, Transaction as WrTransaction,
};

#[derive(Debug)]
pub enum CocoaWindowCreateError {
    FailedToCreateHWND(u32),
    NoHDC,
    NoGlContext,
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

#[derive(Debug, Copy, Clone)]
pub enum CocoaOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToCreateHiddenHWND(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

#[derive(Debug)]
pub enum CocoaStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(CocoaWindowCreateError),
    Gl(CocoaOpenGlError),
}

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}

/// Main function that starts when app.run() is invoked
pub fn run(app: App, root_window: WindowCreateOptions) -> Result<isize, CocoaStartupError> {
    println!("azul.App.run(cocoa)");
    Ok(0)
}