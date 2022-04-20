use azul::prelude::*;
use azul::str::String as AzString;
use azul::widgets::{Button, Label};

struct DataModel {
    counter: usize,
}

extern "C"
fn myLayoutFunc(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {

    get_dom()

    /*Dom::body().with_child(
        Dom::text("Test".into())
        .with_inline_style("font-size: 50px;".into())
    ).style(Css::empty())*/
}

fn get_dom() -> StyledDom {

    Dom::body()
    .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
    .with_children(vec![
       Dom::div()
       // .with_class("__azul-native-scroll-root-component".into())
       .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
       .with_children(vec![

           Dom::div()
           // .with_class("__azul-native-scroll-vertical-container".into())
           .with_inline_style("display:flex; flex-grow:1; flex-direction:column-reverse;".into())
           .with_children(vec![

               Dom::div()
               // .with_class("__azul-native-scroll-horizontal-scrollbar".into())
               .with_inline_style("display:flex; flex-grow:1; flex-direction:row; height:15px; max-height:15px; background:grey;".into())
               .with_children(vec![
                   Dom::div(),
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-left".into()),
                   Dom::div()
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-middle".into())
                   .with_children(vec![
                       Dom::div()
                       // .with_class("__azul-native-scroll-horizontal-scrollbar-track-thumb".into())
                   ].into()),
                   Dom::div()
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-right".into()),
               ].into()),

               Dom::div()
               // .with_class("__azul-native-scroll-content-container-1".into())
               .with_inline_style("display:flex; flex-grow:1; flex-direction:row-reverse;".into())
               .with_children(vec![

                   Dom::div()
                   // .with_class("__azul-native-scroll-vertical-scrollbar".into())
                   .with_inline_style("display:flex; flex-grow:1; flex-direction:column; width:15px; max-width:15px; background:grey;".into())
                   .with_children(vec![
                      Dom::div(),
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-top".into()),
                      Dom::div()
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-middle".into())
                      .with_children(vec![
                          Dom::div()
                          // .with_class("__azul-native-scroll-vertical-scrollbar-track-thumb".into())
                      ].into()),
                      Dom::div()
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-bottom".into()),
                   ].into()),

                   Dom::div()
                   // .with_class("__azul-native-scroll-content-container-1".into())
                   .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
                   .with_children(vec![
                       Dom::div() // <- this div is where the new children will be injected into
                       .with_inline_style("display:block;width:50px;height:50px;background:red;".into())
                   ].into())
               ].into())
           ].into())
       ].into())
    ].into())

    .style(Css::empty())
}

fn main() {
    let data = DataModel { counter: 0 };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);

    println!("inject scroll bars:\r\n{}", get_dom().get_html_string());
}