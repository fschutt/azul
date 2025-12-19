#![windows_subsystem = "windows"]

use std::{
    string::String,
    time::{Duration, Instant},
};

use azul::{prelude::*, widgets::*};

use self::{BackgroundThreadReturn::*, ConnectionStatus::*};

#[derive(Default)]
struct MyDataModel {
    connection_status: ConnectionStatus,
}

#[derive(Debug)]
enum ConnectionStatus {
    NotConnected {
        // which database to connect to
        // ex. "user@localhost:5432"
        database: String,
    },
    InProgress {
        // handle to the background thread
        background_thread_id: ThreadId,
        // time when the thread was started
        start_time: Instant,
        // estimated time to completion
        estimated_wait: Duration,
        // data that has been loaded so far
        data_in_progress: Vec<usize>,
        // stage of the connection
        stage: ConnectionStage,
    },
    DataLoaded {
        // the established connection
        data: Vec<usize>,
    },
    Error {
        // error establishing a connection
        error: String,
    },
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::NotConnected {
            database: format!("database@localhost:1234"),
        }
    }
}

// Main function that renders the UI
extern "C" 
fn render_ui(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    use self::ConnectionStatus::*;

    let mut body = Dom::create_body()
    .with_inline_style(
        "font-family: sans-serif;
        align-items: center;
        justify-content: center;
        flex-direction: row;"
    );

    let data_clone = data.clone();
    let downcasted = match data.downcast_ref::<MyDataModel>() {
        Some(f) => f,
        None => return body.style(Css::empty()), // error
    };

    body.add_child(
        Dom::create_div()
            .with_inline_style(
                "flex-direction: column; 
                align-items: center; 
                justify-content: center;",
            )
            .with_children(vec![
                downcasted.connection_status.dom()
                .with_inline_style(
                    "max-width: 350px; display:block;"
                )
            ]),
    );

    body.style(Css::empty())
}

impl ConnectionStatus {
    pub fn dom(&self) -> Dom {
        match self {
            NotConnected { database } => Dom::create_div().with_children(vec![
                Dom::create_text("Enter database to connect to:"),
                TextInput::new()
                    .with_text(database.clone())
                    .with_on_text_input(data_clone.clone(), edit_database_input)
                    .dom(),
                Button::new("Connect")
                    .with_on_click(data_clone.clone(), start_background_thread)
                    .dom(),
            ]),
            InProgress {
                stage,

                data_in_progress,
                ..
            } => {
                use self::ConnectionStage::*;

                let progress_div = match stage {
                    EstablishingConnection => Dom::create_text("Establishing connection..."),
                    ConnectionEstablished => {
                        Dom::create_text("Connection established! Waiting for data...")
                    }
                    LoadingData { percent_done } => Dom::create_div().with_children(vec![
                        Dom::create_text("Loading data..."),
                        ProgressBar::new(*percent_done).dom(),
                    ]),
                    LoadingFinished => Dom::create_text("Loading finished!"),
                };

                let data_rendered_div = data_in_progress
                    .chunks(10)
                    .map(|chunk| Dom::create_text(format!("{:?}", chunk)))
                    .collect::<Dom>();

                let stop_btn = Button::new("Stop thread")
                    .with_on_click(data_clone.clone(), stop_background_thread)
                    .dom();

                Dom::create_div().with_children(vec![progress_div, data_rendered_div, stop_btn])
            }
            DataLoaded { data: data_loaded } => {
                let data_rendered_div = data_loaded
                    .chunks(10)
                    .map(|chunk| Dom::create_text(format!("{:?}", chunk)))
                    .collect::<Dom>();

                let reset_btn = Button::new("Reset")
                    .with_on_click(data_clone.clone(), reset)
                    .dom();

                Dom::create_div().with_children(vec![data_rendered_div, reset_btn])
            }
            Error { error } => {
                let error_div = Dom::create_text(format!("{}", error));

                let reset_btn = Button::new("Reset")
                    .with_on_click(data_clone.clone(), reset)
                    .dom();

                Dom::create_div().with_children(vec![error_div, reset_btn])
            }
        }
    }
}

