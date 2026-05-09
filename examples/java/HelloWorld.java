// examples/java/HelloWorld.java
//
// Java port of examples/c/hello-world.c built against the host-invoker
// runtime helpers in `AzulHostInvoker.java` (see `lang_java/managed.rs`).
//
// Same shape as examples/csharp/hello-world.cs and examples/lua/hello-world.lua:
//   * `AzulHostInvoker.refanyCreate(value)` wraps any Java object in an
//     AzRefAny held alive by the framework's refcount.
//   * Callbacks implement JNA `Callback` interfaces (e.g.
//     `AzulNativeManaged.CallbackInvokerCallback`) and pass through
//     `AzulHostInvoker.registerCallback(handler)`, which returns
//     the `AzCallback` cdata struct the C ABI expects.
//
// Build + run via the sibling pom.xml:
//
//     mvn package
//     java -cp target/azul-hello-world-0.1.0.jar:$(mvn -Dmdep.outputFile=/dev/stdout dependency:build-classpath -q) com.azul.HelloWorld

package com.azul;

import com.sun.jna.Pointer;

public final class HelloWorld {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    public static void main(String[] args) {
        // ── Wrap the model in an AzRefAny ────────────────────────────────
        MyDataModel model = new MyDataModel(5);
        AzRefAny.ByValue data = AzulHostInvoker.refanyCreate(model);

        // ── Callbacks ────────────────────────────────────────────────────
        AzulNativeManaged.CallbackInvokerCallback onClick =
                (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object obj = AzulHostInvoker.refanyGet(dataPtr);
            int update;
            if (obj instanceof MyDataModel) {
                ((MyDataModel) obj).counter++;
                update = 1; // AzUpdate.RefreshDom
            } else {
                update = 0; // AzUpdate.DoNothing
            }
            outPtr.setInt(0, update);
        };

        AzulNativeManaged.LayoutCallbackInvokerCallback layout =
                (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            // wrappers.rs callback substitution is a future PR; the
            // host-invoker plumbing IS wired here. The body is left as a
            // stub for the demo — running App.run() with this binding
            // requires struct-field setters that lang_java/wrappers.rs
            // doesn't yet emit.
            System.err.println("[azul] layout callback fired (id=" + id + ")");
        };

        // ── Register callbacks ───────────────────────────────────────────
        AzCallback.ByValue clickCb = AzulHostInvoker.registerCallback(onClick);
        AzLayoutCallback.ByValue layoutCb = AzulHostInvoker.registerLayoutCallback(layout);

        System.out.println("[azul] host-invoker plumbing wired.");
        System.out.println("[azul] (Full App.run wiring requires struct-field setters from");
        System.out.println("[azul]  lang_java/wrappers.rs which is still a stub today.)");

        if (clickCb == null || layoutCb == null) System.exit(1);
    }
}
