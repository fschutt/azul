// Azul counter example — Odin.
//
// Build: odin build . -out:hello-world -extra-linker-flags:"-L."
// (needs libazul on the link path and the generated binding in ./azul/azul.odin)

package main

import azul "azul"

// Data model wrapped in an AzRefAny: a process-unique type id (address of a
// global we never read), an upcast that copies the struct in, and a downcast.

MyDataModel :: struct {
	counter: u32,
}

MY_DATA_TYPE_TOKEN: u8 = 0

my_data_type_id :: proc "contextless" () -> u64 {
	return u64(uintptr(&MY_DATA_TYPE_TOKEN))
}

my_data_destructor :: proc "c" (_: rawptr) {
}

my_data_upcast :: proc(model: MyDataModel) -> azul.AzRefAny {
	// newC copies the bytes into its own allocation, so a stack pointer is
	// fine; run_destructor=false means libazul won't free the caller's pointer.
	local := model
	type_name_bytes := "MyDataModel"
	type_name := azul.AzString_fromUtf8(raw_data(type_name_bytes), uint(len(type_name_bytes)))
	ptr_wrapper := azul.AzGlVoidPtrConst{ ptr = &local, run_destructor = false }
	return azul.AzRefAny_newC(
		ptr_wrapper,
		uint(size_of(MyDataModel)),
		uint(align_of(MyDataModel)),
		my_data_type_id(),
		type_name,
		my_data_destructor,
		0, // no serialize_fn
		0, // no deserialize_fn
	)
}

my_data_downcast :: proc "contextless" (refany: ^azul.AzRefAny) -> ^MyDataModel {
	if !azul.AzRefAny_isType(refany, my_data_type_id()) {
		return nil
	}
	ptr := azul.AzRefAny_getDataPtr(refany)
	if ptr == nil {
		return nil
	}
	return cast(^MyDataModel)ptr
}

// Click callback — must be `proc "c"` so it is a bare C function pointer.

on_click :: proc "c" (data: azul.AzRefAny, info: azul.AzCallbackInfo) -> azul.AzUpdate {
	d := data
	m := my_data_downcast(&d)
	if m == nil {
		return azul.AzUpdate.DoNothing
	}
	m.counter += 1
	return azul.AzUpdate.RefreshDom
}

// Contextless u32 -> decimal, so `layout` needs no Odin `context`.
u32_write :: proc "contextless" (n: u32, buf: []u8) -> int {
	if n == 0 {
		buf[0] = '0'
		return 1
	}
	tmp: [10]u8
	i := 0
	v := n
	for v > 0 {
		tmp[i] = u8('0') + u8(v % 10)
		v /= 10
		i += 1
	}
	j := 0
	for j < i {
		buf[j] = tmp[i - 1 - j]
		j += 1
	}
	return i
}

// Layout callback — `proc "c"`, re-run on every RefreshDom.
layout :: proc "c" (data: azul.AzRefAny, info: azul.AzLayoutCallbackInfo) -> azul.AzDom {
	d := data
	m := my_data_downcast(&d)
	if m == nil {
		return azul.AzDom_createBody()
	}

	// Counter label (wrapped in a div so the font-size sticks).
	buf: [16]u8
	n := u32_write(m.counter, buf[:])
	counter_str := azul.AzString_fromUtf8(raw_data(buf[:]), uint(n))
	label := azul.AzDom_createText(counter_str)

	label_wrapper := azul.AzDom_createDiv()
	font_size := azul.AzStyleFontSize_px(32.0)
	css_prop := azul.AzCssProperty_fontSize(font_size)
	cond := azul.AzCssPropertyWithConditions_simple(css_prop)
	azul.AzDom_addCssProperty(&label_wrapper, cond)
	azul.AzDom_addChild(&label_wrapper, label)

	// Increment button. AzButton_setOnClick takes the bare fn pointer directly.
	btn_label_bytes := "Increase counter"
	btn_label := azul.AzString_fromUtf8(raw_data(btn_label_bytes), uint(len(btn_label_bytes)))
	button := azul.AzButton_create(btn_label)
	azul.AzButton_setButtonType(&button, azul.AzButtonType.Primary)
	data_clone := azul.AzRefAny_clone(&d)
	azul.AzButton_setOnClick(&button, data_clone, on_click)
	button_dom := azul.AzButton_dom(button)

	body := azul.AzDom_createBody()
	azul.AzDom_addChild(&body, label_wrapper)
	azul.AzDom_addChild(&body, button_dom)
	return body
}

main :: proc() {
	model := MyDataModel{ counter = 5 }
	data := my_data_upcast(model)

	window := azul.AzWindowCreateOptions_create(layout)
	title_bytes := "Hello World"
	window.window_state.title = azul.AzString_fromUtf8(raw_data(title_bytes), uint(len(title_bytes)))
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0

	// NoTitleAutoInject: OS draws the window buttons; framework injects a draggable titlebar.
	window.window_state.flags.decorations = azul.AzWindowDecorations.NoTitleAutoInject
	window.window_state.flags.background_material = azul.AzWindowBackgroundMaterial.Sidebar

	app := azul.AzApp_create(data, azul.AzAppConfig_create())
	azul.AzApp_run(&app, window)
}
