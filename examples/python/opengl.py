# OpenGL Integration - Python
# python opengl.py

from azul import *

class OpenGlState:
    def __init__(self):
        self.rotation_deg = 0.0
        self.texture_uploaded = False

def layout(data, info):
    title = Dom.text("OpenGL Integration Demo")
    title.set_inline_style("color: white; font-size: 24px; margin-bottom: 20px;")
    
    image = Dom.image(ImageRef.callback(data, render_texture))
    image.set_inline_style("""
        flex-grow: 1;
        min-height: 300px;
        border-radius: 10px;
        box-shadow: 0px 0px 20px rgba(0,0,0,0.5);
    """)
    
    body = Dom.body()
    body.set_inline_style("background: linear-gradient(#1a1a2e, #16213e); padding: 20px;")
    body.add_child(title)
    body.add_child(image)
    
    return body.style(Css.empty())

def render_texture(data, info):
    size = info.get_bounds().get_physical_size()
    
    gl_context = info.get_gl_context()
    if not gl_context:
        return ImageRef.null_image(size.width, size.height, RawImageFormat.RGBA8, [])
    
    texture = Texture.allocate_rgba8(gl_context, size, ColorU.from_str("#1a1a2e"))
    texture.clear()
    
    rotation = data.rotation_deg
    
    # Draw rotating rectangles
    texture.draw_rect(
        LogicalRect(100, 100, 200, 200),
        ColorU.from_str("#e94560"),
        [StyleTransform.Rotate(AngleValue.deg(rotation))]
    )
    
    texture.draw_rect(
        LogicalRect(150, 150, 100, 100),
        ColorU.from_str("#0f3460"),
        [StyleTransform.Rotate(AngleValue.deg(-rotation * 2))]
    )
    
    return ImageRef.gl_texture(texture)

def on_startup(data, info):
    timer = Timer(data, animate, info.get_system_time_fn())
    timer.set_interval(Duration.milliseconds(16))
    info.start_timer(timer)
    return Update.DoNothing

def animate(data, info):
    data.rotation_deg += 1.0
    if data.rotation_deg >= 360.0:
        data.rotation_deg = 0.0
    return Update.RefreshDom

state = OpenGlState()

window = WindowCreateOptions.create(layout)

app = App.create(state, AppConfig.create())
app.run(window)
