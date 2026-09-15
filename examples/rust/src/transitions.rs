use azul::{
    dom::{IdOrClass, ZombieAnimCallback},
    image::ZombieAnimInfo,
    option::OptionF32,
    prelude::*,
    widgets::{Button, ZombieFrame},
};

struct AppState {
    sidebar_open: bool,
    screen: Screen,
    rows: Vec<&'static str>,
    shuffles: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Overview,
    Detail,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sidebar_open: true,
            screen: Screen::Overview,
            rows: vec!["Alpha", "Bravo", "Charlie", "Delta", "Echo"],
            shuffles: 0,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; background: #0e0e14; \
                    font-family: sans-serif;";
const TOOLBAR: &str = "display: flex; flex-direction: row; padding: 12px; background: #16161f; \
                       border-bottom: 1px solid #2a2a3a;";
const BTN: &str = "padding: 8px 14px; margin-right: 10px; border-radius: 6px; background: \
                   #2a2a3a; color: #e6e6f0; font-size: 14px;";
const BODY: &str = "display: flex; flex-direction: row; flex-grow: 1;";
const SIDEBAR_OPEN: &str = "width: 220px; background: #1b1b26; border-right: 1px solid #2a2a3a; \
                            padding: 16px; display: flex; flex-direction: column; \
                            -azul-animation-out: sidebarFlyOut 0.5s; -azul-animation-in: \
                            sidebarFlyIn 0.5s;";

extern "C" fn sidebar_fly_out(
    _data: &mut RefAny,
    _live: &mut TimerCallbackInfo,
    z: &ZombieAnimInfo,
) -> ZombieFrame {
    ZombieFrame {
        translate_x: -z.rect.size.width * z.timing.evaluate(z.t),
        translate_y: 0.0,
        opacity: 1.0,
        width: OptionF32::None,
        clip_to_frozen_rect: true,
    }
}

extern "C" fn sidebar_fly_in(
    _data: &mut RefAny,
    _live: &mut TimerCallbackInfo,
    z: &ZombieAnimInfo,
) -> ZombieFrame {
    ZombieFrame {
        translate_x: -z.rect.size.width * (1.0 - z.timing.evaluate(z.t)),
        translate_y: 0.0,
        opacity: 1.0,
        width: OptionF32::None,
        clip_to_frozen_rect: true,
    }
}
const CONTENT: &str = "flex-grow: 1; padding: 24px; display: flex; flex-direction: column;";
const CARD: &str = "background: #202030; border-radius: 10px; padding: 18px; margin-bottom: 16px; \
                    color: #e6e6f0; font-size: 16px;";
const ROW: &str = "background: #23233a; border-radius: 8px; padding: 12px; margin-bottom: 8px; \
                   color: #cfd2e0; font-size: 14px;";
const HINT: &str = "color: #6a7080; font-size: 12px; margin-top: 4px;";
const SIDE_ITEM: &str = "color: #9aa0b4; font-size: 13px; margin-bottom: 10px;";

extern "C" fn on_toggle_sidebar(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.sidebar_open = !s.sidebar_open;
    }
    Update::RefreshDom
}

extern "C" fn on_swap_screen(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.screen = match s.screen {
            Screen::Overview => Screen::Detail,
            Screen::Detail => Screen::Overview,
        };
    }
    Update::RefreshDom
}

extern "C" fn on_shuffle(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.rows.rotate_left(1);
        s.shuffles += 1;
    }
    Update::RefreshDom
}

fn button(label: &str, cb: extern "C" fn(RefAny, CallbackInfo) -> Update, data: &RefAny) -> Dom {
    let mut b = Button::create(label);
    b.set_on_click(data.clone(), cb);
    b.dom().with_css(BTN)
}

fn div_with_id(id: &str, css: &str) -> Dom {
    let mut nd = NodeData::create_div();
    nd.set_ids_and_classes(vec![IdOrClass::Id(id.to_string().into())]);
    Dom::create_from_data(nd).with_css(css)
}

fn shared_card(title: &str) -> Dom {
    let mut card = div_with_id("shared-card", CARD);
    card.add_child(Dom::create_div_with_text(title));
    card.add_child(Dom::create_div_with_text("same node, different screen").with_css(HINT));
    card
}

fn overview(state: &AppState) -> Dom {
    let mut content = Dom::create_div().with_css(CONTENT);
    content.add_child(shared_card("Overview"));
    for label in &state.rows {
        let mut row = div_with_id(label, ROW);
        row.add_child(Dom::create_p_with_text(*label));
        content.add_child(row);
    }
    content
}

fn detail(state: &AppState) -> Dom {
    let mut content = Dom::create_div().with_css(CONTENT);
    let mut spacer = Dom::create_div().with_css(
        "height: 90px; background: #191926; border-radius: 10px; margin-bottom: 16px; padding: \
         14px; color: #6a7080; font-size: 13px;",
    );
    spacer.add_child(Dom::create_p_with_text("Detail header"));
    content.add_child(spacer);
    content.add_child(shared_card("Detail"));
    let mut note = Dom::create_div().with_css(CARD);
    note.add_child(Dom::create_p_with_text(
        format!("Shuffles so far: {}", state.shuffles).as_str(),
    ));
    content.add_child(note);
    content
}

extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let mut d = data.clone();
    let Some(state) = d.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };

    let mut toolbar = Dom::create_div().with_css(TOOLBAR);
    toolbar.add_child(button("Toggle sidebar", on_toggle_sidebar, &data));
    toolbar.add_child(button("Swap screen", on_swap_screen, &data));
    toolbar.add_child(button("Reorder list", on_shuffle, &data));

    let mut body = Dom::create_div().with_css(BODY);
    if state.sidebar_open {
        let mut sidebar = div_with_id("sidebar", SIDEBAR_OPEN)
            .with_animation_callback(
                "sidebarFlyOut",
                ZombieAnimCallback {
                    cb: sidebar_fly_out as usize,
                },
                RefAny::new(()),
            )
            .with_animation_callback(
                "sidebarFlyIn",
                ZombieAnimCallback {
                    cb: sidebar_fly_in as usize,
                },
                RefAny::new(()),
            );
        for item in ["Inbox", "Drafts", "Archive", "Trash"] {
            sidebar.add_child(Dom::create_div_with_text(item).with_css(SIDE_ITEM));
        }
        body.add_child(sidebar);
    }
    body.add_child(match state.screen {
        Screen::Overview => overview(&state),
        Screen::Detail => detail(&state),
    });

    let mut root = Dom::create_div().with_css(ROOT);
    root.add_child(toolbar);
    root.add_child(body);

    Dom::create_body().with_child(root)
}

fn main() {
    let data = RefAny::new(AppState::default());
    let app = App::create(data, AppConfig::create());
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
