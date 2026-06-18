# python xhtml.py

from azul import *


def layout(data, info):
    src = open("assets/spreadsheet.xhtml").read()
    parsed = Xml.from_str(src)
    if parsed.is_ok():
        return Dom.create_from_parsed_xml(parsed.unwrap())
    return Dom.create_body()


app = App.create(None, AppConfig.create())
window = WindowCreateOptions.create(layout)
app.run(window)
