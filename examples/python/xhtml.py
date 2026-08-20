import os

from azul import *

DOC_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "spreadsheet.xhtml")


def error_dom(message):
    heading = (Dom.create_div()
               .with_css("font-size:20px;font-weight:bold;color:#a61b1b;margin-bottom:8px;")
               .with_child(Dom.create_p_with_text("XHTML load failed")))
    detail = (Dom.create_div()
              .with_css("font-size:13px;color:#5f2120;")
              .with_child(Dom.create_p_with_text(message)))
    return (Dom.create_body()
            .with_css("display:flex;flex-direction:column;padding:24px;background:#fdf2f2;")
            .with_child(heading)
            .with_child(detail))


def layout(data, info):
    try:
        with open(DOC_PATH, encoding="utf-8") as handle:
            src = handle.read()
    except OSError as err:
        return error_dom(str(err))

    parsed = Xml.from_str(src)
    if parsed.is_ok():
        return Dom.create_from_parsed_xml(parsed.unwrap())
    return error_dom("the document is not well-formed XML")


app = App.create(None, AppConfig.create())
window = WindowCreateOptions.create(layout)
app.run(window)
