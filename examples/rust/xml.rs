#![windows_subsystem = "windows"]

use azul::prelude::*;

struct Data { }

extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_file("./ui.xml".into())
    .with_menu_bar(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Application".into()).with_children(vec![
            MenuItem::String(StringMenuItem::new("Select File...".into()).with_callback(data.clone(), on_menu_click))
        ].into()))
    ].into()))
    .with_context_menu(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Application".into()).with_children(vec![
            MenuItem::String(StringMenuItem::new("Select File...".into()).with_callback(data.clone(), on_context_clicked))
        ].into()))
    ].into()))
}

extern "C" fn on_menu_click(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    println!("menu item clicked!");
    Update::RefreshDom
}

extern "C" fn on_context_clicked(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    println!("reload clicked!");
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(Data { });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(layout);
    window.hot_reload = true;
    window.state.flags.frame = WindowFrame::Maximized;
    app.run(window);
}
