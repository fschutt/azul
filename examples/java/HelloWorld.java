// Hello-world example for the Azul Java bindings.
//
// Java port of `examples/c/hello-world.c`. It uses the generated Azul
// JNA bindings: `AzulNative` for the FFI interface, `Az<Type>` JNA
// `Structure` subclasses for FFI-shaped data, and idiomatic
// `AutoCloseable` wrapper classes (`App`, `WindowCreateOptions`, ...)
// where they exist.
//
// Behavioural parity with the C version:
//   - Counter starts at 5
//   - Layout draws a label showing the counter and an "Increase counter" button
//   - Clicking the button increments the counter and refreshes the DOM
//
// Build via the sibling `pom.xml`:
//
//     mvn package
//     java -cp target/azul-1.0.0.jar:target/dependency/jna-5.14.0.jar HelloWorld
//
// Place the prebuilt native library under
// `src/main/resources/{linux-x86-64,win32-x86-64,darwin}/` so JNA picks
// it up on `Native.load("azul", ...)`.

import com.azul.App;
import com.azul.AzAppConfig;
import com.azul.AzCallbackInfo;
import com.azul.AzCss;
import com.azul.AzDom;
import com.azul.AzLayoutCallbackInfo;
import com.azul.AzRefAny;
import com.azul.AzString;
import com.azul.AzUpdate;
import com.azul.AzWindowCreateOptions;
import com.azul.AzulNative;
import com.azul.WindowCreateOptions;

import com.sun.jna.Native;
import com.sun.jna.Pointer;

import java.lang.ref.WeakReference;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

public final class HelloWorld {

    // ── Data model ──────────────────────────────────────────────────────
    //
    // The C example uses `AZ_REFLECT_JSON` to wire a custom type into
    // the framework's RefAny machinery. Java has no macro system; we
    // approximate by keeping the live `MyDataModel` instance in a
    // class-level concurrent map and threading its key through a
    // RefAny. The framework treats the RefAny opaquely.

    static final class MyDataModel {
        long counter;
        MyDataModel(long counter) { this.counter = counter; }
    }

    private static final ConcurrentHashMap<Long, MyDataModel> MODELS = new ConcurrentHashMap<>();
    private static final AtomicLong NEXT_ID = new AtomicLong(1L);

    // Keep callback delegates strongly reachable so JNA does not GC
    // them while the native side holds raw function pointers.
    @SuppressWarnings("unused")
    private static com.azul.AzLayoutCallbackType layoutDelegate;
    @SuppressWarnings("unused")
    private static com.azul.AzCallbackType onClickDelegate;

    // ── Callbacks ───────────────────────────────────────────────────────

    static AzUpdate.ByValue onClick(Pointer dataPtr, AzCallbackInfo.ByValue info) {
        // SKIPPED: real RefAny downcasting needs the generated
        // RefAny/RefAnyMut helpers. We thread the model id through the
        // raw pointer for simplicity in this example.
        long id = Pointer.nativeValue(dataPtr);
        MyDataModel m = MODELS.get(id);
        AzUpdate.ByValue res = new AzUpdate.ByValue();
        if (m == null) {
            // DoNothing variant.
            res.DoNothing.tag = 0;
            res.setType("DoNothing");
        } else {
            m.counter += 1;
            res.RefreshDom.tag = 1;
            res.setType("RefreshDom");
        }
        return res;
    }

    static AzDom.ByValue layout(Pointer dataPtr, AzLayoutCallbackInfo.ByValue info) {
        long id = Pointer.nativeValue(dataPtr);
        MyDataModel m = MODELS.get(id);

        long counter = (m == null) ? 0L : m.counter;

        AzString.ByValue labelText = utf8String(Long.toString(counter));
        AzDom.ByValue label = AzulNative.INSTANCE.AzDom_createText(labelText);

        AzDom.ByValue labelWrapper = AzulNative.INSTANCE.AzDom_createDiv();
        // SKIPPED: idiomatic CSS-property mutation needs ref-mut wrappers
        // we have not generated yet. The native call below mutates the
        // Java struct in place via JNA's Structure.write/read cycle.
        AzulNative.INSTANCE.AzDom_addChild(labelWrapper.getPointer(), label);

        AzString.ByValue buttonText = utf8String("Increase counter");
        com.azul.AzButton.ByValue button = AzulNative.INSTANCE.AzButton_create(buttonText);
        Pointer dataClone = AzulNative.INSTANCE.AzRefAny_clone(dataPtr);
        AzulNative.INSTANCE.AzButton_setOnClick(button.getPointer(), dataClone, onClickDelegate);
        AzDom.ByValue buttonDom = AzulNative.INSTANCE.AzButton_dom(button);

        AzDom.ByValue body = AzulNative.INSTANCE.AzDom_createBody();
        AzulNative.INSTANCE.AzDom_addChild(body.getPointer(), labelWrapper);
        AzulNative.INSTANCE.AzDom_addChild(body.getPointer(), buttonDom);

        AzCss.ByValue css = AzulNative.INSTANCE.AzCss_empty();
        return AzulNative.INSTANCE.AzDom_style(body, css);
    }

    // ── Main ────────────────────────────────────────────────────────────

    public static void main(String[] args) {
        long id = NEXT_ID.getAndIncrement();
        MODELS.put(id, new MyDataModel(5L));

        // Pin the id as the RefAny payload for the duration of the run.
        Pointer pinned = new Pointer(id);
        AzRefAny.ByValue data = makeRefAny(pinned);

        layoutDelegate = HelloWorld::layout;
        onClickDelegate = HelloWorld::onClick;

        try (WindowCreateOptions window = WindowCreateOptions.create(layoutDelegate)) {
            // SKIPPED: deep window_state mutation through the wrapper.
            // The generator surfaces the raw FFI struct via rawPointer();
            // we leave it at defaults for this hello-world.

            try (App app = App.create(data, AzulNative.INSTANCE.AzAppConfig_create())) {
                // App.run consumes the WindowCreateOptions by value.
                AzWindowCreateOptions.ByValue raw = new AzWindowCreateOptions.ByValue();
                raw.setPointer(window.rawPointer());
                raw.read();
                app.run(raw);
            }
        }

        MODELS.remove(id);
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /** Allocate an AzString from a Java String via UTF-8 + copyFromBytes. */
    private static AzString.ByValue utf8String(String s) {
        byte[] bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        com.sun.jna.Memory buf = new com.sun.jna.Memory(bytes.length == 0 ? 1 : bytes.length);
        if (bytes.length > 0) {
            buf.write(0, bytes, 0, bytes.length);
        }
        return AzulNative.INSTANCE.AzString_copyFromBytes(buf, 0L, (long) bytes.length);
    }

    /** Wrap a raw Pointer in an AzRefAny via the C-level newC helper. */
    // SKIPPED: a real implementation would register a destructor and a
    // type id; for the hello-world we use the bare-bones helper exposed
    // by the framework for opaque payloads.
    private static AzRefAny.ByValue makeRefAny(Pointer p) {
        AzString.ByValue typeName = utf8String("MyDataModel");
        return AzulNative.INSTANCE.AzRefAny_newC(p, 0L, 0, typeName, Pointer.NULL);
    }

    private HelloWorld() {}
}
