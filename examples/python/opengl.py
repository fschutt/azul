# python opengl.py

from azul import *


class OpenGlState:
    def __init__(self):
        self.rotation_deg = 0.0
        self.texture_uploaded = False


def layout(data, info):
    title = (Dom.create_text("OpenGL Integration Demo")
             .with_css("color:white;font-size:24px;margin-bottom:20px;"))

    placeholder = (Dom.create_text(
        "OpenGL texture would render here (timer-driven animation pending)")
        .with_css("flex-grow:1;min-height:300px;border-radius:10px;"
                  "background:#222;color:white;display:flex;"
                  "align-items:center;justify-content:center;"
                  "box-shadow:0px 0px 20px rgba(0,0,0,0.5);"))

    body = (Dom.create_body()
            .with_css("background:linear-gradient(#1a1a2e, #16213e);padding:20px;")
            .with_child(title)
            .with_child(placeholder))

    return body.style(Css.empty())


state = OpenGlState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
