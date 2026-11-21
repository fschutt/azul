// Copy target/codegen/azul.v to ./azul/ and libazul here, then: v run .
module main

fn main() {
	style_btn_value := style_btn()
	println('style_btn: ${style_btn_value.len} properties')
}
