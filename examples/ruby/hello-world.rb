# frozen_string_literal: true
#
# examples/ruby/hello-world.rb — Python-quality Ruby port.
#
# Uses the smart `WindowCreateOptions.create_with_layout(&block)`
# factory and `Button#on_click(data, &block)` smart builder. User code
# never has to call `Azul._register_callback` directly.
#
# Run:  ruby -I. hello-world.rb
# Reqs: `ffi` gem.

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

  # CssProperty is a tagged-union without a Ruby wrapper class today;
  # use the Native form directly.
  font_size_px = Azul::Native.az_style_font_size_px(32.0)
  font_size_prop = Azul::Native.az_css_property_font_size(font_size_px)
  label = Azul::Dom.create_div
    .with_css_property(Azul::CssPropertyWithConditions.simple(font_size_prop))
    .with_child(Azul::Dom.create_text(m.counter.to_s))

  # Smart .on_click(data, &block) wraps refany + register internally.
  button = Azul::Button.create('Increase counter')
    .with_button_type(Azul::ButtonType::Primary)
    .on_click(m, on_click)

  Azul::Dom.create_body
    .with_child(label)
    .with_child(Azul::Dom.new(button.dom))
end

# Smart factory: hides the manual layout_callback splice. Window
# state extras (title, size, flags) still hang off the wrapper's
# raw ptr like before.
window = Azul::WindowCreateOptions.create_with_layout(layout)
window.ptr[:window_state][:title] = Azul._az_string('Hello World')
window.ptr[:window_state][:size][:dimensions][:width]  = 400.0
window.ptr[:window_state][:size][:dimensions][:height] = 300.0
window.ptr[:window_state][:flags][:decorations]         = Azul::WindowDecorations::NoTitleAutoInject
window.ptr[:window_state][:flags][:background_material] = Azul::WindowBackgroundMaterial::Sidebar

app = Azul::App.create(data, Azul::AppConfig.create)
app.run(window.ptr)
