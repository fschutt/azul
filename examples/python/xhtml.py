# XHTML file loading and rendering example

from azul import *

def layout(data, info):
    xhtml = open("assets/spreadsheet.xhtml").read()
    return StyledDom.from_xml(xhtml)

app = App.create(None, AppConfig.create())
window = WindowCreateOptions.create(layout)
app.run(window)