# Graphics Stress Test - Python
# python graphics.py

from azul import *

# Style constants
ROOT_STYLE = "display:flex; flex-direction:column; width:100%; height:100%; padding:20px;"
ROW_STYLE = "display:flex; gap:20px; margin-bottom:20px;"
ROW_STYLE_LAST = "display:flex; gap:20px;"

GRADIENTS = [
    "width:200px; height:120px; border-radius:15px; "
    "background:linear-gradient(135deg,#667eea,#764ba2); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);",
    
    "width:200px; height:120px; border-radius:15px; "
    "background:radial-gradient(circle,#f093fb,#f5576c); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);",
    
    "width:200px; height:120px; border-radius:15px; "
    "background:conic-gradient(#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);"
]

FILTERS = [
    "width:180px; height:100px; border-radius:10px; "
    "background:#4a90d9; filter:grayscale(100%);",
    
    "width:180px; height:100px; border-radius:10px; "
    "background:rgba(255,255,255,0.2); backdrop-filter:blur(10px);",
    
    "width:180px; height:100px; border-radius:10px; "
    "background:#e91e63; opacity:0.6;"
]

BORDERS = [
    "width:180px; height:100px; border:3px solid #f44336; "
    "border-radius:10px; background:#ffebee;",
    
    "width:180px; height:100px; border:3px solid #4caf50; "
    "border-radius:10px; background:#e8f5e9;",
    
    "width:180px; height:100px; border:3px solid #2196f3; "
    "border-radius:10px; background:#e3f2fd;"
]

class AppData:
    pass

def row(row_style, styles):
    r = Dom.div()
    r.set_inline_style(row_style)
    for style in styles:
        child = Dom.div()
        child.set_inline_style(style)
        r.add_child(child)
    return r

def layout(data, info):
    root = Dom.div()
    root.set_inline_style(ROOT_STYLE)
    root.add_child(row(ROW_STYLE, GRADIENTS))
    root.add_child(row(ROW_STYLE, FILTERS))
    root.add_child(row(ROW_STYLE_LAST, BORDERS))
    return root.style(Css.empty())

window = WindowCreateOptions(layout)
window.set_title("Graphics Stress Test")
window.set_dimensions(800, 600)

app = App(AppData(), AppConfig.default())
app.run(window)
