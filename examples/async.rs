#![windows_subsystem = "windows"]

extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, label::Label},
};
use std::{
    thread,
    time::{Duration, Instant},
    sync::{Arc, Mutex},
};

#[derive(Debug, PartialEq)]
enum ConnectionStatus {
    NotConnected,
    Connected,
    Error(String),
    InProgress(Instant, Duration),
}

struct MyDataModel {
    connection_status: ConnectionStatus,
}

impl Layout for MyDataModel {
    fn layout(&self, _info: WindowInfo<Self>) -> Dom<Self> {

        use self::ConnectionStatus::*;

        let status = match &self.connection_status {
            ConnectionStatus::NotConnected       => format!("Not connected!"),
            ConnectionStatus::Connected          => format!("You are connected!"),
            ConnectionStatus::InProgress(_, d)   => format!("Loading... {}.{:02}s", d.as_secs(), d.subsec_millis()),
            ConnectionStatus::Error(e)           => format!("There was an error: {}", e),
        };

        let mut dom = Dom::new(NodeType::Div)
            .with_child(Label::new(status.clone()).dom());

        match &self.connection_status {
            NotConnected => {
                let button = Button::with_label("Connect to database...").dom()
                                .with_callback(On::MouseUp, Callback(start_connection));

                dom.add_child(button);
            },
            Error(_) | Connected => {
                let button = Button::with_label(format!("{}\nRetry?", status)).dom()
                                .with_callback(On::MouseUp, Callback(reset_connection));
                dom.add_child(button);
            }
            InProgress(_, _) => { },
        }

        dom
    }
}

fn reset_connection(app_state: &mut AppState<MyDataModel>, _event: WindowEvent<MyDataModel>) -> UpdateScreen {
    app_state.data.modify(|state| state.connection_status = ConnectionStatus::NotConnected);
    UpdateScreen::Redraw
}

fn start_connection(app_state: &mut AppState<MyDataModel>, _event: WindowEvent<MyDataModel>) -> UpdateScreen {
    let status = ConnectionStatus::InProgress(Instant::now(), Duration::from_secs(0));
    app_state.data.modify(|state| state.connection_status = status);
    app_state.add_task(connect_to_db_async, &[]);
    app_state.add_daemon(Daemon::unique(DaemonCallback(timer_daemon)));
    UpdateScreen::Redraw
}

fn timer_daemon(state: &mut MyDataModel, _resources: &mut AppResources) -> (UpdateScreen, TerminateDaemon) {
    if let ConnectionStatus::InProgress(start, duration) = &mut state.connection_status {
        *duration = Instant::now() - *start;
        (UpdateScreen::Redraw, TerminateDaemon::Continue)
    } else {
        (UpdateScreen::DontRedraw, TerminateDaemon::Terminate)
    }
}

fn connect_to_db_async(app_data: Arc<Mutex<MyDataModel>>, _: Arc<()>) {
    thread::sleep(Duration::from_secs(10)); // simulate slow load
    app_data.modify(|state| state.connection_status = ConnectionStatus::Connected);
}

fn main() {
    let model = MyDataModel { connection_status: ConnectionStatus::NotConnected };
    let app = App::new(model, AppConfig::default());
    app.run(Window::new(WindowCreateOptions::default(), Css::native()).unwrap()).unwrap();
}