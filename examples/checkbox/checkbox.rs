extern crate azul;

use azul::{widgets::checkbox::{CheckBox, CheckBoxState}, prelude::*};

fn main() {
    let checkbox1 = CheckBoxState::new(false, Some("a checkbox".to_string()));
    let checkbox2 = CheckBoxState::new(false, None);
    let mut app = App::new(
        Gui {
            checkbox1,
            checkbox2,
        },
        AppConfig::default(),
    )
    .unwrap();
    let window = app
        .create_window(
            WindowCreateOptions::default(),
            css::native(),
        )
        .unwrap();
    app.run(window).unwrap();
}

struct Gui {
    checkbox1: CheckBoxState,
    checkbox2: CheckBoxState,
}

impl Layout for Gui {
    fn layout(&self, info: LayoutInfo<Self>) -> Dom<Self> {
        Dom::div()
            .with_child(Dom::label("checkbox example"))
            .with_child(CheckBox::new().dom(&self.checkbox1, self, info.window))
            .with_child(CheckBox::new().dom(&self.checkbox2, self, info.window))
    }
}
