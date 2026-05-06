use azul::prelude::*;
use azul::error::ResultXmlXmlError;
use azul::xml::Xml;

// Include XHTML at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct AppData;

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    // Xml::from_str returns ResultXmlXmlError; map Ok to a Dom, Err to a body.
    match Xml::from_str(XHTML) {
        ResultXmlXmlError::Ok(ref xml) => Dom::create_from_parsed_xml(Xml::clone(xml)),
        ResultXmlXmlError::Err(_) => Dom::create_body(),
    }
}

fn main() {
    let data = RefAny::new(AppData);
    let app = App::create(data, AppConfig::create());
    let options = WindowCreateOptions::create(layout);
    app.run(options);
}
