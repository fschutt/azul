# XHTML file loading and rendering example

from azul import *

def layout(data, info):
    xhtml = open("assets/spreadsheet.xhtml").read()
    return StyledDom.from_xml(xhtml)

app = App(None, AppConfig.default())
window = WindowCreateOptions(layout)
window.state.title = "XHTML Spreadsheet"
app.run(window)