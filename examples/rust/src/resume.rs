//! The request / resume pattern, one button per request kind.
//!
//! Every OS-facing operation that a browser can only answer asynchronously
//! is a *request*: the click callback issues it, returns, and the answer
//! arrives later in a *resume* callback as a fresh activation. Desktop
//! answers within the same frame, mobile when the OS delegate fires, web on
//! a later task - the code is the same.
//!
//! This is also the e2e target for the resumable API: the specs in
//! `tests/e2e/resume_*.json` drive it with `AZ_BACKEND=headless` and the
//! `mock` op (a canned picker answer, a canned HTTP response, ...), then
//! assert on the labels below.

use azul::db::{Db, DbConfig, DbOpenResult, DbSyncStatusResult, DbValue, DbValueResult};
use azul::callbacks::CallbackType;
use azul::dialog::{ColorPickResult, ColorPickerDialog, FileDialog, FileOpenResult};
use azul::dom::IdOrClass;
use azul::error::ResultDbDbError;
use azul::file::FileReadBytesResult;
use azul::http::{HttpGetResult, HttpRequestConfig};
use azul::option::{OptionColorU, OptionDbScope, OptionFileTypeList, OptionString};
use azul::prelude::*;
use azul::vec::IdOrClassVec;
use azul::widgets::Button;

/// The endpoints the demo talks to. Under e2e they are mocked; in a real
/// run they simply fail (the domain is reserved) and the labels show it.
const FETCH_URL: &str = "https://example.invalid/e2e/status";
const SYNC_URL: &str = "https://sync.example.invalid/demo";

struct Demo {
    file: String,
    http: String,
    color: String,
    export: String,
    db_value: String,
    sync: String,
    picked_path: String,
    db: Option<Db>,
}

impl Demo {
    fn new() -> Self {
        Self {
            file: "file: -".to_string(),
            http: "http: -".to_string(),
            color: "colour: -".to_string(),
            export: "exported: no".to_string(),
            db_value: "db: -".to_string(),
            sync: "sync: -".to_string(),
            picked_path: String::new(),
            db: None,
        }
    }
}

fn label(id: &str, text: &str) -> Dom {
    Dom::create_div()
        .with_css("font-size: 16px; margin: 4px 0px;")
        .with_ids_and_classes(IdOrClassVec::from(vec![IdOrClass::Id(id.into())]))
        .with_child(Dom::create_span_with_text(text))
}

fn button(text: &str, data: &RefAny, cb: CallbackType) -> Dom {
    let mut b = Button::create(text);
    b.set_on_click(data.clone(), cb);
    let mut dom = b.dom();
    dom.set_css("margin: 4px;");
    dom
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let (file, http, color, export, db_value, sync) = match data.downcast_ref::<Demo>() {
        Some(d) => (
            d.file.clone(),
            d.http.clone(),
            d.color.clone(),
            d.export.clone(),
            d.db_value.clone(),
            d.sync.clone(),
        ),
        None => return Dom::create_body(),
    };
    Dom::create_body()
        .with_css("padding: 12px; font-family: sans-serif;")
        .with_child(label("file", file.as_str()))
        .with_child(label("http", http.as_str()))
        .with_child(label("color", color.as_str()))
        .with_child(label("export", export.as_str()))
        .with_child(label("db", db_value.as_str()))
        .with_child(label("sync", sync.as_str()))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
                .with_child(button("Open file", &data, on_open))
                .with_child(button("Fetch", &data, on_fetch))
                .with_child(button("Pick colour", &data, on_pick_color))
                .with_child(button("Export", &data, on_export))
                .with_child(button("Db round-trip", &data, on_db))
                .with_child(button("Sync", &data, on_sync)),
        )
}

fn set<F: FnOnce(&mut Demo)>(data: &mut RefAny, f: F) -> Update {
    if let Some(mut d) = data.downcast_mut::<Demo>() {
        f(&mut d);
    }
    Update::RefreshDom
}

// ---- open file -> read bytes: a chain of two resumes -----------------------

extern "C" fn on_open(data: RefAny, _: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        "Open a file",
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_file_picked,
    );
    Update::DoNothing
}

