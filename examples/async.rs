extern crate azul;

use azul::{prelude::*, widgets::*};
use std::{thread, time::Duration, sync::{Arc, Mutex}};
use self::ConnectionStatus::*;

#[derive(Debug, PartialEq)]
enum ConnectionStatus {
    NotConnected,
    Connected,
    Error(String),
    InProgress,
}

struct MyDataModel {
    connection_status: ConnectionStatus,
}

impl Layout for MyDataModel {
    fn layout(&self, info: WindowInfo) -> Dom<Self> {

        let status = match &self.connection_status {
            NotConnected => format!("Not connected!"),
            Connected    => format!("You are connected!"),
            InProgress   => format!("Loading..."),
            Error(e)     => format!("There was an error: {}", e),
        };

        let status_p = Label::new(status).dom();

        let mut dom = Dom::new(NodeType::Div).with_child(status_p);

        if self.connection_status == NotConnected {
            dom.add_child(Button::with_label("Connect to database...").dom()
                          .with_callback(On::MouseUp, Callback(start_connection)));
        }

        dom
    }
}

fn start_connection(app_state: &mut AppState<MyDataModel>, event: WindowEvent) -> UpdateScreen {
    app_state.data.modify(|state| state.connection_status = ConnectionStatus::InProgress);
    app_state.add_task(connect_to_db_async);
    UpdateScreen::Redraw
}

fn connect_to_db_async(app_data: Arc<Mutex<MyDataModel>>, _: Arc<()>) {
    thread::sleep(Duration::from_secs(4)); // simulate slow load
    app_data.modify(|state| state.connection_status = ConnectionStatus::Connected);
}

fn main() {
    let model = MyDataModel { connection_status: ConnectionStatus::NotConnected };
    let mut app = App::new(model, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}