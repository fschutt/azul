// Azul counter example — Swift.
//
// Build (with libazul.{so,dylib}/azul.dll, azul.h, module.modulemap and the
// generated azul.swift in this directory):
//
//   swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world
//   DYLD_LIBRARY_PATH=. ./hello-world      # macos
//
// Callbacks are C-direct: `onClick` and `layout` are plain Swift funcs whose
// C-compatible signatures convert to `@convention(c)` function pointers and
// are passed straight to the C-ABI setters — no host-invoker, exactly like
// the C / Zig / Odin bindings.

import CAzul

// ── Data model ────────────────────────────────────────────────────────
//
// A process-unique type id (the address of a one-byte heap allocation we
// never read/write), plus upcast/downcast to/from an AzRefAny. Plain old
// data → empty destructor.

struct MyDataModel {
    var counter: UInt32
}

private let myDataToken = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
private let myDataTypeId = UInt64(UInt(bitPattern: myDataToken))

func myDataDestructor(_ ptr: UnsafeMutableRawPointer?) {
    // Plain old data — nothing to free.
}

// Convert a Swift String to an AzString (copies the bytes into a
// refcounted heap buffer, so a temporary source buffer is fine).
func azString(_ s: String) -> AzString {
    let bytes = Array(s.utf8)
    return bytes.withUnsafeBufferPointer { AzString_fromUtf8($0.baseAddress, $0.count) }
}

func myDataUpcast(_ model: MyDataModel) -> AzRefAny {
    // AzRefAny_newC copies the bytes into its own heap allocation, so a
    // stack pointer is fine here; run_destructor=false means libazul won't
    // free the caller's pointer.
    var local = model
    let typeName = azString("MyDataModel")
    return withUnsafePointer(to: &local) { p in
        let wrapper = AzGlVoidPtrConst(ptr: UnsafeRawPointer(p), run_destructor: false)
        return AzRefAny_newC(
            wrapper,
            MemoryLayout<MyDataModel>.size,
            MemoryLayout<MyDataModel>.alignment,
            myDataTypeId,
            typeName,
            myDataDestructor,
            0, // no serialize_fn
            0  // no deserialize_fn
        )
    }
}

func myDataDowncast(_ refany: inout AzRefAny) -> UnsafeMutablePointer<MyDataModel>? {
    if !AzRefAny_isType(&refany, myDataTypeId) {
        return nil
    }
    guard let ptr = AzRefAny_getDataPtr(&refany) else {
        return nil
    }
    return UnsafeMutableRawPointer(mutating: ptr).assumingMemoryBound(to: MyDataModel.self)
}

// ── Callback: button click ────────────────────────────────────────────

func onClick(_ data: AzRefAny, _ info: AzCallbackInfo) -> AzUpdate {
    var d = data
    guard let m = myDataDowncast(&d) else {
        return AzUpdate_DoNothing
    }
    m.pointee.counter += 1
    return AzUpdate_RefreshDom
}

// ── Layout callback ───────────────────────────────────────────────────

func layout(_ data: AzRefAny, _ info: AzLayoutCallbackInfo) -> AzDom {
    var d = data
    guard let m = myDataDowncast(&d) else {
        return AzDom_createBody()
    }

    // Counter label (wrapped in a div so the font-size sticks).
    let counterStr = azString(String(m.pointee.counter))
    let label = AzDom_createText(counterStr)

    var labelWrapper = AzDom_createDiv()
    let fontSize = AzStyleFontSize_px(32.0)
    let cssProp = AzCssProperty_fontSize(fontSize)
    let cond = AzCssPropertyWithConditions_simple(cssProp)
    AzDom_addCssProperty(&labelWrapper, cond)
    AzDom_addChild(&labelWrapper, label)

    // Increment button. AzButton_setOnClick takes the bare fn-pointer
    // typedef — `onClick` (a plain func) converts to `@convention(c)`.
    var button = AzButton_create(azString("Increase counter"))
    AzButton_setButtonType(&button, AzButtonType_Primary)
    let dataClone = AzRefAny_clone(&d)
    AzButton_setOnClick(&button, dataClone, onClick)
    let buttonDom = AzButton_dom(button)

    // Body.
    var body = AzDom_createBody()
    AzDom_addChild(&body, labelWrapper)
    AzDom_addChild(&body, buttonDom)
    return body
}

// ── Main ──────────────────────────────────────────────────────────────
//
// `@main` provides the entry point. (A plain top-level `AzApp_run(...)` is
// only allowed in a file literally named `main.swift`; `@main` lets the
// driver keep its `hello-world.swift` name while compiling alongside
// `azul.swift` as one module.)

@main
struct HelloWorld {
    static func main() {
        let model = MyDataModel(counter: 5)
        let data = myDataUpcast(model)

        var window = AzWindowCreateOptions_create(layout)
        window.window_state.title = azString("Hello World")
        window.window_state.size.dimensions.width = 400.0
        window.window_state.size.dimensions.height = 300.0

        // NoTitleAutoInject: OS draws close/min/max buttons; framework
        // auto-injects a Titlebar with drag support.
        window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
        window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

        var app = AzApp_create(data, AzAppConfig_create())
        AzApp_run(&app, window)
    }
}
