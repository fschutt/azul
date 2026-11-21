// D: `Nullable!T` where an Option is asked for and back from fields and
// downcasts, a thrown `ResultException` for an Err (inside a callback the
// trampoline logs it and the callback does nothing). Built and run by
// scripts/native_optionals_smoke.sh.
import azul;
import core.stdc.stdlib : exit;
import std.stdio : stderr, writeln;
import std.typecons : Nullable, nullable;

final class Model
{
    string folder;
    string link = "no scheme";
}

// The guide's file-dialog example, as written there (hello-world/d.md).
Update onChooseFolder(Model model, CallbackInfo info)
{
    FileDialog.openDirectory("Choose a folder", Nullable!string(), model, &onPickFolder);
    return Update.doNothing;
}

Update onPickFolder(Model model, CallbackInfo info, RefAny result)
{
    auto picked = FileOpenResult.downcast(result);
    if (picked.isNull || picked.get.path.isNull)
        return Update.doNothing; // not a folder pick, or cancelled
    model.folder = picked.get.path.get.inner;
    return Update.refreshDom;
}

Update onOpenLink(Model model, CallbackInfo info)
{
    auto url = Url.parse(model.link); // an Err is logged, the callback does nothing
    return Update.refreshDom;
}

void check(bool ok, string what)
{
    if (!ok)
    {
        stderr.writeln("FAIL: ", what);
        exit(1);
    }
}

void main()
{
    // A field: None in, None out; Some in, Some out.
    auto icons = IconVariants();
    check(icons.light.isNull, "a default IconVariants has no light icon");
    icons.light = nullable("light.png");
    check(!icons.light.isNull && icons.light.get == "light.png", "Some in, Some out");
    icons.light = Nullable!string();
    check(icons.light.isNull, "None in, None out");

    // A result: Ok is the value, Err a thrown ResultException.
    Url.parse("https://azul.rs/guide");
    bool threw = false;
    try
        Url.parse("no scheme");
    catch (ResultException!UrlParseError e)
        threw = true;
    check(threw, "Url.parse Err throws");

    auto clicked = &onChooseFolder;
    auto picked = &onPickFolder;
    auto opened = &onOpenLink;
    writeln("d native optionals: ok");
}
