// Hello-world example for the Azul Kotlin bindings.
//
// Kotlin port of `examples/c/hello-world.c`. Uses the generated
// `com.azul.AzulNative` JNA interface plus idiomatic Kotlin wrapper
// classes (`App`, `WindowCreateOptions`, ...) where they exist.
//
// Behavioural parity with the C version:
//   - Counter starts at 5
//   - Layout draws a label showing the counter and an "Increase counter" button
//   - Clicking the button increments the counter and refreshes the DOM
//
// Build via the sibling `build.gradle.kts`:
//
//     ./gradlew run
//
// Place the prebuilt native library under
// `src/main/resources/{linux-x86-64,win32-x86-64,darwin}/` so JNA picks
// it up on `Native.load("azul", ...)`.

import com.azul.App
import com.azul.AzAppConfig
import com.azul.AzCallbackInfo
import com.azul.AzCss
import com.azul.AzDom
import com.azul.AzLayoutCallbackInfo
import com.azul.AzRefAny
import com.azul.AzString
import com.azul.AzUpdate
import com.azul.AzulNative
import com.azul.AzCallbackType
import com.azul.AzLayoutCallbackType
import com.azul.WindowCreateOptions

import com.sun.jna.Memory
import com.sun.jna.Pointer
import java.nio.charset.StandardCharsets
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

// ── Data model ─────────────────────────────────────────────────────────────
//
// Kotlin has no macro system, so we approximate the C example's
// `AZ_REFLECT_JSON` registration by keeping a class-level map keyed by
// a numeric id and threading the id through a RefAny payload.

data class MyDataModel(var counter: Long)

private val MODELS = ConcurrentHashMap<Long, MyDataModel>()
private val NEXT_ID = AtomicLong(1L)

// Keep JNA callback objects strongly reachable; the native side holds
// raw function pointers and JNA collects unreferenced callbacks.
private lateinit var layoutDelegate: AzLayoutCallbackType
private lateinit var onClickDelegate: AzCallbackType

// ── Callbacks ──────────────────────────────────────────────────────────────

private fun onClick(dataPtr: Pointer?, info: AzCallbackInfo.ByValue): AzUpdate.ByValue {
    val id = if (dataPtr == null) 0L else Pointer.nativeValue(dataPtr)
    val m = MODELS[id]
    val res = AzUpdate.ByValue()
    if (m == null) {
        res.DoNothing.tag = 0
        res.setType("DoNothing")
    } else {
        m.counter += 1
        res.RefreshDom.tag = 1
        res.setType("RefreshDom")
    }
    return res
}

private fun layout(dataPtr: Pointer?, info: AzLayoutCallbackInfo.ByValue): AzDom.ByValue {
    val id = if (dataPtr == null) 0L else Pointer.nativeValue(dataPtr)
    val m = MODELS[id]
    val counter = m?.counter ?: 0L

    val labelText = utf8String(counter.toString())
    val label = AzulNative.INSTANCE.AzDom_createText(labelText)

    val labelWrapper = AzulNative.INSTANCE.AzDom_createDiv()
    // SKIPPED: idiomatic CSS-property mutation — needs ref-mut wrappers
    // we have not generated yet. The native call mutates the JNA struct
    // through its underlying Pointer.
    AzulNative.INSTANCE.AzDom_addChild(labelWrapper.pointer, label)

    val buttonText = utf8String("Increase counter")
    val button = AzulNative.INSTANCE.AzButton_create(buttonText)
    val dataClone = AzulNative.INSTANCE.AzRefAny_clone(dataPtr)
    AzulNative.INSTANCE.AzButton_setOnClick(button.pointer, dataClone, onClickDelegate)
    val buttonDom = AzulNative.INSTANCE.AzButton_dom(button)

    val body = AzulNative.INSTANCE.AzDom_createBody()
    AzulNative.INSTANCE.AzDom_addChild(body.pointer, labelWrapper)
    AzulNative.INSTANCE.AzDom_addChild(body.pointer, buttonDom)

    val css = AzulNative.INSTANCE.AzCss_empty()
    return AzulNative.INSTANCE.AzDom_style(body, css)
}

// ── Main ───────────────────────────────────────────────────────────────────

fun main() {
    val id = NEXT_ID.getAndIncrement()
    MODELS[id] = MyDataModel(5L)

    val pinned = Pointer(id)
    val data = makeRefAny(pinned)

    layoutDelegate = AzLayoutCallbackType { d, i -> layout(d, i) }
    onClickDelegate = AzCallbackType { d, i -> onClick(d, i) }

    WindowCreateOptions.create(layoutDelegate).use { window ->
        // SKIPPED: deep window_state mutation through the wrapper.
        // The wrapper exposes rawPointer(); leaving defaults for this
        // hello-world keeps the example focused.

        App.create(data, AzulNative.INSTANCE.AzAppConfig_create()).use { app ->
            // The C example calls AzApp_run with a stack-allocated
            // AzWindowCreateOptions. Re-materialise one from the
            // wrapper's raw pointer.
            val raw = com.azul.AzWindowCreateOptions.ByValue()
            raw.pointer = window.rawPointer()
            raw.read()
            app.run(raw)
        }
    }

    MODELS.remove(id)
}

// ── Helpers ────────────────────────────────────────────────────────────────

/** Allocate an AzString from a Kotlin string via UTF-8 + copyFromBytes. */
private fun utf8String(s: String): AzString.ByValue {
    val bytes = s.toByteArray(StandardCharsets.UTF_8)
    val buf = Memory(if (bytes.isEmpty()) 1L else bytes.size.toLong())
    if (bytes.isNotEmpty()) buf.write(0L, bytes, 0, bytes.size)
    return AzulNative.INSTANCE.AzString_copyFromBytes(buf, 0L, bytes.size.toLong())
}

/** Wrap a raw Pointer in an AzRefAny via the C-level newC helper. */
// SKIPPED: a real implementation would register a destructor and a
// type id; the hello-world uses the bare-bones helper exposed by the
// framework for opaque payloads.
private fun makeRefAny(p: Pointer): AzRefAny.ByValue {
    val typeName = utf8String("MyDataModel")
    return AzulNative.INSTANCE.AzRefAny_newC(p, 0L, 0, typeName, Pointer.NULL)
}
