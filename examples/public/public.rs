//! Example of the new, public API

use azul::prelude::*;
use azul::style::StyledDom;
use azul::css::{CssProperty, StyleTextColor, ColorU};
use azul::callbacks::{
    UpdateScreen, TimerCallbackInfo, CallbackInfo,
    TimerCallbackReturn, CallbackReturn, Callback,
};
use azul::task::{TimerId, Timer, TerminateTimer};
use azul::time::Duration;

#[derive(Debug)]
struct Data {
    counter: usize,
}

extern "C" fn layout(data: &RefAny, _info: LayoutInfo) -> StyledDom {
    let data = data.downcast_ref::<Data>().unwrap();
    Dom::body().with_child(
        Dom::label(format!("hello: {}", data.counter).into())
        .with_inline_css(CssProperty::text_color(StyleTextColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))
    ).style(Css::empty())
}

#[derive(Debug)]
struct TimerData {
    flip: bool,
}

extern "C" fn resize_window_timer(_app_data: &mut RefAny, timer_data: &mut RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {

    let mut data = timer_data.downcast_mut::<TimerData>().unwrap();
    let mut new_window_state = info.callback_info.get_window_state();

    println!("timer run: {:?}", info.call_count);

    if data.flip {
        new_window_state.size.dimensions.width += 50.0;
    } else {
        new_window_state.size.dimensions.width -= 50.0;
    }

    info.callback_info.set_window_state(new_window_state);

    data.flip = !data.flip;

    TimerCallbackReturn {
        should_update: UpdateScreen::DoNothing,
        should_terminate: TerminateTimer::Continue,
    }
}

extern "C" fn start_timer(_app_data: &mut RefAny, mut info: CallbackInfo) -> CallbackReturn {

    let timer = Timer::new(RefAny::new(TimerData { flip: false }), resize_window_timer);

    info.start_timer(TimerId::unique(), timer);

    UpdateScreen::DoNothing
}

fn main() {

    let data = Data { counter: 5 };
    let app = App::new(RefAny::new(data), AppConfig::default());

    let mut create_options = WindowCreateOptions::new(layout);
    create_options.create_callback = Some(Callback { cb: start_timer }).into();
    create_options.state.background_color = ColorU { r: 255, g: 0, b: 0, a: 255 };

    app.run(create_options);
}