# XHTML Example - Python
# python xhtml.py

from azul import *

XHTML = """
<body style='display:flex; flex-direction:column; width:100%; height:100%; padding:40px;'>
  <h1 style='color:#ecf0f1; font-size:48px;'>XHTML Demo</h1>
  <p style='color:#bdc3c7; font-size:18px;'>Loaded from XHTML string</p>
  <div style='display:flex; gap:20px;'>
    <div style='width:200px; height:150px; background:#3498db; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#e74c3c; border-radius:10px;'/>
    <div style='width:200px; height:150px; background:#2ecc71; border-radius:10px;'/>
  </div>
</body>
"""

class AppData:
    pass

def layout(data, info):
    return StyledDom.from_xml(XHTML)

window = WindowCreateOptions(layout)
window.set_title("XHTML Example")
window.set_dimensions(800, 400)

app = App(AppData(), AppConfig.default())
app.run(window)