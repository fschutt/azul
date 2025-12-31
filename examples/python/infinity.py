# Infinite Scrolling - Python
# python infinity.py

from azul import *

class InfinityState:
    def __init__(self):
        self.file_paths = []
        self.visible_start = 0
        self.visible_count = 20
        
        # Generate dummy file names
        for i in range(1000):
            self.file_paths.append(f"image_{i:04d}.png")

def layout(data, info):
    title = Dom.text(f"Infinite Gallery - {len(data.file_paths)} images")
    title.set_inline_style("font-size: 20px; margin-bottom: 10px;")
    
    iframe = Dom.iframe(data, render_iframe)
    iframe.set_inline_style("flex-grow: 1; overflow: scroll; background: #f5f5f5;")
    iframe.set_callback(On.Scroll, data, on_scroll)
    
    body = Dom.body()
    body.set_inline_style("padding: 20px; font-family: sans-serif;")
    body.add_child(title)
    body.add_child(iframe)
    
    return body.style(Css.empty())

def render_iframe(data, info):
    container = Dom.div()
    container.set_inline_style("display: flex; flex-wrap: wrap; gap: 10px; padding: 10px;")
    
    end = min(data.visible_start + data.visible_count, len(data.file_paths))
    for i in range(data.visible_start, end):
        item = Dom.div()
        item.set_inline_style("width: 150px; height: 150px; background: white; border: 1px solid #ddd;")
        item.add_child(Dom.text(data.file_paths[i]))
        container.add_child(item)
    
    return container.style(Css.empty())

def on_scroll(data, info):
    scroll_pos = info.get_scroll_position()
    if not scroll_pos:
        return Update.DoNothing
    
    new_start = int(scroll_pos.y / 160) * 4
    if new_start != data.visible_start:
        data.visible_start = min(new_start, len(data.file_paths))
        return Update.RefreshDom
    
    return Update.DoNothing

state = InfinityState()

window = WindowCreateOptions.create(layout)

app = App.create(state, AppConfig.create())
app.run(window)
