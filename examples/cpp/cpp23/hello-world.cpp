// g++ -std=c++23 -o hello-world hello-world.cpp -lazul

#include "azul23.hpp"
#include <expected>
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Wrapper payload types: the Result's toStdExpected()/operator hands
// ownership of Url / UrlParseError to the std::expected.
static std::expected<Url, UrlParseError> parse_homepage_url() {
    return Url::parse("https://example.com/"sv);
}
static bool homepage_ok() { return parse_homepage_url().has_value(); }

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return Dom::create_body();

    String css = homepage_ok()
        ? String(R"(body { background-color: #efefef; })")
        : String(R"(body { background-color: #ffaaaa; })");

    Dom body = Dom::create_body();
    body = body.with_child(Dom::create_div()
        .with_css("font-size: 32px;"sv)
        .with_child(Dom::create_text(String(std::to_string(d->counter).c_str()))));
    body = body.with_child(Button::create("Increase counter"sv)
        .with_button_type(ButtonType::Primary)
        .with_on_click(data_wrapper.clone(), on_click)
        .dom());
    body = body.with_css(std::move(css));
    return std::move(body);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return Update::DoNothing;
    d->counter += 1;
    return Update::RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
