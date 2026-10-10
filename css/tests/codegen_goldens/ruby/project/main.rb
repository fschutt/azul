# Copy target/codegen/azul.rb and libazul next to this file, then: ruby -I. main.rb
require_relative 'styles'

style_btn = AzulStyles.style_btn
puts "style_btn: #{style_btn[:len]} properties"
