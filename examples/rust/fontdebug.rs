use azul::prelude::*;

struct DataModel {
    counter: usize,
}

extern "C" fn myLayoutFunc(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    get_dom()

    /*Dom::body().with_child(
        Dom::text("Test")
        .with_inline_style("font-size: 50px;")
    ).style(Css::empty())*/
}

fn get_dom() -> StyledDom {
    Dom::body()
    .with_inline_style("display:flex; flex-grow:1; flex-direction:column;")
    .with_children(vec![
       Dom::div()
       // .with_class("__azul-native-scroll-root-component")
       .with_inline_style("display:flex; flex-grow:1; flex-direction:column;")
       .with_children(vec![

           Dom::div()
           // .with_class("__azul-native-scroll-vertical-container")
           .with_inline_style("display:flex; flex-grow:1; flex-direction:column-reverse;")
           .with_children(vec![

               Dom::div()
               // .with_class("__azul-native-scroll-horizontal-scrollbar")
               .with_inline_style("display:flex; flex-grow:1; flex-direction:row; height:15px; max-height:15px; background:grey;")
               .with_children(vec![
                   Dom::div(),
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-left"),
                   Dom::div()
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-middle")
                   .with_children(vec![
                       Dom::div()
                       // .with_class("__azul-native-scroll-horizontal-scrollbar-track-thumb")
                   ]),
                   Dom::div()
                   // .with_class("__azul-native-scroll-horizontal-scrollbar-track-right"),
               ]),

               Dom::div()
               // .with_class("__azul-native-scroll-content-container-1")
               .with_inline_style("display:flex; flex-grow:1; flex-direction:row-reverse;")
               .with_children(vec![

                   Dom::div()
                   // .with_class("__azul-native-scroll-vertical-scrollbar")
                   .with_inline_style("display:flex; flex-grow:1; flex-direction:column; width:15px; max-width:15px; background:grey;")
                   .with_children(vec![
                      Dom::div(),
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-top"),
                      Dom::div()
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-middle")
                      .with_children(vec![
                          Dom::div()
                          // .with_class("__azul-native-scroll-vertical-scrollbar-track-thumb")
                      ]),
                      Dom::div()
                      // .with_class("__azul-native-scroll-vertical-scrollbar-track-bottom"),
                   ]),

                   Dom::div()
                   // .with_class("__azul-native-scroll-content-container-1")
                   .with_inline_style("display:flex; flex-grow:1; flex-direction:column;")
                   .with_children(vec![
                       Dom::div() // <- this div is where the new children will be injected into
                       .with_inline_style("display:block;width:50px;height:50px;background:red;")
                   ])
               ])
           ])
       ])
    ])

    .style(Css::empty())
}

fn main() {
    let data = DataModel { counter: 0 };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    let window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);

    println!("inject scroll bars:\r\n{}", get_dom().get_html_string());
}
