# Infinite Scrolling - Python
# python infinity.py
#
# NOTE: VirtualView requires custom callbacks plus an OptionDom return,
# both of which are still experimental in the Python binding. This example
# falls back to a plain scrolling container that pre-renders a windowed slice.

from azul import *


class InfinityState:
    def __init__(self):
        self.file_paths = [f"image_{i:04d}.png" for i in range(1000)]
        self.visible_start = 0
        self.visible_count = 50


def layout(data, info):
    title = (Dom.create_text(
        f"Infinite Gallery - {len(data.file_paths)} images")
        .with_css("font-size:20px;margin-bottom:10px;"))

    end = min(data.visible_start + data.visible_count, len(data.file_paths))
    container = Dom.create_div().with_css(
        "display:flex;flex-wrap:wrap;gap:10px;padding:10px;"
        "flex-grow:1;overflow:scroll;background:#f5f5f5;")

    for i in range(data.visible_start, end):
        item = (Dom.create_div()
                .with_css("width:150px;height:150px;background:white;"
                          "border:1px solid #ddd;display:flex;"
                          "align-items:center;justify-content:center;")
                .with_child(Dom.create_text(data.file_paths[i])))
        container = container.with_child(item)

    body = (Dom.create_body()
            .with_css("padding:20px;font-family:sans-serif;")
            .with_child(title)
            .with_child(container))
    return body.style(Css.empty())


state = InfinityState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
