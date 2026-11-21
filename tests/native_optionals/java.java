// Java: the value or `null` where an Option is asked for, `Optional<T>` back
// from fields and downcasts, an exception for an Err (inside a callback the
// binding logs it and the callback does nothing), and the resume callback
// as a typed method reference. Built and run by
// scripts/native_optionals_smoke.sh (the class is not public, so the file
// name is free).
import com.azul.*;
import java.util.Optional;

final class Model {
    String folder = "";
    String link = "no scheme";
}

final class NativeOptionals {
    // The guide's file-dialog example, as written there (hello-world/java.md).
    static Update onChooseFolder(Model model, CallbackInfo info) {
        FileDialog.openDirectory("Choose a folder", null, model, NativeOptionals::onPickFolder);
        return Update.DoNothing;
    }

    static Update onPickFolder(Model model, CallbackInfo info, RefAny result) {
        Optional<FilePath> path = FileOpenResult.downcast(result).flatMap(FileOpenResult::getPath);
        if (path.isEmpty()) return Update.DoNothing; // not a folder pick, or cancelled
        model.folder = path.get().getInner();
        return Update.RefreshDom;
    }

    static Update onOpenLink(Model model, CallbackInfo info) {
        Url url = Url.parse(model.link); // an Err throws: logged, the callback does nothing
        return Update.RefreshDom;
    }

    static void check(boolean ok, String what) {
        if (ok) return;
        System.err.println("FAIL: " + what);
        System.exit(1);
    }

    public static void main(String[] args) {
        // A field: null in, empty out; a value in, the value out.
        IconVariants icons = IconVariants.createDefault();
        check(icons.getLight().isEmpty(), "a default IconVariants has no light icon");
        icons.setLight("light.png");
        check(icons.getLight().equals(Optional.of("light.png")), "Some in, Some out");
        icons.setLight(null);
        check(icons.getLight().isEmpty(), "None in, None out");

        // A class payload: a Dom moved in, a Dom read back, null clears it.
        VirtualViewReturn view = VirtualViewReturn.createDefault();
        check(view.getDom().isEmpty(), "a default VirtualViewReturn has no Dom");
        view.setDom(Dom.createBody());
        check(view.getDom().isPresent(), "a Dom moved into an Option field reads back");
        view.setDom(null);
        check(view.getDom().isEmpty(), "null clears the field");

        // A result: Ok is the value, Err an exception.
        check(Url.parse("https://azul.rs/guide") != null, "Url.parse Ok");
        boolean threw = false;
        try {
            Url.parse("no scheme");
        } catch (RuntimeException e) {
            threw = true;
        }
        check(threw, "Url.parse Err throws");

        Button button = Button.create("Open").withOnClick(new Model(), NativeOptionals::onOpenLink);
        System.out.println("java native optionals: ok");
    }
}
