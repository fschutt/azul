# frozen_string_literal: true
#
# examples/ruby/hello-world.rb
#
# Ruby port of examples/c/hello-world.c. Same data model (a counter),
# same behaviour (mouse click increments, layout rebuilds the DOM).
# Callbacks go through libazul's host-invoker plumbing — ruby-ffi never
# has to synthesize a struct-by-value trampoline for user code.
#
# Run with:
#     ruby -I. hello-world.rb
#
# Requires the `ffi` gem (`gem install --user-install ffi -v 1.15.5`
# on system Ruby 2.6 on macOS).

require 'azul'

# ── Helpers ───────────────────────────────────────────────────────────
# AzString constructor: copy a Ruby String into an AzString wrapper.
# The auto-generated wrapper doesn't yet accept Ruby Strings directly.
def az_str(s)
  bytes = s.encode(Encoding::UTF_8).bytes
  buf = FFI::MemoryPointer.new(:uint8, bytes.size)
  buf.write_array_of_uint8(bytes)
  Azul::String.new(Azul::Native.az_string_from_utf8(buf, bytes.size))
end

# ── Data model ────────────────────────────────────────────────────────
class MyDataModel
  attr_accessor :counter
  def initialize(counter)
    @counter = counter
  end
end

model = MyDataModel.new(5)
data  = Azul::RefAny.wrap(model)

# ── Callbacks ─────────────────────────────────────────────────────────

on_click = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next 0 if m.nil? # Update::DoNothing
  m.counter += 1
  1 # Update::RefreshDom
end

layout = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next Azul::Dom.create_body if m.nil?

  # Counter label wrapped in a font-size-32 div. Consuming `with_*`
  # builders so the codegen's `Azul._consume(...)` calls keep the
  # ObjectSpace finalizers from double-freeing on builder-chain moves.
  #
  # `CssProperty` is a tagged-union without a Ruby wrapper class today;
  # we use the Native form directly. The result is an FFI::Struct that
  # `CssPropertyWithConditions.simple` accepts.
  font_size_px = Azul::Native.az_style_font_size_px(32.0)
  font_size_prop = Azul::Native.az_css_property_font_size(font_size_px)
  label = Azul::Dom.create_div
    .with_css_property(Azul::CssPropertyWithConditions.simple(font_size_prop))
    .with_child(Azul::Dom.create_text(az_str(m.counter.to_s)))

  # Increment button.
  button = Azul::Button.create(az_str('Increase counter'))
    .with_button_type(Azul::Native::AzButtonType::Primary)
    .with_on_click(Azul::Native.az_ref_any_clone(data), on_click)

  Azul::Dom.create_body
    .with_child(label)
    .with_child(Azul::Dom.new(button.dom))
end

# ── Main ──────────────────────────────────────────────────────────────
#
# Window setup uses `WindowCreateOptions.default()` + field assignment
# rather than `WindowCreateOptions.create(layout)`, because the latter
# routes through `AzWindowCreateOptions_create(AzLayoutCallbackType)`,
# which takes a raw fn pointer and discards the host-invoker ctx
# carrying our dispatch handle. Polish item for follow-up codegen pass.
window = Azul::WindowCreateOptions.default
window.ptr[:window_state][:layout_callback] = Azul._register_callback('LayoutCallback', layout)
window.ptr[:window_state][:title] = az_str('Hello World').ptr
window.ptr[:window_state][:size][:dimensions][:width]  = 400.0
window.ptr[:window_state][:size][:dimensions][:height] = 300.0
# NoTitleAutoInject: OS draws close/min/max buttons; framework
# auto-injects a Titlebar with drag support.
window.ptr[:window_state][:flags][:decorations]         = 1 # WindowDecorations::NoTitleAutoInject
window.ptr[:window_state][:flags][:background_material] = 3 # WindowBackgroundMaterial::Sidebar

app = Azul::App.create(data, Azul::AppConfig.create)
app.run(window.ptr)
