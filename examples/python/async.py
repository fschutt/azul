# python async.py

from azul import *


class AsyncState:
    def __init__(self):
        # not_connected, connecting, loading, loaded, error
        self.stage = "not_connected"
        self.database_url = "postgres://localhost:5432/mydb"
        self.loaded_data = []
        self.progress = 0.0
        self.error_message = ""


CLICK = EventFilter.Hover(HoverEventFilter.MouseUp)


def connect_button(data):
    return (Dom.create_div()
            .with_css("padding:10px 20px;background:#4CAF50;color:white;cursor:pointer;")
            .with_child(Dom.create_text("Connect"))
            .with_callback(CLICK, data, start_connection))


def reset_button(data):
    return (Dom.create_div()
            .with_css("padding:10px;background:#2196F3;color:white;cursor:pointer;")
            .with_child(Dom.create_text("Reset"))
            .with_callback(CLICK, data, reset_connection))


def progress_view(data):
    return (Dom.create_div()
            .with_child(Dom.create_text(f"Progress: {int(data.progress)}%"))
            .with_child(ProgressBar.create(data.progress).dom()))


def loaded_view(data):
    return (Dom.create_div()
            .with_child(Dom.create_text(f"Loaded {len(data.loaded_data)} records"))
            .with_child(reset_button(data)))


def layout(data, info):
    title = (Dom.create_text("Async Database Connection")
             .with_css("font-size:24px;margin-bottom:20px;"))

    if data.stage == "not_connected":
        content = connect_button(data)
    elif data.stage in ("connecting", "loading"):
        content = progress_view(data)
    elif data.stage == "loaded":
        content = loaded_view(data)
    else:
        content = Dom.create_text(data.error_message)

    body = (Dom.create_body()
            .with_css("padding:30px;font-family:sans-serif;")
            .with_child(title)
            .with_child(content))

    return body


def start_connection(data, info):
    data.stage = "connecting"
    data.progress = 0.0
    return Update.RefreshDom


def reset_connection(data, info):
    data.stage = "not_connected"
    data.progress = 0.0
    data.loaded_data = []
    data.error_message = ""
    return Update.RefreshDom


state = AsyncState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
