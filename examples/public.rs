#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Example of the new, public API

/*
    // to compile on no_std, uncomment this block and
    // uncomment libc_alloc in the dependencies
    //
    // ~>    cd examples && cargo +nightly build --release --bin public

    #![no_std]
    #![feature(start, lang_items, rustc_private, libc, default_alloc_error_handler)]

    #[macro_use]
    extern crate alloc;

    use core::panic::PanicInfo;

    #[panic_handler]
    fn panic(_: &PanicInfo) -> ! { loop {} }
    #[lang = "eh_personality"]
    extern fn eh_personality() {}

    #[global_allocator]
    static ALLOC: libc_alloc::LibcAlloc = libc_alloc::LibcAlloc;

    #[start]
    fn main(_: isize, _: *const *const u8) -> isize {
        let data = Data { counter: 5 };
        let app = App::new(RefAny::new(data), AppConfig::default());
        app.run(WindowCreateOptions::new(layout));
        return 0;
    }
*/

use azul::prelude::*;
use azul::style::StyledDom;
use azul::callbacks::{
    UpdateScreen, TimerCallbackInfo, CallbackInfo,
    TimerCallbackReturn, CallbackReturn,
};
use azul::task::{TimerId, Timer, TerminateTimer};

#[derive(Debug)]
struct Data {
    counter: usize,
}

extern "C" fn layout(data: &RefAny, _info: LayoutInfo) -> StyledDom {
    let data = data.downcast_ref::<Data>().unwrap();
    Dom::body().with_child(
        Dom::label("h".into())
    ).style(Css::empty())
}

fn main() {
    use azul::dom::NodeData;
    use azul::style::RectStyle;
    use azul::style::RectLayout;
    use azul::vec::CssPropertyVec;
    use azul::vec::StringVec;

    // rectstyle: 1192 bytes
    // rectlayout: 896 bytes

    // total optimization = 8.3 MB saved @ 4000 nodes (2184 bytes per node)
    // 24000 nodes -> ~50MB (currently: 250MB)

    let data = Data { counter: 5 };
    let app = App::new(RefAny::new(data), AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}
