from azul import *

CLICK = EventFilter.Hover(HoverEventFilter.MouseUp)


class OpenGlState:
    def __init__(self):
        self.rotation_deg = 0.0
        self.step_deg = 15.0


def button(text, data, callback):
    return (Dom.create_div()
            .with_css("padding:6px 14px;margin-right:8px;background:#2d3f5e;color:white;"
                      "border:1px solid #4a6a9e;font-size:12px;cursor:pointer;")
            .with_child(Dom.create_p_with_text(text))
            .with_callback(CLICK, data, callback))


def on_rotate(data, info):
    data.rotation_deg = (data.rotation_deg + data.step_deg) % 360.0
    return Update.RefreshDom


def on_faster(data, info):
    data.step_deg = min(90.0, data.step_deg + 5.0)
    return Update.RefreshDom


def on_slower(data, info):
    data.step_deg = max(5.0, data.step_deg - 5.0)
    return Update.RefreshDom


def layout(data, info):
    title = (Dom.create_div()
             .with_css("color:white;font-size:22px;margin-bottom:16px;")
             .with_child(Dom.create_p_with_text(
                 "OpenGL Integration Demo")))

    quad = (Dom.create_div()
            .with_css("width:160px;height:160px;background:#39c0ed;border-radius:12px;"
                      "box-shadow:0px 0px 24px rgba(0,0,0,0.6);"
                      "transform:rotate(%.0fdeg);" % data.rotation_deg))

    stage = (Dom.create_div()
             .with_css("flex-grow:1;min-height:280px;border-radius:10px;background:#222222;"
                       "display:flex;align-items:center;justify-content:center;"
                       "margin-bottom:16px;")
             .with_child(quad))

    controls = (Dom.create_div()
                .with_css("display:flex;flex-direction:row;align-items:center;")
                .with_child(button("Rotate", data, on_rotate))
                .with_child(button("Faster", data, on_faster))
                .with_child(button("Slower", data, on_slower))
                .with_child(Dom.create_div()
                            .with_css("color:#9fb0c4;font-size:12px;")
                            .with_child(Dom.create_p_with_text(
                                "%.0f deg   step %.0f deg" % (data.rotation_deg, data.step_deg)))))

    return (Dom.create_body()
            .with_css("display:flex;flex-direction:column;height:100%;padding:20px;"
                      "background:#16213e;font-family:sans-serif;")
            .with_child(title)
            .with_child(stage)
            .with_child(controls))


state = OpenGlState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
