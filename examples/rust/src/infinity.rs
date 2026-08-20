use azul::prelude::*;
use azul::option::OptionDom;

#[derive(Default)]
struct InfinityState {
    file_paths: Vec<std::string::String>,
    visible_start: usize,
    visible_count: usize,
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let file_count = {
        let d = match data.downcast_ref::<InfinityState>() {
            Some(s) => s,
            None => return Dom::create_body(),
        };
        d.file_paths.len()
    };

    let title = Dom::create_div_with_text(format!("Pictures - {} images", file_count))
        .with_css("font-size: 20px; margin-bottom: 10px;");

    let vview = Dom::create_virtual_view(data.clone(), render_virtual_view)
        .with_css("flex-grow: 1; overflow: scroll; background: #f5f5f5;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            data.clone(),
            on_scroll,
        );

    Dom::create_body()
        .with_css("padding: 20px; font-family: sans-serif;")
        .with_child(title)
        .with_child(vview)
}

extern "C" fn render_virtual_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let d = match data.downcast_ref::<InfinityState>() {
        Some(s) => s,
        None => return VirtualViewReturn::default(),
    };

    let mut container = Dom::create_div()
        .with_css("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;");

    let end = (d.visible_start + d.visible_count).min(d.file_paths.len());
    for i in d.visible_start..end {
        let item = Dom::create_div()
            .with_css(
                "
                width: 150px; 
                height: 150px; 
                background: white; 
                border: 1px solid #ddd; 
                display: flex; 
                align-items: center; 
                justify-content: center;
            ",
            )
            .with_child(
                Dom::create_div_with_text(d.file_paths[i].clone())
                    .with_css("font-size: 10px; text-align: center;"),
            );

        container.add_child(item);
    }

    let rows = (d.file_paths.len() + 3) / 4;
    let virtual_height = rows as f32 * 160.0;

    VirtualViewReturn {
        dom: OptionDom::Some(container),
        materialized: LogicalRect::create(
            LogicalPosition::create(0.0, 0.0),
            LogicalSize::create(0.0, virtual_height),
        ),
        virtual_rect: LogicalRect::create(
            LogicalPosition::create(0.0, d.visible_start as f32 * 40.0),
            LogicalSize::create(0.0, virtual_height),
        ),
    }
}

extern "C" fn on_scroll(mut data: RefAny, info: CallbackInfo) -> Update {
    let scroll_pos = match info.get_scroll_offset() {
        OptionLogicalPosition::Some(pos) => pos,
        OptionLogicalPosition::None => return Update::DoNothing,
    };

    let mut d = match data.downcast_mut::<InfinityState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let items_per_row = 4;
    let item_height = 160.0;
    let new_start = ((scroll_pos.y / item_height) as usize) * items_per_row;

    if new_start != d.visible_start {
        d.visible_start = new_start.min(d.file_paths.len().saturating_sub(1));
        return Update::RefreshDom;
    }

    Update::DoNothing
}

fn main() {
    let mut state = InfinityState {
        file_paths: Vec::new(),
        visible_start: 0,
        visible_count: 20,
    };

    for i in 0..1000 {
        state.file_paths.push(format!("image_{:04}.png", i));
    }

    let data = RefAny::new(state);
    let app = App::create(data, AppConfig::create());
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
