# Memory test for the azul Ruby (ffi) binding. See tests/memtest/README.md.
#
# The harness measures RSS/gdb externally; this file just exercises the
# create/consume/DROP paths in a loop and exits 0. No event loop (App#run
# needs a display and hangs headless). Native objects are freed by
# ObjectSpace finalizers, so the loop drops references and nudges the GC.
#
# Run (matches examples/ruby/hello-world.rb):
#   ruby -I. mem_test.rb

require 'azul'

class MyDataModel
  attr_accessor :counter
  def initialize(counter); @counter = counter; end
end

n = (ENV['AZ_MEMTEST_N'] || '200000').to_i

# 1. The consume-by-value DROP path: App.create consumes the RefAny +
#    AppConfig. Build it without running (no window), then let it drop.
app = Azul::App.create(Azul::RefAny.wrap(MyDataModel.new(5)), Azul::AppConfig.create)
app = nil
GC.start

# 2. Leak loop: create/destroy droppable objects N times. The FFI wrappers
#    free the native memory on GC, so drop the refs and nudge the collector.
n.times do |i|
  cfg = Azul::AppConfig.create
  dom = Azul::Dom.create_body
  cfg = nil
  dom = nil
  GC.start if (i & 0x3fff).zero?
end
GC.start

puts "memtest ruby OK (N=#{n})"
exit 0
