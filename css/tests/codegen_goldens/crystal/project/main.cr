# Copy target/codegen/crystal to ./azul and libazul here, then:
#   shards install && crystal run main.cr --link-flags=-L.
require "./styles"

style_btn = AzulStyles.style_btn
puts "style_btn: #{style_btn.size} properties"
