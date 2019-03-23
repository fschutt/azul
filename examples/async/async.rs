#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    InProgress(Instant, Duration),
}

struct MyDataModel {
    connection_status: ConnectionStatus,
}

impl Layout for MyDataModel {

    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {

        use self::ConnectionStatus::*;

        println!("layout!");

        let status = match &self.connection_status {
            ConnectionStatus::NotConnected       => format!("Not connected!"),
            ConnectionStatus::Connected          => format!("You are connected!"),
            ConnectionStatus::InProgress(_, d)   => format!("Loading... {}.{:02}s", d.as_secs(), d.subsec_millis()),
        };

        let mut dom = Dom::div()
            .with_child(Label::new(status.clone()).dom());

        match &self.connection_status {
            NotConnected => {
                dom.add_child(
                    Button::with_label("Connect to database...").dom()
                    .with_callback(On::MouseUp, Callback(start_connection))
                );
            },
            Connected => {
                dom.add_child(
                    Button::with_label(format!("{}\nRetry?", status)).dom()
                    .with_callback(On::MouseUp, Callback(reset_connection))
                );
            }
            InProgress(_, _) => { },
        }

        dom
    }
}

fn reset_connection(app_state: &mut AppState<MyDataModel>, _event: &mut CallbackInfo<MyDataModel>) -> UpdateScreen {
    app_state.data.modify(|state| state.connection_status = ConnectionStatus::NotConnected)?;
    Redraw
}

fn start_connection(app_state: &mut AppState<MyDataModel>, _event: &mut CallbackInfo<MyDataModel>) -> UpdateScreen {
    let status = ConnectionStatus::InProgress(Instant::now(), Duration::from_secs(0));
    app_state.data.modify(|state| state.connection_status = status)?;
    let task = Task::new(&app_state.data, connect_to_db_async);
    app_state.add_task(task);
    app_state.add_timer(TimerId::new(), Timer::new(timer_timer));
    Redraw
}

fn timer_timer(state: &mut MyDataModel, _resources: &mut AppResources) -> (UpdateScreen, TerminateTimer) {
    if let ConnectionStatus::InProgress(start, duration) = &mut state.connection_status {
        *duration = Instant::now() - *start;
        (Redraw, TerminateTimer::Continue)
    } else {
        (DontRedraw, TerminateTimer::Terminate)
    }
}

fn connect_to_db_async(app_data: Arc<Mutex<MyDataModel>>, _: DropCheck) {
    thread::sleep(Duration::from_secs(10)); // simulate slow load
    app_data.modify(|state| state.connection_status = ConnectionStatus::Connected);
}

fn main() {
    let model = MyDataModel { connection_status: ConnectionStatus::NotConnected };
    let mut app = App::new(model, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}
