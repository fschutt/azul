// examples/java/HelloWorld.java
//
// Minimal Java smoke test for the Azul host-invoker plumbing. Confirms
// that the JNA bindings load, the dylib initialises, and the host-invoker
// init phase (refanyCreate / refanyGet) round-trips a managed object.
//
// Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
// the wrapper layer's idiomatic API surface to settle — separate work,
// not host-invoker. The C# hello-world has the same shape; both verify
// the FFI plumbing one level above libffi.
//
// Build + run:
//     mvn package
//     java -Djna.library.path=. -cp target/hello-world-1.0.0.jar:$(...) com.azul.HelloWorld

package com.azul;

import com.sun.jna.Pointer;

public final class HelloWorld {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    // NOTE: `java.lang.String[]` must be fully qualified because the
    // generated `com.azul.String` Az-wrapper class shadows `java.lang.String`
    // in package scope. Without the qualification the JVM doesn't recognise
    // this as a valid main method.
    public static void main(java.lang.String[] args) {
        MyDataModel model = new MyDataModel(5);
        AzRefAny.ByValue data = AzulHostInvoker.refanyCreate(model);
        java.lang.System.out.println("[azul] refanyCreate ran; RefAny opaque-handle id stored.");

        Object recovered = AzulHostInvoker.refanyGet(data.getPointer());
        if (recovered instanceof MyDataModel) {
            MyDataModel m = (MyDataModel) recovered;
            if (m.counter == 5) {
                java.lang.System.out.println("[azul] refanyGet round-trip succeeded; counter=" + m.counter);
            } else {
                java.lang.System.out.println("[azul] refanyGet round-trip FAILED (counter=" + m.counter + ")");
                java.lang.System.exit(1);
            }
        } else {
            java.lang.System.out.println("[azul] refanyGet round-trip FAILED (recovered=" + recovered + ")");
            java.lang.System.exit(1);
        }

        java.lang.System.out.println("[azul] host-invoker init phase completed successfully.");
        java.lang.System.out.println("[azul] (Full App.run wiring requires wrapper-layer API surface");
        java.lang.System.out.println("[azul]  fixes that are separate from the host-invoker plumbing.)");
    }
}
