// C#: `null` where an Option is asked for, a nullable back from fields and
// downcasts, an exception for an Err (inside a callback the binding logs it
// and the callback does nothing), and the resume callback as a typed
// delegate. Built and run by scripts/native_optionals_smoke.sh.
using System;
using Azul;

public sealed class Model
{
    public string Folder = "";
    public string Link = "no scheme";
}

public static class Program
{
    // The guide's file-dialog example, as written there (hello-world/csharp.md).
    static Update OnChooseFolder(Model model, CallbackInfo info)
    {
        FileDialog.OpenDirectory("Choose a folder", null, model, OnPickFolder);
        return Update.DoNothing;
    }

    static Update OnPickFolder(Model model, CallbackInfo info, RefAny result)
    {
        if (FileOpenResult.Downcast(result)?.Path is not { } path)
            return Update.DoNothing; // not a folder pick, or cancelled
        model.Folder = path.Inner;
        return Update.RefreshDom;
    }

    static Update OnOpenLink(Model model, CallbackInfo info)
    {
        var url = Url.Parse(model.Link); // an Err throws: logged, the callback does nothing
        return Update.RefreshDom;
    }

    static void Check(bool ok, string what)
    {
        if (ok) return;
        Console.Error.WriteLine("FAIL: " + what);
        Environment.Exit(1);
    }

    public static int Main()
    {
        // A field: null in, null out; a value in, the value out.
        using var icons = IconVariants.CreateDefault();
        Check(icons.Light is null, "a default IconVariants has no light icon");
        icons.Light = "light.png";
        Check(icons.Light == "light.png", "Some in, Some out");
        icons.Light = null;
        Check(icons.Light is null, "None in, None out");

        // A class payload: a Dom moved in, a Dom read back, null clears it.
        using var view = VirtualViewReturn.CreateDefault();
        Check(view.Dom is null, "a default VirtualViewReturn has no Dom");
        view.Dom = Dom.CreateBody();
        Check(view.Dom is not null, "a Dom moved into an Option field reads back");
        view.Dom = null;
        Check(view.Dom is null, "null clears the field");

        // A result: Ok is the value, Err an exception.
        Check(Url.Parse("https://azul.rs/guide") != null, "Url.Parse Ok");
        var threw = false;
        try { Url.Parse("no scheme"); } catch (Exception) { threw = true; }
        Check(threw, "Url.Parse Err throws");

        Func<Model, CallbackInfo, Update> click = OnChooseFolder;
        Func<Model, CallbackInfo, Update> open = OnOpenLink;
        var button = Button.Create("Open").OnClick(new Model(), OnOpenLink);
        Console.WriteLine("csharp native optionals: ok");
        return 0;
    }
}
