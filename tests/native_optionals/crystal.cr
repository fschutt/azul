# Crystal: `nil` where an Option is asked for, `T?` back from fields and
# downcasts, a raised `Azul::ResultError` for an Err (inside a callback the
# trampoline logs it and the callback does nothing). Built and run by
# scripts/native_optionals_smoke.sh.
require "azul"

class Model
  property folder = ""
  property link = "no scheme"
end

# The guide's file-dialog example, as written there (hello-world/crystal.md).
def on_choose_folder(model : Model, info : Azul::CallbackInfo) : Azul::Update
  Azul::FileDialog.open_directory("Choose a folder", nil, model, ->on_pick_folder(Model, Azul::CallbackInfo, Azul::RefAny))
  Azul::Update::DoNothing
end

def on_pick_folder(model : Model, info : Azul::CallbackInfo, result : Azul::RefAny) : Azul::Update
  path = Azul::FileOpenResult.downcast(result).try(&.path)
  return Azul::Update::DoNothing unless path # not a folder pick, or cancelled
  model.folder = path.inner
  Azul::Update::RefreshDom
end

def on_open_link(model : Model, info : Azul::CallbackInfo) : Azul::Update
  url = Azul::Url.parse(model.link) # an Err is logged, the callback does nothing
  Azul::Update::RefreshDom
end

def check(ok : Bool, what : String)
  unless ok
    STDERR.puts "FAIL: #{what}"
    exit 1
  end
end

# A field: nil in, nil out; a value in, the value out.
icons = Azul::IconVariants.new
check icons.light.nil?, "a default IconVariants has no light icon"
icons.light = "light.png"
check icons.light == "light.png", "Some in, Some out"
icons.light = nil
check icons.light.nil?, "None in, None out"

# A result: Ok is the value, Err a raised error.
Azul::Url.parse("https://azul.rs/guide")
threw = false
begin
  Azul::Url.parse("no scheme")
rescue Azul::ResultError(Azul::UrlParseError)
  threw = true
end
check threw, "Url.parse Err raises"

clicked = ->on_choose_folder(Model, Azul::CallbackInfo)
opened = ->on_open_link(Model, Azul::CallbackInfo)
puts "crystal native optionals: ok"