extern "C" fn on_file_picked(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return set(&mut data, |d| d.file = "file: cancelled".to_string());
    };
    let path_string = path.as_string().as_str().to_string();
    if let Some(mut d) = data.downcast_mut::<Demo>() {
        d.picked_path = path_string;
    }
    let _request = path.read_bytes(data, on_file_read);
    Update::DoNothing
}

extern "C" fn on_file_read(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(read) = FileReadBytesResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let outcome = match read.result.into_result() {
        Ok(bytes) => Some(bytes.len()),
        Err(_) => None,
    };
    set(&mut data, |d| {
        d.file = match outcome {
            Some(len) => format!("file: {} ({len} bytes)", d.picked_path),
            None => format!("file: {} (read failed)", d.picked_path),
        }
    })
}

// ---- http ------------------------------------------------------------------

extern "C" fn on_fetch(data: RefAny, _: CallbackInfo) -> Update {
    let _request = HttpRequestConfig::create()
        .with_timeout(5)
        .http_get(FETCH_URL, data, on_fetched);
    Update::DoNothing
}

extern "C" fn on_fetched(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let text = match answer.result.into_result() {
        Ok(response) => format!("http: {}", response.status_code),
        Err(_) => "http: error".to_string(),
    };
    set(&mut data, |d| d.http = text)
}

// ---- colour picker -----------------------------------------------------------

extern "C" fn on_pick_color(data: RefAny, _: CallbackInfo) -> Update {
    let _request = ColorPickerDialog::open("Pick a colour", OptionColorU::None, data, on_color);
    Update::DoNothing
}

extern "C" fn on_color(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = ColorPickResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let text = match picked.color.into_option() {
        Some(c) => format!("colour: rgb({}, {}, {})", c.r, c.g, c.b),
        None => "colour: cancelled".to_string(),
    };
    set(&mut data, |d| d.color = text)
}

// ---- export (fire-and-forget) -------------------------------------------------

extern "C" fn on_export(mut data: RefAny, _: CallbackInfo) -> Update {
    let bytes: Vec<u8> = b"hello from the resume demo\n".to_vec();
    let scheduled = FileDialog::save_bytes("resume-demo.txt", "text/plain", bytes);
    set(&mut data, |d| {
        d.export = if scheduled {
            "exported: yes".to_string()
        } else {
            "exported: no".to_string()
        }
    })
}

// ---- db: open -> set -> get, then sync ------------------------------------------

extern "C" fn on_db(data: RefAny, _: CallbackInfo) -> Update {
    let config = DbConfig::create(":memory:").with_backup_sync_url(SYNC_URL);
    let _request = Db::open(config, data, on_db_opened);
    Update::DoNothing
}

extern "C" fn on_db_opened(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(opened) = DbOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let db = match opened.result {
        ResultDbDbError::Ok(db) => db,
        ResultDbDbError::Err(e) => {
            let message = format!("db: open failed ({})", e.message.as_str());
            return set(&mut data, |d| d.db_value = message);
        }
    };
    db.set("notes", DbValue::Integer(1), DbValue::Text("hello".into()));
    if let Some(mut d) = data.downcast_mut::<Demo>() {
        d.db = Some(db.clone());
    }
    let _request = db.get("notes", DbValue::Integer(1), data, on_db_got);
    Update::DoNothing
}

extern "C" fn on_db_got(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(answer) = DbValueResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let text = match answer.value.into_option() {
        Some(DbValue::Text(s)) => format!("db: {}", s.as_str()),
        Some(other) => format!("db: {other:?}"),
        None => "db: missing".to_string(),
    };
    set(&mut data, |d| d.db_value = text)
}

extern "C" fn on_sync(mut data: RefAny, _: CallbackInfo) -> Update {
    let db = match data.downcast_ref::<Demo>().and_then(|d| d.db.clone()) {
        Some(db) => db,
        None => return set(&mut data, |d| d.sync = "sync: open the db first".to_string()),
    };
    let _request = db.sync_now(OptionDbScope::None, data, on_synced);
    Update::DoNothing
}

extern "C" fn on_synced(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(answer) = DbSyncStatusResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let text = format!("sync: {:?}", answer.status.state);
    set(&mut data, |d| d.sync = text)
}

fn main() {
    let data = RefAny::new(Demo::new());
    let app = App::create(data, AppConfig::create());
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
