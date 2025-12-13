//! Test that actually opens a window
//!
//! This example can run in two modes:
//! - Static linking (default): Uses internal types directly, better for debugging
//! - Dynamic linking (c-api feature): Uses FFI types, same code path as C
//!
//! Run statically linked (for debugging):
//!   cargo run --bin hello_world_window --package azul-dll --features "desktop"
//!
//! Run dynamically linked (same as C):
//!   cargo run --bin hello_world_window --package azul-dll --features "c-api desktop"

// ===== Static linking (internal types) =====
#[cfg(not(feature = "c-api"))]
mod static_impl {
    use azul_core::{
        refany::RefAny,
        resources::AppConfig,
        styled_dom::StyledDom,
        dom::Dom,
        callbacks::{LayoutCallbackInfo, LayoutCallbackType},
    };
    use azul_css::{css::Css, parser2::CssApiWrapper};
    use azul_dll::desktop::app::App;
    use azul_layout::window_state::WindowCreateOptions;

    #[derive(Debug, Clone)]
    pub struct MyDataModel {
        pub counter: u32,
    }

    pub extern "C" fn my_layout_func(data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
        eprintln!("[my_layout_func] Called!");
        if let Some(model) = data.downcast_ref::<MyDataModel>() {
            eprintln!("[my_layout_func] counter = {}", model.counter);
        }
        
        // Create a simple red rectangle
        let mut dom = Dom::div()
            .with_inline_style("width: 200px; height: 200px; background-color: red;");
        
        eprintln!("[my_layout_func] Created DOM with red rectangle");
        let styled = dom.style(CssApiWrapper { css: Css::empty() });
        eprintln!("[my_layout_func] StyledDom has {} nodes", styled.styled_nodes.len());
        styled
    }

    pub fn run() {
        eprintln!("[main] STATIC LINKING MODE");
        eprintln!("[main] Creating MyDataModel...");
        let model = MyDataModel { counter: 5 };
        
        eprintln!("[main] Creating RefAny...");
        let data = RefAny::new(model);
        
        eprintln!("[main] Creating AppConfig...");
        let config = AppConfig::new();
        
        eprintln!("[main] Creating App...");
        let app = App::new(data, config);
        
        eprintln!("[main] Creating WindowCreateOptions...");
        let window = WindowCreateOptions::new(my_layout_func as LayoutCallbackType);
        
        eprintln!("[main] About to call app.run()...");
        app.run(window);
        
        eprintln!("[main] App finished!");
    }
}

// ===== Dynamic linking (FFI types, same as C) =====
#[cfg(feature = "c-api")]
mod dynamic_impl {
    use azul_dll::ffi::dll::{
        AzRefAny, AzStyledDom, 
        AzLayoutCallbackInfo, AzLayoutCallbackType,
        AzApp_new, AzApp_run, AzAppConfig_new, AzWindowCreateOptions_new,
        AzStyledDom_default, AzRefAny_newC, AzString, AzU8Vec, AzGlVoidPtrConst,
        AzU8VecDestructor,
    };
    use core::ffi::c_void;

    #[derive(Debug, Clone)]
    pub struct MyDataModel {
        pub counter: u32,
    }

    extern "C" fn my_destructor(_ptr: *mut c_void) {
        eprintln!("[my_destructor] Called!");
    }

    pub extern "C" fn my_layout_func(_data: AzRefAny, _info: AzLayoutCallbackInfo) -> AzStyledDom {
        eprintln!("[my_layout_func] Called!");
        unsafe { AzStyledDom_default() }
    }

    pub fn run() {
        eprintln!("[main] DYNAMIC LINKING MODE (FFI, same as C)");
        eprintln!("[main] Creating MyDataModel...");
        let model = Box::new(MyDataModel { counter: 5 });
        let model_ptr = Box::into_raw(model) as *const c_void;
        
        eprintln!("[main] Creating AzRefAny via FFI...");
        let type_name_bytes = b"MyDataModel";
        let type_name = AzString {
            vec: AzU8Vec {
                ptr: type_name_bytes.as_ptr() as *const u8,
                len: type_name_bytes.len(),
                cap: type_name_bytes.len(),
                destructor: AzU8VecDestructor::DefaultRust,
            }
        };
        
        let ptr_wrapper = AzGlVoidPtrConst {
            ptr: model_ptr,
            run_destructor: false,
        };
        
        let data = unsafe {
            AzRefAny_newC(
                ptr_wrapper,
                std::mem::size_of::<MyDataModel>(),
                std::mem::align_of::<MyDataModel>(),
                0x12345678,
                type_name,
                my_destructor,
            )
        };
        
        eprintln!("[main] Creating AzAppConfig via FFI...");
        let config = unsafe { AzAppConfig_new() };
        
        eprintln!("[main] Creating AzApp via FFI...");
        let app = unsafe { AzApp_new(data, config) };
        
        eprintln!("[main] Creating AzWindowCreateOptions via FFI...");
        let window = unsafe { AzWindowCreateOptions_new(my_layout_func as AzLayoutCallbackType) };
        
        eprintln!("[main] About to call AzApp_run()...");
        unsafe { AzApp_run(&app, window) };
        
        eprintln!("[main] App finished!");
    }
}

fn main() {
    #[cfg(not(feature = "c-api"))]
    static_impl::run();
    
    #[cfg(feature = "c-api")]
    dynamic_impl::run();
}
