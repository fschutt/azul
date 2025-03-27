use azul::{
    prelude::*,
    widgets::{Ribbon, RibbonOnTabClickedCallback},
};

struct DataModel {
    current_active_tab: i32,
}

extern "C" fn myLayoutFunc(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    println!("myLayoutFunc!");

    let data_clone = data.clone();
    let data = match data.downcast_ref::<DataModel>() {
        Some(s) => s,
        None => return StyledDom::default(),
    };

    let mut ribbon = Ribbon {
        tab_active: data.current_active_tab,
    };

    Dom::body()
        .with_child(ribbon.dom(RibbonOnTabClickedCallback { cb: update_tab }, data_clone))
        .style(Css::empty())
}

extern "C" fn update_tab(data: &mut RefAny, info: &mut CallbackInfo, new_tab: i32) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("BINARY: clicked on tab {}", new_tab);

    data.current_active_tab = new_tab;

    Update::RefreshDom
}

fn main() {
    let data = DataModel {
        current_active_tab: 3,
    };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    let window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);
}
