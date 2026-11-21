// Copy target/codegen/azul.odin to ./azul/ and libazul here, then:
//   odin run . -extra-linker-flags:"-L."
package main

import "core:fmt"

main :: proc() {
	style_btn_value := style_btn()
	fmt.println("style_btn:", style_btn_value.len, "properties")
}
