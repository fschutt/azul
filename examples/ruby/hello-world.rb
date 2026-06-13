# frozen_string_literal: true
#
# Run: ruby -I. hello-world.rb   (requires the `ffi` gem)

require 'azul'

class MyDataModel
  attr_accessor :counter
  def initialize(counter); @counter = counter; end
end

model = MyDataModel.new(5)
data  = Azul::RefAny.wrap(model)

on_click = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next Azul::Update::DoNothing if m.nil?
  m.counter += 1
  Azul::Update::RefreshDom
end

layout = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next Azul::Dom.create_body if m.nil?

  font_size_px = Azul::Native.az_style_font_size_px(32.0)
  font_size_prop = Azul::Native.az_css_property_font_size(font_size_px)
  label = Azul::Dom.create_div
    .with_css_property(Azul::CssPropertyWithConditions.simple(font_size_prop))
    .with_child(Azul::Dom.create_text(m.counter.to_s))

  button = Azul::Button.create('Increase counter')
    .with_button_type(Azul::ButtonType::Primary)
    .on_click(m, on_click)

  Azul::Dom.create_body
    .with_child(label)
    .with_child(Azul::Dom.new(button.dom))
end

window = Azul::WindowCreateOptions.create_with_layout(layout).with(
  window_state: {
    title: 'Hello World',
    size: { dimensions: { width: 400.0, height: 300.0 } },
    flags: {
      decorations: Azul::WindowDecorations::NoTitleAutoInject,
      background_material: Azul::WindowBackgroundMaterial::Sidebar,
    },
  },
)

app = Azul::App.create(data, Azul::AppConfig.create)
app.run(window.ptr)
