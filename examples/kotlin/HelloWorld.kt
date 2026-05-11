// examples/kotlin/HelloWorld.kt
//
// Minimal Kotlin smoke test for the Azul host-invoker plumbing. Confirms
// that the JNA bindings load, the dylib initialises, and the host-invoker
// init phase (refanyCreate / refanyGet) round-trips a managed object.
//
// Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
// the wrapper layer's idiomatic API surface to settle — separate work,
// not host-invoker. The C# and Java hello-worlds have the same shape;
// all three verify the FFI plumbing one level above libffi.
//
// Build + run (without Gradle):
//   kotlinc -cp $JNA_JAR Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar
//   java -Djna.library.path=. -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt

package com.azul

class MyDataModel(var counter: Int)

fun main() {
    val model = MyDataModel(5)
    val data: AzRefAny.ByValue = AzulHostInvoker.refanyCreate(model)
    println("[azul] refanyCreate ran; RefAny opaque-handle id stored.")

    val recovered = AzulHostInvoker.refanyGet(data.pointer)
    if (recovered is MyDataModel && recovered.counter == 5) {
        println("[azul] refanyGet round-trip succeeded; counter=${recovered.counter}")
    } else {
        println("[azul] refanyGet round-trip FAILED (recovered=$recovered)")
        kotlin.system.exitProcess(1)
    }

    println("[azul] host-invoker init phase completed successfully.")
    println("[azul] (Full App.run wiring requires wrapper-layer API surface")
    println("[azul]  fixes that are separate from the host-invoker plumbing.)")
}
