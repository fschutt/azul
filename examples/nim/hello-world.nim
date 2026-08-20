import azul

type
  MyDataModel = object
    counter: uint32

var myDataTypeToken: uint8 = 0
proc myDataTypeId(): uint64 = cast[uint64](addr myDataTypeToken)

proc myDataDestructor(p: pointer) {.cdecl.} = discard

proc azStr(s: string): AzString =
  if s.len == 0:
    AzString_fromUtf8(nil, csize_t(0))
  else:
    AzString_fromUtf8(cast[ptr uint8](s.cstring), csize_t(s.len))

proc myDataUpcast(model: MyDataModel): AzRefAny =
  # newC copies the bytes into its own allocation, so a stack pointer is fine;
  # run_destructor = false means libazul won't free the caller's pointer.
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

proc onClick(data: AzRefAny, info: AzCallbackInfo): AzUpdate {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil:
    return AzUpdate.DoNothing
  m.counter += 1
  return AzUpdate.RefreshDom

proc layout(data: AzRefAny, info: AzLayoutCallbackInfo): AzDom {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil:
    return AzDom_createBody()

  let label = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(azStr($m.counter))
  var labelWrapper = AzDom_createDiv()
  let cond = AzCssPropertyWithConditions_simple(
    AzCssProperty_fontSize(AzStyleFontSize_px(32.0'f32)))
  AzDom_addCssProperty(addr labelWrapper, cond)
  AzDom_addChild(addr labelWrapper, label)

  var button = AzButton_create(azStr("Increase counter"))
  AzButton_setButtonType(addr button, AzButtonType.Primary)
  let dataClone = AzRefAny_clone(addr d)
  AzButton_setOnClick(addr button, dataClone, onClick)
  let buttonDom = AzButton_dom(button)

  var body = AzDom_createBody()
  AzDom_addChild(addr body, labelWrapper)
  AzDom_addChild(addr body, buttonDom)
  return body

proc main() =
  let model = MyDataModel(counter: 5)
  let data = myDataUpcast(model)

  var window = AzWindowCreateOptions_create(layout)
  window.window_state.title = azStr("Hello World")
  window.window_state.size.dimensions.width = 400.0'f32
  window.window_state.size.dimensions.height = 300.0'f32
  window.window_state.flags.decorations = AzWindowDecorations.NoTitleAutoInject
  window.window_state.flags.background_material = AzWindowBackgroundMaterial.Sidebar

  var app = AzApp_create(data, AzAppConfig_create())
  AzApp_run(addr app, window)
  AzApp_delete(addr app)

main()
