// C++: std::nullopt, a std::optional or the value where an Option is asked
// for (C++17), a std::optional back (toStdOptional / the implicit
// conversion), and std::expected for a Result (C++23). Built and run by
// scripts/native_optionals_smoke.sh, once as C++17 and once as C++23.
#include AZUL_HEADER
#include <cstdio>
#include <cstdlib>
#include <optional>
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    std::string folder;
    std::string link;
};

// The guide's file-dialog example, as written there (hello-world/cpp.md).
ffi::Update on_pick_folder(ffi::RefAny data, ffi::CallbackInfo info, ffi::RefAny result) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return Update::DoNothing;
    std::optional<FileOpenResult> picked = FileOpenResult::downcast(RefAny(result));
    if (!picked) return Update::DoNothing;
    std::optional<FilePath> path = picked->get_path();
    if (!path) return Update::DoNothing; // cancelled
    d->folder = path->get_inner();
    return Update::RefreshDom;
}

ffi::Update on_choose_folder(ffi::RefAny data, ffi::CallbackInfo info) {
    RefAny data_wrapper(data);
    FileDialog::open_directory("Choose a folder"sv, std::nullopt, data_wrapper.clone(), on_pick_folder);
    return Update::DoNothing;
}

static void check(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        std::exit(1);
    }
}

int main() {
    // A field: std::nullopt in, std::nullopt out; a value in, the value out.
    IconVariants icons = IconVariants::createDefault();
    check(!icons.get_light().toStdOptional(), "a default IconVariants has no light icon");
    icons.set_light("light.png");
    std::optional<String> light = icons.get_light();
    check(light && light->toStdString() == "light.png", "Some in, Some out");
    icons.set_light(std::nullopt);
    check(!icons.get_light().toStdOptional(), "None in, None out");
    icons.set_light(std::optional<String>(String("dark.png")));
    check(icons.get_light().isSome(), "a std::optional goes in as it is");

    // A result: Ok and Err.
    check(Url::parse("https://azul.rs/guide"sv).isOk(), "Url::parse Ok");
    check(Url::parse("no scheme"sv).isErr(), "Url::parse Err");
#if __cplusplus > 202002L
    auto ok = Url::parse("https://azul.rs/guide"sv).toStdExpected();
    check(ok.has_value(), "std::expected Ok");
    auto err = Url::parse("no scheme"sv).toStdExpected();
    check(!err.has_value(), "std::expected Err");
#endif
    (void)on_choose_folder;
    std::printf("cpp native optionals: ok\n");
    return 0;
}
