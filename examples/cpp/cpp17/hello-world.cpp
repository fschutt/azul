// g++ -std=c++17 -o hello-world hello-world.cpp -lazul

#include "azul17.hpp"
#include <optional>
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
    // OptionXxx wrappers convert implicitly to std::optional<Inner>, so a
    // model field that nullably caches a parsed URL can keep its source-of-
    // truth shape while the rest of the app reads it as std::optional.
    std::optional<AzUrl> last_url;
};

// Callback signatures take the raw C types because the framework dispatches
// through C function pointers. Inside the body we use `azul::downcast_ref<T>`
// and `azul::downcast_mut<T>` directly on the parameter. The wrapper `RefAny`
// class is needed only when we want RAII auto-cleanup of the borrowed
// reference, or when we need to clone it for a child callback.
AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    auto* d = downcast_ref<MyDataModel>(data);
    if (!d) return Dom::create_body();

    // To pass the data to a child callback we clone the underlying ref.
    // `AzRefAny_clone` bumps the refcount; the new handle is owned by
    // whoever consumes it (here: the button).
    AzRefAny on_click_data = AzRefAny_clone(&data);

    // String-taking methods gained std::string_view overloads in C++17,
    // so "..."sv literals flow straight in - no String() wrapping needed.
    // The wrapper's r-value `operator AzDom()` does the C-ABI conversion
    // implicitly on return, so no `.release()` is needed.
    return Dom::create_body()
        .with_child(Dom::create_p_with_text(String(std::to_string(d->counter).c_str()))
            .with_css("font-size: 50px;"sv))
        .with_child(Button::create("Increase counter"sv)
            .with_button_type(AzButtonType_Primary)
            .with_on_click(RefAny(on_click_data), on_click)
            .dom())
        .style(Css::empty());
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    auto* d = downcast_mut<MyDataModel>(data);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

// Every ResultXxx wrapper destructures into (std::optional<Ok>, std::optional<Err>)
// via the codegen's tuple_size / tuple_element specializations - no per-class
// helper, just structured bindings.
static void demo_structured_bindings() {
    auto [ok, err] = std::move(Url::parse("https://example.com/"sv));
    if (ok) {
        // *ok is an AzUrl; the Url wrapper would adopt it via Url(*ok).
    } else if (err) {
        // *err is an AzUrlParseError.
    }
}

int main() {
    MyDataModel model = { 5, std::nullopt };
    (void)demo_structured_bindings;

    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
