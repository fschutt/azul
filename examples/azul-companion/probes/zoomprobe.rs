use azul::prelude::*;

fn case(n: &str, flex_parent: bool, flex_zoomed: bool, flex_inner: bool) -> Dom {
    let fl = |on: bool, dir: &str| if on { format!("display: flex; flex-direction: {dir};") } else { String::new() };
    let c = Dom::create_div().with_id(format!("{n}-c").as_str()).with_css("width: 100px; height: 40px; background: #f00; flex-shrink: 0;");
    let b = Dom::create_div()
        .with_id(format!("{n}-b").as_str())
        .with_css(format!("{} position: relative; overflow: hidden; width: 200px; height: 100px; padding: 20px 0px 10px 0px; background: #0f0; flex-shrink: 0;", fl(flex_inner, "column")).as_str())
        .with_child(c);
    let a = Dom::create_div()
        .with_id(format!("{n}-a").as_str())
        .with_css(format!("{} zoom: 50%; width: 400px; height: 300px; background: #00f; flex-shrink: 0;", fl(flex_zoomed, "column")).as_str())
        .with_child(b);
    Dom::create_div()
        .with_id(format!("{n}-p").as_str())
        .with_css(format!("{} width: 600px; height: 320px; background: #333;", fl(flex_parent, "row")).as_str())
        .with_child(a)
}

extern "C" fn layout(_d: RefAny, _i: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(case("k1", false, false, false))
        .with_child(case("k2", false, true, false))
        .with_child(case("k3", false, false, true))
        .with_child(case("k4", true, false, false))
        .with_child(case("k5", true, true, true))
}

fn main() {
    let app = App::create(RefAny::new(()), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(700.0, 1700.0);
    window.window_state.size.dpi = 192;
    app.run(window);
}
