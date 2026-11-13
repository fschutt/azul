# Ruby: nil where an Option is asked for, nil or the value back from fields
# and downcasts (so `&.` reads a chain), a raised error for an Err (inside a
# callback the binding logs it and the callback does nothing), and the
# resume callback's result as a RefAny `downcast` takes. Run by
# scripts/native_optionals_smoke.sh.
require_relative 'azul'

Model = Struct.new(:folder, :link)

# The guide's file-dialog example, as written there (hello-world/ruby.md).
on_pick_folder = lambda do |model, _info, result|
  path = Azul::FileOpenResult.downcast(result)&.path
  next Azul::Update::DoNothing if path.nil? # not a folder pick, or cancelled
  model.folder = path.inner
  Azul::Update::RefreshDom
end

on_choose_folder = lambda do |model, _info|
  Azul::FileDialog.open_directory('Choose a folder', nil, model, on_pick_folder)
  Azul::Update::DoNothing
end

on_open_link = lambda do |model, _info|
  Azul::Url.parse(model.link) # an Err raises: logged, the callback does nothing
  Azul::Update::RefreshDom
end

def check(ok, what)
  return if ok
  warn "FAIL: #{what}"
  exit 1
end

# A field: nil in, nil out; a value in, the value out.
icons = Azul::IconVariants.create_default
check(icons.light.nil?, 'a default IconVariants has no light icon')
icons.light = 'light.png'
check(icons.light == 'light.png', 'Some in, Some out')
icons.light = nil
check(icons.light.nil?, 'None in, None out')

# A class payload: a Dom moved in, a Dom read back, nil clears it.
view = Azul::VirtualViewReturn.create_default
check(view.dom.nil?, 'a default VirtualViewReturn has no Dom')
view.dom = Azul::Dom.create_body
check(!view.dom.nil?, 'a Dom moved into an Option field reads back')
view.dom = nil
check(view.dom.nil?, 'nil clears the field')

# A result: Ok is the value, Err a raised error.
check(!Azul::Url.parse('https://azul.rs/guide').nil?, 'Url.parse Ok')
threw = false
begin
  Azul::Url.parse('no scheme')
rescue StandardError
  threw = true
end
check(threw, 'Url.parse Err raises')

Azul::Button.create('Open').on_click(Model.new('', 'no scheme'), on_open_link)
check(on_choose_folder.arity == 2, 'the guide callbacks exist')
puts 'ruby native optionals: ok'
