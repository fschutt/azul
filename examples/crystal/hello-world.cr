require "./azul"

module MyData
  struct Model
    property counter : UInt32

    def initialize(@counter : UInt32)
    end
  end

  TOKEN = Pointer(UInt8).malloc(1)

  def self.type_id : UInt64
    TOKEN.address.to_u64
  end

  DESTRUCTOR = ->(_ptr : Void*) { }

  def self.upcast(model : Model) : LibAzul::AzRefAny
    # AzRefAny_newC copies the bytes into its own allocation, so a stack
    # local is fine; run_destructor=false = don't free the caller's ptr.
    local = model
    type_name = "Model"
    name = LibAzul.azString_fromUtf8(type_name.to_unsafe, LibC::SizeT.new(type_name.bytesize))

    wrapper = LibAzul::AzGlVoidPtrConst.new
    wrapper.ptr = pointerof(local).as(Void*)
    wrapper.run_destructor = false

    LibAzul.azRefAny_newC(
      wrapper,
      LibC::SizeT.new(sizeof(Model)),
      LibC::SizeT.new(alignof(Model)),
      type_id,
      name,
      DESTRUCTOR,
      LibC::SizeT.new(0), # no serialize_fn
      LibC::SizeT.new(0)  # no deserialize_fn
    )
  end

  def self.downcast(refany : LibAzul::AzRefAny*) : Model*
    return Pointer(Model).null unless LibAzul.azRefAny_isType(refany, type_id)
    ptr = LibAzul.azRefAny_getDataPtr(refany)
    return Pointer(Model).null if ptr.null?
    ptr.as(Model*)
  end
end

# Non-capturing proc → bare C function pointer.
ON_CLICK = ->(data : LibAzul::AzRefAny, _info : LibAzul::AzCallbackInfo) : LibAzul::AzUpdate {
  d = data
  m = MyData.downcast(pointerof(d))
  next LibAzul::AzUpdate::DoNothing if m.null?
  m.value.counter += 1
  LibAzul::AzUpdate::RefreshDom
}

LAYOUT = ->(data : LibAzul::AzRefAny, _info : LibAzul::AzLayoutCallbackInfo) : LibAzul::AzDom {
  d = data
  m = MyData.downcast(pointerof(d))
  next LibAzul.azDom_createBody if m.null?

  text = m.value.counter.to_s
  counter_str = LibAzul.azString_fromUtf8(text.to_unsafe, LibC::SizeT.new(text.bytesize))
  label = LibAzul.azDom_createTextDoNotUseWithoutBlockLevelWrapper(counter_str)

  label_wrapper = LibAzul.azDom_createDiv
  font_size = LibAzul.azStyleFontSize_px(32.0_f32)
  css_prop = LibAzul.azCssProperty_fontSize(font_size)
  cond = LibAzul.azCssPropertyWithConditions_simple(css_prop)
  LibAzul.azDom_addCssProperty(pointerof(label_wrapper), cond)
  LibAzul.azDom_addChild(pointerof(label_wrapper), label)

  btn_label = "Increase counter"
  button = LibAzul.azButton_create(
    LibAzul.azString_fromUtf8(btn_label.to_unsafe, LibC::SizeT.new(btn_label.bytesize))
  )
  LibAzul.azButton_setButtonType(pointerof(button), LibAzul::AzButtonType::Primary)
  data_clone = LibAzul.azRefAny_clone(pointerof(d))
  LibAzul.azButton_setOnClick(pointerof(button), data_clone, ON_CLICK)
  button_dom = LibAzul.azButton_dom(button)

  body = LibAzul.azDom_createBody
  LibAzul.azDom_addChild(pointerof(body), label_wrapper)
  LibAzul.azDom_addChild(pointerof(body), button_dom)
  body
}

model = MyData::Model.new(5_u32)
data = MyData.upcast(model)

window = LibAzul.azWindowCreateOptions_create(LAYOUT)
title = "Hello World"
window.window_state.title = LibAzul.azString_fromUtf8(title.to_unsafe, LibC::SizeT.new(title.bytesize))
window.window_state.size.dimensions.width = 400.0_f32
window.window_state.size.dimensions.height = 300.0_f32

window.window_state.flags.decorations = LibAzul::AzWindowDecorations::NoTitleAutoInject
window.window_state.flags.background_material = LibAzul::AzWindowBackgroundMaterial::Sidebar

app = LibAzul.azApp_create(data, LibAzul.azAppConfig_create)
LibAzul.azApp_run(pointerof(app), window)
