# XHTML file loading and rendering example
# Xml::from_str(s) returns a Result wrapper - match on Ok / Err.

from azul import *


def layout(data, info):
    src = open("assets/spreadsheet.xhtml").read()
    parsed = Xml.from_str(src)
    if parsed.is_ok():
        return Dom.from_parsed_xml(parsed.unwrap())
    return Dom.create_body()


app = App.create(None, AppConfig.create())
window = WindowCreateOptions.create(layout)
app.run(window)
