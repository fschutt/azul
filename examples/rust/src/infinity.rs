use azul::{prelude::*, widgets::*};

#[derive(Default)]
struct InfinityState {
    file_paths: Vec<String>,
    visible_start: usize,
    visible_count: usize,
}

extern "C" 
fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let d = match data.downcast_ref::<InfinityState>() {
        Some(s) => s,
        None => return StyledDom::default(),
    };
    
    let title = Dom::text(format!("Pictures - {} images", d.file_paths.len()))
        .with_inline_style("font-size: 20px; margin-bottom: 10px;");
    
    let iframe = Dom::iframe(data.clone(), render_iframe)
        .with_inline_style("flex-grow: 1; overflow: scroll; background: #f5f5f5;")
        .with_callback(On::Scroll.into_event_filter(), data.clone(), on_scroll);
    
    Dom::new_body()
        .with_inline_style(
            "padding: 20px; font-family: sans-serif;"
        )
        .with_child(title)
        .with_child(iframe)
        .style(Css::empty())
}

extern "C" 
fn render_iframe(mut data: RefAny, info: IFrameCallbackInfo) -> IFrameCallbackReturn {
    let d = match data.downcast_ref::<InfinityState>() {
        Some(s) => s,
        None => return IFrameCallbackReturn::default(),
    };
    
    let mut container = Dom::new_div()
        .with_inline_style("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;");
    
    let end = (d.visible_start + d.visible_count).min(d.file_paths.len());
    for i in d.visible_start..end {

        let item = Dom::new_div()
            .with_inline_style("
                width: 150px; 
                height: 150px; 
                background: white; 
                border: 1px solid #ddd; 
                display: flex; 
                align-items: center; 
                justify-content: center;
            ")
            .with_child(
                Dom::text(&d.file_paths[i])
                .with_inline_style("font-size: 10px; text-align: center;")
            );

        container.add_child(item);
    }
    
    // Calculate virtual scroll height based on total items
    let rows = (d.file_paths.len() + 3) / 4; // 4 items per row
    let virtual_height = rows as f32 * 160.0; // 150px + 10px gap
    
    IFrameCallbackReturn {
        dom: container.style(Css::empty()),
        scroll_size: LogicalSize::new(0.0, virtual_height),
        scroll_offset: LogicalPosition::new(0.0, 0.0),
        virtual_scroll_size: LogicalSize::new(0.0, virtual_height),
        virtual_scroll_offset: LogicalPosition::new(0.0, d.visible_start as f32 * 40.0),
    }
}

/// Handle scroll events to update visible items
extern "C" 
fn on_scroll(mut data: RefAny, info: CallbackInfo) -> Update {
    let scroll_pos = match info.get_scroll_position() {
        Some(pos) => pos,
        None => return Update::DoNothing,
    };
    
    let mut d = match data.downcast_mut::<InfinityState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    
    // Calculate which items should be visible based on scroll position
    let items_per_row = 4;
    let item_height = 160.0; // 150px + 10px gap
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
    
    // Generate dummy file names
    for i in 0..1000 {
        state.file_paths.push(format!("image_{:04}.png", i));
    }
    
    let data = RefAny::new(state);
    let config = AppConfig::new();
    let app = App::new(data.clone(), config);
    
    let mut window = WindowCreateOptions::new(layout);
    window.state.title = "Infinite Scrolling Gallery".into();
    window.state.size.dimensions.width = 800.0;
    window.state.size.dimensions.height = 600.0;
    
    app.run(window);
}