#[derive(Debug, Clone)]
enum ConnectionStage {
    EstablishingConnection,
    ConnectionEstablished,
    LoadingData { percent_done: f32 },
    LoadingFinished,
}

// Runs when "connect to database" button is clicked
extern "C" 
fn edit_database_input(
    mut data: RefAny,
    event: CallbackInfo,
    textinputstate: &TextInputState,
) -> OnTextInputReturn {
    let ret = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };

    let mut data_mut = match data.downcast_mut::<MyDataModel>() {
        Some(s) => s,
        None => return ret, // error
    };

    match &mut data_mut.connection_status {
        NotConnected { database } => {
            *database = textinputstate.get_text().as_str().into();
        }
        _ => return ret,
    }

    ret
}

extern "C" 
fn start_background_thread(mut data: RefAny, mut event: CallbackInfo) -> Update {
    // Copy the string of what database to connect to and
    // use it to initialize a new background thread
    let data_clone = data.clone();
    let mut data_mut = match data.downcast_mut::<MyDataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    let database_to_connect_to = match &data_mut.connection_status {
        NotConnected { database } => database.clone(),
        _ => return Update::DoNothing, // error
    };

    let init_data = RefAny::new(BackgroundThreadInit {
        database: database_to_connect_to,
    });

    let thread_id = match event
        .start_thread(init_data, data_clone.clone(), background_thread)
        .into_option()
    {
        Some(s) => s,
        None => return Update::DoNothing, // thread creation failed
    };

    data_mut.connection_status = InProgress {
        background_thread_id: thread_id,
        start_time: Instant::now(),
        estimated_wait: Duration::from_secs(10),
        stage: ConnectionStage::EstablishingConnection,
        data_in_progress: Vec::new(),
    };

    // Update the UI
    Update::RefreshDom
}

// Runs when "cancel" button is clicked while background thread is running
extern "C" 
fn stop_background_thread(mut data: RefAny, mut event: CallbackInfo) -> Update {
    let mut data_mut = match data.downcast_mut::<MyDataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let thread_id = match data_mut.connection_status {
        InProgress {
            background_thread_id,
            ..
        } => background_thread_id.clone(),
        _ => return Update::DoNothing, // error
    };

    event.stop_thread(thread_id);

    data_mut.connection_status = ConnectionStatus::default();

    Update::RefreshDom
}

// Runs when "reset" is clicked (resets the data)
extern "C"
fn reset(mut data: RefAny, event: CallbackInfo) -> Update {
    let mut data_mut = match data.downcast_mut::<MyDataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    match data_mut.connection_status {
        DataLoaded { .. } => {}
        _ => return Update::DoNothing, // error
    };

    data_mut.connection_status = ConnectionStatus::default();

    Update::RefreshDom
}

// Data sent from the main to the background thread
#[derive(Debug)]
struct BackgroundThreadInit {
    database: String,
}

// Data returned from the background thread
#[derive(Debug)]
enum BackgroundThreadReturn {
    StatusUpdated { new: ConnectionStage },
    ErrorOccurred { error: String },
    NewDataLoaded { data: Vec<usize> },
}

