// Swift: `nil` where an Option is asked for, `T?` back from fields and
// downcasts, a thrown error for an Err - and a callback may `try`, its error
// logged and the callback doing nothing. Built and run by
// scripts/native_optionals_smoke.sh.
import Azul

final class Model {
    var folder = ""
    var link = "no scheme"
}

// The guide's file-dialog example, as written there (hello-world/swift.md).
func onChooseFolder(_ model: Model, _ info: CallbackInfo) -> Update {
    _ = FileDialog.openDirectory("Choose a folder", defaultPath: nil, data: model, onResult: onPickFolder)
    return .doNothing
}

func onPickFolder(_ model: Model, _ info: CallbackInfo, _ result: RefAny) -> Update {
    guard let path = FileOpenResult.downcast(result)?.path else {
        return .doNothing // not a folder pick, or cancelled
    }
    model.folder = path.inner
    return .refreshDom
}

func onOpenLink(_ model: Model, _ info: CallbackInfo) throws -> Update {
    let url = try Url.parse(model.link) // an Err is logged, the callback does nothing
    model.link = url.toString()
    return .refreshDom
}

func check(_ ok: Bool, _ what: String) {
    if !ok {
        fatalError("FAIL: \(what)")
    }
}

// A field: nil in, nil out; a value in, the value out.
let icons = IconVariants()
check(icons.light == nil, "a default IconVariants has no light icon")
icons.light = "light.png"
check(icons.light == "light.png", "Some in, Some out")
icons.light = nil
check(icons.light == nil, "None in, None out")

// A result: Ok is the value, Err a thrown error.
check((try? Url.parse("https://azul.rs/guide")) != nil, "Url.parse Ok")
do {
    _ = try Url.parse("no scheme")
    check(false, "Url.parse Err throws")
} catch {}

// The callbacks have the types the API asks for.
let _: (Model, CallbackInfo) throws -> Update = onChooseFolder
let _: (Model, CallbackInfo, RefAny) throws -> Update = onPickFolder
let _: (Model, CallbackInfo) throws -> Update = onOpenLink
let _ = Button("Open").withOnClick(Model(), onClick: onOpenLink)
print("swift native optionals: ok")
