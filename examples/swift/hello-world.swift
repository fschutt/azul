import CAzul

struct MyDataModel {
    var counter: UInt32
}

private let myDataToken = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
private let myDataTypeId = UInt64(UInt(bitPattern: myDataToken))

func myDataDestructor(_ ptr: UnsafeMutableRawPointer?) {}

func azString(_ s: String) -> AzString {
    let bytes = Array(s.utf8)
    return bytes.withUnsafeBufferPointer { AzString_fromUtf8($0.baseAddress, $0.count) }
}

func myDataUpcast(_ model: MyDataModel) -> AzRefAny {
    // AzRefAny_newC copies the bytes into its own heap allocation, so a
    // stack pointer is fine; run_destructor=false ⇒ libazul won't free ours.
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

// A plain (non-capturing) top-level func converts to a `@convention(c)` pointer.

func onClick(_ data: AzRefAny, _ info: AzCallbackInfo) -> AzUpdate {
    var d = data
    guard let m = myDataDowncast(&d) else {
        return AzUpdate_DoNothing
    }
    m.pointee.counter += 1
    return AzUpdate_RefreshDom
}

func layout(_ data: AzRefAny, _ info: AzLayoutCallbackInfo) -> AzDom {
    var d = data
    guard let m = myDataDowncast(&d) else {
        return AzDom_createBody()
    }

    let counterStr = azString(String(m.pointee.counter))
    let label = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(counterStr)

    var labelWrapper = AzDom_createDiv()
    let fontSize = AzStyleFontSize_px(32.0)
    let cssProp = AzCssProperty_fontSize(fontSize)
    let cond = AzCssPropertyWithConditions_simple(cssProp)
    AzDom_addCssProperty(&labelWrapper, cond)
    AzDom_addChild(&labelWrapper, label)

    var button = AzButton_create(azString("Increase counter"))
    AzButton_setButtonType(&button, AzButtonType_Primary)
    let dataClone = AzRefAny_clone(&d)
    AzButton_setOnClick(&button, dataClone, onClick)
    let buttonDom = AzButton_dom(button)

    var body = AzDom_createBody()
    AzDom_addChild(&body, labelWrapper)
    AzDom_addChild(&body, buttonDom)
    return body
}

// `@main`: top-level statements are only allowed in a file named `main.swift`.

@main
struct HelloWorld {
    static func main() {
        let model = MyDataModel(counter: 5)
        let data = myDataUpcast(model)

        var window = AzWindowCreateOptions_create(layout)
        window.window_state.title = azString("Hello World")
        window.window_state.size.dimensions.width = 400.0
        window.window_state.size.dimensions.height = 300.0

        window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
        window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

        var app = AzApp_create(data, AzAppConfig_create())
        AzApp_run(&app, window)
    }
}
