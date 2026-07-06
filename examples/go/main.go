// CGO_CFLAGS="-I." CGO_LDFLAGS="-L." go build && LD_LIBRARY_PATH=. ./hello-world

package main

/*
#cgo linux,darwin LDFLAGS: -lazul
// On Windows the MSVC import lib (azul.dll.lib) is linked via CGO_LDFLAGS
// instead; a bare -lazul has no libazul.a/azul.lib to resolve there.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "azul.h"

// Forward declarations for the Go-exported callbacks below. cgo
// generates a header `_cgo_export.h` with these too, but pulling them
// in here lets the C-side cast lift `AzCallbackType` / `AzLayoutCallbackType`
// out into single helpers.
extern AzUpdate goOnClick        (AzRefAny data, AzCallbackInfo info);
extern AzDom    goLayout         (AzRefAny data, AzLayoutCallbackInfo info);
extern void     myDataDestructor (void* m);

// AzButton_setOnClick / AzWindowCreateOptions_create take a RAW C-ABI
// function pointer (AzCallbackType / AzLayoutCallbackType), NOT the
// AzCallback wrapper struct. cgo maps a raw fn-pointer typedef to
// `*[0]byte` and a struct to `_Ctype_struct_Az...`, so returning the
// struct here is a type error at the Go call site. Return the raw
// fn-pointer types directly.
static inline AzCallbackType              make_click_callback     (void) { return (AzCallbackType)goOnClick; }
static inline AzLayoutCallbackType        make_layout_callback    (void) { return (AzLayoutCallbackType)goLayout; }
static inline AzRefAnyDestructorType      make_my_data_destructor (void) { return (AzRefAnyDestructorType)myDataDestructor; }
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// Compile-time-unique type id: the address of a package var. upcast wraps
// the struct in an AzRefAny; downcast recovers a typed pointer.

type myDataModel struct {
	counter C.uint32_t
}

// The address of this package var is the per-type RTTI id.
var myDataTypeToken byte
var myDataTypeID = C.uint64_t(uintptr(unsafe.Pointer(&myDataTypeToken)))

//export myDataDestructor
func myDataDestructor(_ unsafe.Pointer) {}

func myDataUpcast(model myDataModel) C.AzRefAny {
	local := model // stack copy; AzRefAny_newC copies the bytes
	typeName := []byte("MyDataModel")
	cTypeName := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&typeName[0])), C.size_t(len(typeName)))
	ptr := C.AzGlVoidPtrConst{
		ptr:            unsafe.Pointer(&local),
		run_destructor: C.bool(false),
	}
	return C.AzRefAny_newC(
		ptr,
		C.size_t(unsafe.Sizeof(local)),
		C.size_t(unsafe.Alignof(local)),
		myDataTypeID,
		cTypeName,
		C.make_my_data_destructor(),
		0, // serialize_fn
		0, // deserialize_fn
	)
}

func myDataDowncast(refany *C.AzRefAny) *myDataModel {
	if !bool(C.AzRefAny_isType(refany, myDataTypeID)) {
		return nil
	}
	raw := C.AzRefAny_getDataPtr(refany)
	if raw == nil {
		return nil
	}
	return (*myDataModel)(raw)
}

// ── Callback: button click ────────────────────────────────────────────

//export goOnClick
func goOnClick(data C.AzRefAny, _ C.AzCallbackInfo) C.AzUpdate {
	d := data
	m := myDataDowncast(&d)
	if m == nil {
		return C.AzUpdate_DoNothing
	}
	m.counter++
	return C.AzUpdate_RefreshDom
}

// ── Layout callback ───────────────────────────────────────────────────

//export goLayout
func goLayout(data C.AzRefAny, _ C.AzLayoutCallbackInfo) C.AzDom {
	d := data
	m := myDataDowncast(&d)
	if m == nil {
		return C.AzDom_createBody()
	}

	// Counter label (wrapped in a div so the font-size sticks).
	counterStr := []byte(fmt.Sprintf("%d", m.counter))
	counterAz := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&counterStr[0])), C.size_t(len(counterStr)))
	label := C.AzDom_createText(counterAz)

	labelWrapper := C.AzDom_createDiv()
	fontSize := C.AzStyleFontSize_px(C.float(32.0))
	cssProp := C.AzCssProperty_fontSize(fontSize)
	cond := C.AzCssPropertyWithConditions_simple(cssProp)
	C.AzDom_addCssProperty(&labelWrapper, cond)
	C.AzDom_addChild(&labelWrapper, label)

	// AzButton_setOnClick takes the bare fn-pointer typedef; the C helper
	// casts the //export'd goOnClick to AzCallbackType (see the preamble).
	btnLabelBytes := []byte("Increase counter")
	btnLabel := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&btnLabelBytes[0])), C.size_t(len(btnLabelBytes)))
	button := C.AzButton_create(btnLabel)
	C.AzButton_setButtonType(&button, C.AzButtonType_Primary)
	dataClone := C.AzRefAny_clone(&d)
	C.AzButton_setOnClick(&button, dataClone, C.make_click_callback())
	buttonDom := C.AzButton_dom(button)

	// Body.
	body := C.AzDom_createBody()
	C.AzDom_addChild(&body, labelWrapper)
	C.AzDom_addChild(&body, buttonDom)
	return body
}

// ── Main ──────────────────────────────────────────────────────────────

func main() {
	model := myDataModel{counter: 5}
	data := myDataUpcast(model)

	window := C.AzWindowCreateOptions_create(C.make_layout_callback())
	titleBytes := []byte("Hello World")
	window.window_state.title = C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&titleBytes[0])), C.size_t(len(titleBytes)))
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0

	// NoTitleAutoInject: OS draws close/min/max buttons; framework
	// auto-injects a Titlebar with drag support.
	window.window_state.flags.decorations = C.AzWindowDecorations_NoTitleAutoInject
	window.window_state.flags.background_material = C.AzWindowBackgroundMaterial_Sidebar

	app := C.AzApp_create(data, C.AzAppConfig_create())
	C.AzApp_run(&app, window)
}
