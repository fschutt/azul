use azul::prelude::*;
use azul::css::StyledDom;

struct DataModel;

extern "C" 
fn my_layout_func(mut _data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    // Create a container with overflow: auto and many block items
    let mut container = Dom::create_div();
    container.set_inline_style(
        "height: 300px; width: 400px; overflow: auto; \
         background-color: #ffff00; border: 4px solid #ff00ff; margin: 20px;"
    );

    // Add 10 block items (simple colored rectangles) that will overflow the container
    for i in 1..=10 {
        let mut item = Dom::create_div();
        let color = if i % 2 == 0 { "#ff0000" } else { "#00ff00" };
        let style = format!(
            "height: 80px; margin: 10px; background-color: {};",
            color
        );
        item.set_inline_style(style.as_str());
        container.add_child(item);
    }

    let mut body = Dom::create_body();
    body.set_inline_style(
        "display: flex; flex-direction: column; height: 100%; \
         background-color: #00ffff; margin: 0; padding: 10px;"
    );

    body.add_child(container);

    body.style(Css::empty())
}

fn main() {
    let data = DataModel;
    let config = AppConfig::create();
    let app = App::create(RefAny::new(data), config);
    let window = WindowCreateOptions::create(my_layout_func);
    app.run(window);
}
