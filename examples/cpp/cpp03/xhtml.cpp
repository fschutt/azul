// g++ -std=c++03 -o xhtml xhtml.cpp -lazul

#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct XhtmlState {
    int dummy;
};
AZ_REFLECT(XhtmlState);

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    const XhtmlState* d = XhtmlState_downcast_ref(data_wrapper);
    if (!d) return AzStyledDom_default();
    
    Dom title = Dom::create_text(String("XHTML Spreadsheet Demo"));
    title.set_inline_style(String("font-size: 24px; font-weight: bold; margin-bottom: 20px;"));
    
    Dom cell1 = Dom::create_text(String("Cell A1"));
    cell1.set_inline_style(String("padding: 8px; border: 1px solid #ccc; background: #f9f9f9;"));
    
    Dom cell2 = Dom::create_text(String("Cell B1"));
    cell2.set_inline_style(String("padding: 8px; border: 1px solid #ccc; background: #f9f9f9;"));
    
    Dom row = Dom::create_div();
    row.set_inline_style(String("display: flex; gap: 0;"));
    row.add_child(cell1);
    row.add_child(cell2);
    
    Dom table = Dom::create_div();
    table.set_inline_style(String("border: 1px solid #333; display: inline-block;"));
    table.add_child(row);
    
    Dom body = Dom::create_body();
    body.set_inline_style(String("padding: 20px; font-family: sans-serif;"));
    body.add_child(title);
    body.add_child(table);
    
    return body.style(Css::empty()).release();
}

int main() {
    XhtmlState state;
    state.dummy = 0;
    RefAny data = XhtmlState_upcast(state);
    
    LayoutCallback layout_cb = LayoutCallback::create(layout);
    WindowCreateOptions window = WindowCreateOptions::create(layout_cb);
    window.inner().window_state.title = AzString_copyFromBytes((const uint8_t*)"XHTML Demo", 0, 10);
    window.inner().window_state.size.dimensions.width = 800.0;
    window.inner().window_state.size.dimensions.height = 600.0;
    
    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
