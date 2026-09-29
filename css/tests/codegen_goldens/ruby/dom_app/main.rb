# Copy target/codegen/azul.rb and libazul next to this file, then: ruby -I. main.rb
require_relative 'styles'

render_ui = AzulStyles.render_ui
puts "render_ui: #{render_ui[:len]} properties"
