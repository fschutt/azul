// g++ -std=c++14 -o hello-world hello-world.cpp -lazul

#include "azul14.hpp"
#include <string>

using namespace azul;

struct MyDataModel {
    uint32_t counter;
};

// C++14 lets us keep the click logic inline as a generic lambda. The codegen
// wraps any callable into an extern "C" trampoline via a small std::function
// indirection - users no longer write a free function for trivial handlers.
Dom layout(RefAny data, LayoutCallbackInfo info) {
    auto d = downcast_ref<MyDataModel>(data);  // const MyDataModel*
    if (!d) return Dom::body();

    return Dom::body()
        .with_child(Dom::p_with_text(std::to_string(d->counter))
            .with_inline_style("font-size: 50px;"))
        .with_child(Button::create("Increase counter")
            .with_button_type(ButtonType::Primary)
            .with_on_click(data.clone(), [](auto data, auto info) {
                auto* m = downcast_mut<MyDataModel>(data);
                if (!m) return Update::DoNothing;
                m->counter += 1;
                return Update::RefreshDom;
            })
            .dom())
        .style(Css::empty());
}

int main() {
    // type_id_v is a variable template - shorthand for type_id<T>().
    static_assert(type_id_v<MyDataModel> != 0, "MyDataModel must have a type id");

    MyDataModel model = { 5 };
    RefAny data = upcast(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
