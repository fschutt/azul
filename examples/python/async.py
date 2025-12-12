# Async Operations - Python
# python async.py

from azul import *

class AsyncState:
    def __init__(self):
        self.stage = "not_connected"  # not_connected, connecting, loading, loaded, error
        self.database_url = "postgres://localhost:5432/mydb"
        self.loaded_data = []
        self.progress = 0.0
        self.error_message = ""

def layout(data, info):
    title = Dom.text("Async Database Connection")
    title.set_inline_style("font-size: 24px; margin-bottom: 20px;")
    
    if data.stage == "not_connected":
        button = Dom.div()
        button.set_inline_style("padding: 10px 20px; background: #4CAF50; color: white; cursor: pointer;")
        button.add_child(Dom.text("Connect"))
        button.set_callback(On.MouseUp, data, start_connection)
        content = button
        
    elif data.stage in ["connecting", "loading"]:
        status = Dom.text(f"Progress: {int(data.progress)}%")
        progress_bar = ProgressBar(data.progress).dom()
        content = Dom.div()
        content.add_child(status)
        content.add_child(progress_bar)
        
    elif data.stage == "loaded":
        status = Dom.text(f"Loaded {len(data.loaded_data)} records")
        
        reset_btn = Dom.div()
        reset_btn.set_inline_style("padding: 10px; background: #2196F3; color: white; cursor: pointer;")
        reset_btn.add_child(Dom.text("Reset"))
        reset_btn.set_callback(On.MouseUp, data, reset_connection)
        
        content = Dom.div()
        content.add_child(status)
        content.add_child(reset_btn)
        
    else:  # error
        content = Dom.text(data.error_message)
    
    body = Dom.body()
    body.set_inline_style("padding: 30px; font-family: sans-serif;")
    body.add_child(title)
    body.add_child(content)
    
    return body.style(Css.empty())

def start_connection(data, info):
    data.stage = "connecting"
    data.progress = 0.0
    
    timer = Timer(data, on_timer_tick, info.get_system_time_fn())
    timer.set_interval(Duration.milliseconds(100))
    info.start_timer(timer)
    
    return Update.RefreshDom

def on_timer_tick(data, info):
    data.progress += 2.0
    
    if data.progress >= 100.0:
        data.stage = "loaded"
        data.loaded_data = [f"Record {i+1}" for i in range(10)]
        return Update.RefreshDomAndStopTimer
    
    return Update.RefreshDom

def reset_connection(data, info):
    data.stage = "not_connected"
    data.progress = 0.0
    data.loaded_data = []
    data.error_message = ""
    return Update.RefreshDom

state = AsyncState()

window = WindowCreateOptions(layout)
window.set_title("Async Operations")
window.set_dimensions(600, 400)

app = App(state, AppConfig.default())
app.run(window)
