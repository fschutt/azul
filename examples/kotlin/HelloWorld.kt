// examples/kotlin/HelloWorld.kt
//
// Kotlin port of examples/c/hello-world.c built against the host-invoker
// runtime helpers in `Azul.kt` (see `lang_kotlin/managed.rs`).
//
// Same shape as examples/csharp/hello-world.cs and examples/java/HelloWorld.java:
//   * `AzulHostInvoker.refanyCreate(value)` wraps any Kotlin object in
//     an AzRefAny held alive by the framework's refcount.
//   * Callbacks implement JNA `Callback` interfaces (e.g.
//     `AzulNativeManaged.CallbackInvokerCallback`) and pass through
//     `AzulHostInvoker.registerCallback(handler)`, which returns
//     the `AzCallback.ByValue` cdata struct the C ABI expects.
//
// Build + run via the sibling build.gradle.kts:
//
//     ./gradlew run

package com.azul

import com.sun.jna.Pointer

class MyDataModel(var counter: Int)

fun main() {
    val model = MyDataModel(5)
    val data: AzRefAny.ByValue = AzulHostInvoker.refanyCreate(model)

    val onClick = AzulNativeManaged.CallbackInvokerCallback {
        id, dataPtr, _, outPtr ->
        val obj = AzulHostInvoker.refanyGet(dataPtr)
        val update = if (obj is MyDataModel) {
            obj.counter += 1
            1 // AzUpdate.RefreshDom
        } else {
            0 // AzUpdate.DoNothing
        }
        outPtr?.setInt(0, update)
    }

    val layout = AzulNativeManaged.LayoutCallbackInvokerCallback {
        id, _, _, _ ->
        // wrappers.rs callback substitution is a future PR. Until then the
        // hello-world only proves the host-invoker plumbing wires up.
        System.err.println("[azul] layout callback fired (id=$id)")
    }

    val clickCb = AzulHostInvoker.registerCallback(onClick)
    val layoutCb = AzulHostInvoker.registerLayoutCallback(layout)

    println("[azul] host-invoker plumbing wired.")
    println("[azul] (Full App.run wiring requires struct-field setters from")
    println("[azul]  lang_kotlin/wrappers.rs which is still a stub today.)")

    if (clickCb == null || layoutCb == null) System.exit(1)
}
