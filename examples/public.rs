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
    UpdateScreen, TimerCallbackInfo,
    CallbackInfo, TimerCallbackReturn,
};
use azul::task::{TimerId, Timer, TerminateTimer};
use azul::vec::DomVec;
use azul::str::String as AzString;

#[derive(Debug)]
struct Data {
    counter: usize,
}

extern "C" fn layout(data: &mut RefAny, _info: LayoutInfo) -> StyledDom {

    use azul::dom::NodeData;
    use azul::dom::NodeType;

    const DOM_STRING: &str = "hello";
    const DOM_CHILD: &[Dom] = &[Dom {
        root: NodeData::new(NodeType::Label(AzString::from_const_str(DOM_STRING))),
        children: DomVec::from_const_slice(&[]),
        estimated_total_children: 0,
    }];
    const DOM_CHILDREN: DomVec = DomVec::from_const_slice(DOM_CHILD);
    const DOM: Dom = Dom {
        root: NodeData::body(),
        children: DOM_CHILDREN,
        estimated_total_children: 2,
    };

    DOM.style(Css::empty())
}

fn main() {
    use azul::dom::NodeData;

    loop {
        use azul_widgets::table_view::*;
        let mut table_view_state = TableViewState::default();
        table_view_state.set_cell_content(TableCellIndex { row: 2, column: 2 }, "Hello World");
        table_view_state.set_selection(Some(TableCellSelection::from(3, 4).to(6, 7)));
        let dom = table_view_state.render(0..80, 0..300);
        println!("styled dom len: {:?}", dom.node_count());
    }
    /*
    println!("sizeof NodeData * 25000: {:?}", ::std::mem::size_of::<NodeData>() * 25000); // 9.7275 MB

    let data = RefAny::new(Data { counter: 5 });
    let app = App::new(data, AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
    */
}
