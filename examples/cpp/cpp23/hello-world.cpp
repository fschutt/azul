// g++ -std=c++23 -o hello-world hello-world.cpp -lazul

#include "azul23.hpp"
#include <version>
#if defined(__cpp_lib_expected)
#include <expected>
#endif
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// std::expected is C++23 library support some toolchains still lack, so the
// demo is guarded on __cpp_lib_expected; homepage_ok() works either way.
#if defined(__cpp_lib_expected)
static std::expected<AzUrl, AzUrlParseError> parse_homepage_url() {
    return Url::parse("https://example.com/"sv);
}
static bool homepage_ok() { return parse_homepage_url().has_value(); }
#else
static bool homepage_ok() { return Url::parse("https://example.com/"sv).isOk(); }
#endif

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return AzDom_createBody();

    String css = homepage_ok()
        ? String(R"(body { background-color: #efefef; })")
        : String(R"(body { background-color: #ffaaaa; })");

    Dom body = Dom::create_body();
    body = body.with_child(Dom::create_p_with_text(String(std::to_string(d->counter).c_str()))
        .with_css("font-size: 50px;"sv));
    body = body.with_child(Button::create("Increase counter"sv)
        .with_button_type(AzButtonType_Primary)
        .with_on_click(data_wrapper.clone(), on_click)
        .dom());
    body = body.with_css(std::move(css));
    return std::move(body);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
