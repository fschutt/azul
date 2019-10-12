#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use azul::widgets::button::Button;

struct MyDataModel { }

impl Layout for MyDataModel {
    fn layout(&self, _: LayoutInfo) -> Dom<Self> {
        Button::with_label("Update counter").dom()
    }
}

fn main() {
    let app = App::new(MyDataModel { }, AppConfig::default()).unwrap();
    app.run(WindowCreateOptions::new(css::native()));
}