// Callback that "writes data back" from the background thread to the main thread
// 
// This function runs on the main thread, so that there can't be any data races
extern "C" 
fn writeback_callback(
    mut app_data: RefAny,
    mut incoming_data: RefAny,
    _: CallbackInfo,
) -> Update {
    use crate::BackgroundThreadReturn::*;

    let mut data_mut = match app_data.downcast_mut::<MyDataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut incoming_data = match incoming_data.downcast_mut::<BackgroundThreadReturn>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    match &mut *incoming_data {
        StatusUpdated { new } => match &mut data_mut.connection_status {
            InProgress { stage, .. } => {
                *stage = new.clone();
                Update::RefreshDom
            }
            _ => Update::DoNothing,
        },
        ErrorOccurred { error } => {
            data_mut.connection_status = Error {
                error: error.clone(),
            };
            Update::RefreshDom
        }
        NewDataLoaded { data } => match &mut data_mut.connection_status {
            InProgress {
                data_in_progress, ..
            } => {
                data_in_progress.append(data);
                Update::RefreshDom
            }
            _ => Update::DoNothing,
        },
    }
}

// Function that executes in a non-main thread (main "background thread" logic)
extern "C" 
fn background_thread(
    mut initial_data: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let initial_data = match initial_data.downcast_ref::<BackgroundThreadInit>() {
        Some(s) => s,
        None => return, // error
    };

    // connect to the database (blocking)
    let connection = match postgres::establish_connection(&initial_data.database) {
        Ok(db) => db,
        Err(e) => {
            sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
                data: RefAny::new(ErrorOccurred { error: e }),
                callback: WriteBackCallback {
                    cb: writeback_callback,
                },
            }));
            return;
        }
    };

    // if in the meantime we got a "cancel" message, quit the thread
    if recv.receive() == Some(ThreadSendMsg::TerminateThread).into() {
        return;
    }

    // update the UI again to notify that the connection has been established
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        data: RefAny::new(StatusUpdated {
            new: ConnectionStage::ConnectionEstablished,
        }),
        callback: WriteBackCallback {
            cb: writeback_callback,
        },
    }));

    let query = "SELECT * FROM large_table;";
    let total_items = postgres::estimate_item_count(&connection, query);
    let mut items_loaded = 0;

    for row in postgres::query_rows(&connection, query) {
        // If in the meantime we got a "cancel" message, quit the thread
        if recv.receive() == Some(ThreadSendMsg::TerminateThread).into() {
            return;
        } else {
            items_loaded += row.len();

            // As soon as each row is loaded, update the UI
            sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
                data: RefAny::new(NewDataLoaded { data: row.to_vec() }),
                callback: WriteBackCallback {
                    cb: writeback_callback,
                },
            }));

            let percent_done = (items_loaded as f32 / total_items as f32) * 100.0;
            // Calculate and update the percentage count
            sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
                data: RefAny::new(StatusUpdated {
                    new: ConnectionStage::LoadingData {
                        percent_done,
                    },
                }),
                callback: WriteBackCallback {
                    cb: writeback_callback,
                },
            }));
        }
    }

    println!("all rows sent!");

    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        data: RefAny::new(StatusUpdated {
            new: ConnectionStage::LoadingFinished,
        }),
        callback: WriteBackCallback {
            cb: writeback_callback,
        },
    }));
}

// mock module to simulate a database
mod postgres {

    use std::time::Duration;

    // Mock database connection
    pub(super) struct Database {}

    type Row = [usize; 10];

    static LARGE_TABLE: &'static [Row] = &[
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    ];

    // each of these functions blocks to simulate latency on a real database
    pub(super) fn establish_connection(database: &str) -> Result<Database, String> {
        std::thread::sleep(Duration::from_secs(1));
        Ok(Database {})
    }

    pub(super) fn estimate_item_count(db: &Database, _query: &str) -> usize {
        LARGE_TABLE.len() * LARGE_TABLE[0].len()
    }

    pub(super) fn query_rows(db: &Database, _query: &str) -> impl Iterator<Item = &'static Row> {
        LARGE_TABLE.iter().map(|i| {
            // let's simulate that each row / query takes one second to load in
            std::thread::sleep(Duration::from_secs(1));
            i
        })
    }
}

fn main() {
    let app = App::new(
        RefAny::new(MyDataModel::default()),
        AppConfig::new(),
    );
    app.run(WindowCreateOptions::new(render_ui));
}
