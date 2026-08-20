use azul::prelude::*;
use azul::error::{ResultXmlXmlError, XmlError};
use azul::xml::Xml;

static XHTML: &str = include_str!("../../assets/spreadsheet.xhtml");

struct AppData;

fn error_message(err: &XmlError) -> &'static str {
    match err {
        XmlError::NoParserAvailable => "no XML parser is available",
        XmlError::NoRootNode => "the document has no root node",
        XmlError::UnclosedRootNode => "the root node is never closed",
        XmlError::SizeLimit => "the document exceeds the parser size limit",
        XmlError::DtdDetected => "DTDs are rejected by the parser",
        XmlError::UnexpectedEndOfStream => "unexpected end of stream",
        _ => "the document is not well-formed XML",
    }
}

fn error_dom(message: &str) -> Dom {
    let mut body = Dom::create_body();
    body.set_css("display: flex; flex-direction: column; padding: 24px; background: #fdf2f2;");

    let mut heading = Dom::create_div_with_text("XHTML parse failed");
    heading.set_css("font-size: 20px; font-weight: bold; color: #a61b1b; margin-bottom: 8px;");

    let mut detail = Dom::create_div_with_text(message);
    detail.set_css("font-size: 14px; color: #5f2120;");

    body.add_child(heading);
    body.add_child(detail);
    body
}

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    match Xml::from_str(XHTML) {
        ResultXmlXmlError::Ok(ref xml) => Dom::create_from_parsed_xml(Xml::clone(xml)),
        ResultXmlXmlError::Err(ref e) => error_dom(error_message(e)),
    }
}

fn main() {
    let data = RefAny::new(AppData);
    let app = App::create(data, AppConfig::create());
    let mut options = WindowCreateOptions::create(layout);
    options.window_state.title = "Book1 - Excel".into();
    options.window_state.size.dimensions.width = 1100.0;
    options.window_state.size.dimensions.height = 720.0;
    app.run(options);
}
