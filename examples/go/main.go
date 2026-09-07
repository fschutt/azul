
package main

/*
#cgo linux,darwin LDFLAGS: -lazul
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "azul.h"

extern AzUpdate goOnClick        (AzRefAny data, AzCallbackInfo info);
extern AzDom    goLayout         (AzRefAny data, AzLayoutCallbackInfo info);
extern void     myDataDestructor (void* m);

static inline AzCallbackType              make_click_callback     (void) { return (AzCallbackType)goOnClick; }
static inline AzLayoutCallbackType        make_layout_callback    (void) { return (AzLayoutCallbackType)goLayout; }
static inline AzRefAnyDestructorType      make_my_data_destructor (void) { return (AzRefAnyDestructorType)myDataDestructor; }
*/
import "C"

import (
	"fmt"
	"unsafe"
)

type myDataModel struct {
	counter C.uint32_t
}

var myDataTypeToken byte
var myDataTypeID = C.uint64_t(uintptr(unsafe.Pointer(&myDataTypeToken)))

//export myDataDestructor
func myDataDestructor(_ unsafe.Pointer) {}

func myDataUpcast(model myDataModel) C.AzRefAny {
	typeName := []byte("MyDataModel")
	cTypeName := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&typeName[0])), C.size_t(len(typeName)))

	size := C.size_t(unsafe.Sizeof(model))
	buf := C.malloc(size)
	if buf == nil {
		panic("out of memory allocating the RefAny payload")
	}
	defer C.free(buf)
	*(*myDataModel)(buf) = model

	ptr := C.AzGlVoidPtrConst{
		ptr:            buf,
		run_destructor: C.bool(false),
	}
	return C.AzRefAny_newC(
		ptr,
		size,
		C.size_t(unsafe.Alignof(model)),
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

//export goLayout
func goLayout(data C.AzRefAny, _ C.AzLayoutCallbackInfo) C.AzDom {
	d := data
	m := myDataDowncast(&d)
	if m == nil {
		return C.AzDom_createBody()
	}

	counterStr := []byte(fmt.Sprintf("%d", m.counter))
	counterAz := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&counterStr[0])), C.size_t(len(counterStr)))
	label := C.AzDom_createPWithText(counterAz)
	cssBytes := []byte("font-size: 32px;")
	C.AzDom_setCss(&label, C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&cssBytes[0])), C.size_t(len(cssBytes))))

	btnLabelBytes := []byte("Increase counter")
	btnLabel := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&btnLabelBytes[0])), C.size_t(len(btnLabelBytes)))
	button := C.AzButton_create(btnLabel)
	C.AzButton_setButtonType(&button, C.AzButtonType_Primary)
	dataClone := C.AzRefAny_clone(&d)
	C.AzButton_setOnClick(&button, dataClone, C.make_click_callback())
	buttonDom := C.AzButton_dom(button)

	body := C.AzDom_createBody()
	C.AzDom_addChild(&body, label)
	C.AzDom_addChild(&body, buttonDom)
	return body
}

func main() {
	model := myDataModel{counter: 5}
	data := myDataUpcast(model)

	window := C.AzWindowCreateOptions_create(C.make_layout_callback())
	titleBytes := []byte("Hello World")
	window.window_state.title = C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&titleBytes[0])), C.size_t(len(titleBytes)))
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0

	app := C.AzApp_create(data, C.AzAppConfig_create())
	C.AzApp_run(&app, window)
}
