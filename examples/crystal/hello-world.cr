# Hello World: a counter and a button that increments it.
#
# Build (with libazul next to this file):
#   crystal build hello-world.cr --link-flags "-L."
require "azul"

# The application state is an ordinary Crystal object. libazul keeps it alive
# and hands it back to every callback with its own type.
class Counter
  property count : Int32

  def initialize(@count = 5)
  end
end

def layout(counter : Counter, info : Azul::LayoutCallbackInfo) : Azul::Dom
  label = Azul::Dom.p_with_text(counter.count.to_s)
    .with_css("font-size: 32px; margin: 0;")

  button = Azul::Button.new("Increase counter")
    .with_button_type(:primary)
    .with_on_click(counter) do |counter, _info|
      counter.count += 1
      Azul::Update::RefreshDom
    end

  Azul::Dom.body
    .with_child(label)
    .with_child(button.dom)
end

window = Azul::WindowCreateOptions.new(->layout(Counter, Azul::LayoutCallbackInfo))
window.window_state.title = "Hello World"
window.window_state.size.dimensions.width = 400
window.window_state.size.dimensions.height = 300

app = Azul::App.new(Counter.new, Azul::AppConfig.new)
app.run(window)
