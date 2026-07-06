// Azul counter example — V (vlang).
//
// Build (libazul on the link path, generated binding in ./azul/azul.v):
//   v run .

module main

import azul

struct MyDataModel {
mut:
	counter u32
}

// Any fixed, process-unique u64 works as the RefAny type id (the C
// AZ_REFLECT macro uses a global's address; a constant is simpler in V).
const my_data_type_id = u64(0x617a756c5f6d646d) // "azul_mdm"

fn my_data_destructor(ptr voidptr) {
	// MyDataModel is plain old data: nothing to free.
}

// Build an AzString from a V string (copies the bytes into libazul).
fn az_str(s string) azul.AzString {
	return C.AzString_fromUtf8(s.str, usize(s.len))
}

fn my_data_upcast(model MyDataModel) azul.AzRefAny {
	// AzRefAny_newC copies the bytes into its own allocation, so a stack
	// pointer is fine; run_destructor = false = don't free the caller's ptr.
	mut local := model
	type_name := az_str('MyDataModel')
	blob := azul.AzGlVoidPtrConst{
		ptr:            voidptr(&local)
		run_destructor: false
	}
	return C.AzRefAny_newC(
		blob,
		usize(sizeof(MyDataModel)),
		usize(4), // align of a u32-only POD struct
		my_data_type_id,
		type_name,
		my_data_destructor,
		usize(0), // no serialize_fn
		usize(0), // no deserialize_fn
	)
}

fn my_data_downcast(refany &azul.AzRefAny) &MyDataModel {
	if !C.AzRefAny_isType(refany, my_data_type_id) {
		return unsafe { nil }
	}
	ptr := C.AzRefAny_getDataPtr(refany)
	if isnil(ptr) {
		return unsafe { nil }
	}
	return unsafe { &MyDataModel(ptr) }
}

// Top-level V fns compile to real C functions, so their addresses go
// straight to the C-ABI setters — no host-invoker.
fn on_click(data azul.AzRefAny, info azul.AzCallbackInfo) azul.AzUpdate {
	mut d := data
	m := my_data_downcast(&d)
	if isnil(m) {
		return azul.AzUpdate.DoNothing
	}
	unsafe {
		m.counter++
	}
	return azul.AzUpdate.RefreshDom
}

fn layout(data azul.AzRefAny, info azul.AzLayoutCallbackInfo) azul.AzDom {
	mut d := data
	m := my_data_downcast(&d)
	if isnil(m) {
		return C.AzDom_createBody()
	}

	// Counter label (wrapped in a div so the font-size sticks).
	counter_val := unsafe { m.counter }
	counter_str := az_str(counter_val.str())
	label := C.AzDom_createText(counter_str)

	mut label_wrapper := C.AzDom_createDiv()
	font_size := C.AzStyleFontSize_px(32.0)
	css_prop := C.AzCssProperty_fontSize(font_size)
	cond := C.AzCssPropertyWithConditions_simple(css_prop)
	C.AzDom_addCssProperty(&label_wrapper, cond)
	C.AzDom_addChild(&label_wrapper, label)

	// AzButton_setOnClick takes the bare fn-pointer typedef directly.
	btn_label := az_str('Increase counter')
	mut button := C.AzButton_create(btn_label)
	C.AzButton_setButtonType(&button, azul.AzButtonType.Primary)
	data_clone := C.AzRefAny_clone(&d)
	C.AzButton_setOnClick(&button, data_clone, on_click)
	button_dom := C.AzButton_dom(button)

	mut body := C.AzDom_createBody()
	C.AzDom_addChild(&body, label_wrapper)
	C.AzDom_addChild(&body, button_dom)
	return body
}

fn main() {
	model := MyDataModel{
		counter: 5
	}
	data := my_data_upcast(model)

	mut window := C.AzWindowCreateOptions_create(layout)
	window.window_state.title = az_str('Hello World')
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0

	// NoTitleAutoInject: OS draws the window buttons; the framework
	// auto-injects a draggable titlebar.
	window.window_state.flags.decorations = azul.AzWindowDecorations.NoTitleAutoInject
	window.window_state.flags.background_material = azul.AzWindowBackgroundMaterial.Sidebar

	mut app := C.AzApp_create(data, C.AzAppConfig_create())
	C.AzApp_run(&app, window)
}
