#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::{
    app::{App, AppConfig},
    window::WindowCreateOptions,
    style::StyledDom,
    css::Css,
    dom::{Dom, NodeData, NodeType},
    fs::File,
    option::OptionFile,
    callbacks::{
        RefAny, LayoutInfo,
        UpdateScreen, TimerCallbackInfo,
        CallbackInfo, TimerCallbackReturn,
    },
    task::{TimerId, Timer, TerminateTimer},
    vec::DomVec,
    str::String as AzString,
};

#[derive(Debug)]
struct Data {
    counter: usize,
}

const DOM_STRING: &str = "hello";
const DOM_CHILD: &[Dom] = &[Dom {
    root: NodeData::new(NodeType::Label(AzString::from_const_str(DOM_STRING))),
    children: DomVec::from_const_slice(&[]),
    total_children: 0,
}];
const DOM_CHILDREN: DomVec = DomVec::from_const_slice(DOM_CHILD);
const DOM: Dom = Dom {
    root: NodeData::body(),
    children: DOM_CHILDREN,
    total_children: 2,
};
const CSS: &str = "body { font-size: 50px; }";

extern "C" fn layout(data: &mut RefAny, _info: LayoutInfo) -> StyledDom {

    let styled_dom = DOM.style(Css::from_string(CSS.into()));

    if let OptionFile::Some(mut file) = File::create("./debug.html".into()) {
        file.write_string(styled_dom.get_html_string().as_refstr());
    }

    styled_dom
}

fn main() {
    let data = RefAny::new(Data { counter: 5 });
    let app = App::new(data, AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}
