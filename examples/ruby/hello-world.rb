# frozen_string_literal: true
#
# Minimal Ruby smoke test for the Azul host-invoker plumbing. Confirms
# that the ffi gem loads, the dylib initialises, and the host-invoker
# init phase (RefAny.wrap / RefAny.unwrap) round-trips a managed object.
#
# Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
# the wrapper layer's idiomatic API surface to settle — separate work,
# not host-invoker. Same shape as the C# / Java / Kotlin / Node /
# PowerShell hello-worlds.
#
# Run with:
#     ruby -I. hello-world.rb   # azul.rb + libazul.dylib in same dir
#
# Requires the `ffi` gem (`gem install --user-install ffi -v 1.15.5`
# for system Ruby 2.6 on macOS).

require 'azul'

class MyDataModel
  attr_accessor :counter
  def initialize(counter)
    @counter = counter
  end
end

model = MyDataModel.new(5)
data  = Azul::RefAny.wrap(model)
puts '[azul] RefAny.wrap ran; opaque-handle id stored.'

recovered = Azul::RefAny.unwrap(data)
if recovered.is_a?(MyDataModel) && recovered.counter == 5
  puts "[azul] RefAny.unwrap round-trip succeeded; counter=#{recovered.counter}"
else
  puts "[azul] RefAny.unwrap round-trip FAILED (recovered=#{recovered.inspect})"
  exit 1
end

puts '[azul] host-invoker init phase completed successfully.'
puts '[azul] (Full App.run wiring requires wrapper-layer API surface'
puts '[azul]  fixes that are separate from the host-invoker plumbing.)'
