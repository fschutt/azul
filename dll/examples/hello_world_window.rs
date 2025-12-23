//! Test that actually opens a window
//!
//! This example can run in two modes:
//! - Static linking (default): Uses internal types directly, better for debugging
//! - Dynamic linking (link-dynamic feature): Uses FFI types, same code path as C
//!
//! Run statically linked (for debugging):
//!   cargo run --bin hello_world_window --package azul-dll --features "link-static"
//!
//! Run dynamically linked (same as C):
//!   cargo run --bin hello_world_window --package azul-dll --features "link-dynamic"

// static linking (internal types) - default mode
#[cfg(not(feature = "link-dynamic"))]
mod static_impl {
    use azul_core::{
        callbacks::{LayoutCallbackInfo, LayoutCallbackType},
        dom::Dom,
        refany::RefAny,
        resources::AppConfig,
        styled_dom::StyledDom,
    };
    use azul_css::css::Css;
    use azul::desktop::app::App;
    use azul_layout::window_state::WindowCreateOptions;

    #[derive(Debug, Clone)]
    pub struct MyDataModel {
        pub counter: u32,
    }

    pub extern "C" fn my_layout_func(mut data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
        eprintln!("[my_layout_func] Called!");
        if let Some(model) = data.downcast_ref::<MyDataModel>() {
            eprintln!("[my_layout_func] counter = {}", model.counter);
        }

        // Create a simple red rectangle
        let mut dom =
            Dom::create_div().with_inline_style("width: 200px; height: 200px; background-color: red;");

        eprintln!("[my_layout_func] Created DOM with red rectangle");
        let styled = dom.style(Css::empty());
        eprintln!(
            "[my_layout_func] StyledDom has {} nodes",
            styled.styled_nodes.len()
        );
        styled
    }

    pub fn run() {
        eprintln!("[main] STATIC LINKING MODE");
        eprintln!("[main] Creating MyDataModel...");
        let model = MyDataModel { counter: 5 };

        eprintln!("[main] Creating RefAny...");
        let data = RefAny::new(model);

        eprintln!("[main] Creating AppConfig...");
        let config = AppConfig::create();

        eprintln!("[main] Creating App...");
        let app = App::create(data, config);

        eprintln!("[main] Creating WindowCreateOptions...");
        let window = WindowCreateOptions::create(my_layout_func as LayoutCallbackType);

        eprintln!("[main] About to call app.run()...");
        app.run(window);

        eprintln!("[main] App finished!");
    }
}

// dynamic linking (ffi types, same as c)
#[cfg(feature = "link-dynamic")]
mod dynamic_impl {
    use core::ffi::c_void;

    use azul_dll::ffi::dll::{
        AzAppConfig_new, AzApp_new, AzApp_run, AzGlVoidPtrConst, AzLayoutCallbackInfo,
        AzLayoutCallbackType, AzRefAny, AzRefAny_newC, AzString, AzStyledDom, AzStyledDom_default,
        AzU8Vec, AzU8VecDestructor, AzWindowCreateOptions_new,
    };

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
                destructor: AzU8VecDestructor::NoDestructor,
                run_destructor: false, // Static slice, no destructor needed
            },
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
    #[cfg(not(feature = "link-dynamic"))]
    static_impl::run();

    #[cfg(feature = "link-dynamic")]
    dynamic_impl::run();
}
