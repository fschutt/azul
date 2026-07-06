# nim c -d:release hello-world.nim && LD_LIBRARY_PATH=. ./hello-world
#   (macOS: DYLD_LIBRARY_PATH=. ./hello-world ; Windows: azul.dll in cwd)
#
# azul.nim imports libazul through `{.importc, cdecl, dynlib: azulLib.}`, so
# the library is dlopen'd at run time — no link flags are required, the .so /
# .dylib / .dll just has to be discoverable (hence the *_LIBRARY_PATH=.).
#
# Nim is C-ABI-direct: a top-level `proc (...) {.cdecl.}` is a real C function
# pointer, so `onClick` / `layout` below are handed straight to Azul with no
# trampoline, host-invoker, or handle table. This mirrors examples/zig and
# examples/c one-for-one.

import azul

# ── Data model ────────────────────────────────────────────────────────────
#
# Same shape as the C `AZ_REFLECT_JSON(MyDataModel, ...)` macro:
#   1. a compile-time-unique type id (the address of a global we never read),
#   2. an `upcast` that copies the struct into an AzRefAny,
#   3. a `downcast` that recovers a typed pointer from the refany.
# Plain old data → empty destructor.

type
  MyDataModel = object
    counter: uint32

var myDataTypeToken: uint8 = 0
proc myDataTypeId(): uint64 = cast[uint64](addr myDataTypeToken)

proc myDataDestructor(p: pointer) {.cdecl.} = discard

# Build an AzString from a Nim string (copies the bytes into libazul).
proc azStr(s: string): AzString =
  if s.len == 0:
    AzString_fromUtf8(nil, csize_t(0))
  else:
    AzString_fromUtf8(cast[ptr uint8](s.cstring), csize_t(s.len))

proc myDataUpcast(model: MyDataModel): AzRefAny =
  # AzRefAny_newC copies `len` bytes out of `ptr.ptr` into its own heap
  # allocation, so handing it a stack pointer is fine. run_destructor = false
  # means libazul won't try to free the caller's pointer — only the heap copy
  # is released (via myDataDestructor) when the last clone drops.
  var local = model
  let blob = AzGlVoidPtrConst(`ptr`: cast[pointer](addr local), run_destructor: false)
  AzRefAny_newC(
    blob,
    csize_t(sizeof(MyDataModel)),
    csize_t(alignof(MyDataModel)),
    myDataTypeId(),
    azStr("MyDataModel"),
    myDataDestructor,
    csize_t(0),   # no serialize_fn
    csize_t(0))   # no deserialize_fn

proc myDataDowncast(refany: ptr AzRefAny): ptr MyDataModel =
  if not AzRefAny_isType(refany, myDataTypeId()):
    return nil
  let p = AzRefAny_getDataPtr(refany)
  if p == nil:
    return nil
  cast[ptr MyDataModel](p)

# ── Callback: button click ────────────────────────────────────────────────

proc onClick(data: AzRefAny, info: AzCallbackInfo): AzUpdate {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil:
    return AzUpdate_DoNothing
  m.counter += 1
  return AzUpdate_RefreshDom

# ── Layout callback ───────────────────────────────────────────────────────

proc layout(data: AzRefAny, info: AzLayoutCallbackInfo): AzDom {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil:
    return AzDom_createBody()

  # Counter label, wrapped in a div so the font-size sticks.
  let label = AzDom_createText(azStr($m.counter))
  var labelWrapper = AzDom_createDiv()
  let cond = AzCssPropertyWithConditions_simple(
    AzCssProperty_fontSize(AzStyleFontSize_px(32.0'f32)))
  AzDom_addCssProperty(addr labelWrapper, cond)
  AzDom_addChild(addr labelWrapper, label)

  # Increment button. The typed AzButton_setOnClick takes the bare
  # AzButtonOnClickCallbackType fn pointer directly — `onClick` is exactly
  # that, so it passes straight through.
  var button = AzButton_create(azStr("Increase counter"))
  AzButton_setButtonType(addr button, AzButtonType_Primary)
  let dataClone = AzRefAny_clone(addr d)
  AzButton_setOnClick(addr button, dataClone, onClick)
  let buttonDom = AzButton_dom(button)

  var body = AzDom_createBody()
  AzDom_addChild(addr body, labelWrapper)
  AzDom_addChild(addr body, buttonDom)
  return body

# ── Main ──────────────────────────────────────────────────────────────────

proc main() =
  let model = MyDataModel(counter: 5)
  let data = myDataUpcast(model)

  var window = AzWindowCreateOptions_create(layout)
  window.window_state.title = azStr("Hello World")
  window.window_state.size.dimensions.width = 400.0'f32
  window.window_state.size.dimensions.height = 300.0'f32
  # NoTitleAutoInject: the OS draws the close/min/max buttons and the framework
  # auto-injects a draggable titlebar.
  window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
  window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

  var app = AzApp_create(data, AzAppConfig_create())
  AzApp_run(addr app, window)
  AzApp_delete(addr app)

main()
