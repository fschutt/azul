' ============================================================================
' FreeBASIC port of examples/c/hello-world.c.
'
' Same data model (a 32-bit unsigned counter), same callback semantics
' (clicking the button increments the counter and asks for a redraw),
' same visual output (a centred label + a primary button).
'
' Build:
'
'     fbc hello-world.bas
'
' Run (Linux):
'
'     LD_LIBRARY_PATH=. ./hello-world
'
' Run (macOS):
'
'     DYLD_LIBRARY_PATH=. ./hello-world
'
' Run (Windows): make sure azul.dll is on PATH or in the program dir.
' ============================================================================

#include "azul.bi"

' ---- Data model ------------------------------------------------------------

Type MyDataModel
    counter As ULong
End Type

' Destructor stub: MyDataModel owns no heap memory, so do nothing.
Sub MyDataModel_destructor Cdecl (ByVal p As Any Ptr)
    ' intentionally empty
End Sub

' Forward declarations for the JSON round-trip helpers
' (the C example uses the AZ_REFLECT_JSON macro to register these).
Declare Function MyDataModel_toJson Cdecl (ByVal refany As AzRefAny) As AzJson
Declare Function MyDataModel_fromJson Cdecl (ByVal json As AzJson) As AzResultRefAnyString

' ---- Helpers ---------------------------------------------------------------

' Build an AzString from a constant ANSI literal. FreeBASIC `String` is
' length-prefixed; we pass the data pointer + length to copyFromBytes.
Function AzStr (ByRef s As Const String) As AzString
    Return AzString_copyFromBytes(StrPtr(s), 0, Len(s))
End Function

' ---- Callback: button click ------------------------------------------------

Function on_click Cdecl (ByVal data As AzRefAny, ByVal info As AzCallbackInfo) As AzUpdate
    Dim modelPtr As MyDataModel Ptr
    Dim result As AzUpdate

    ' SKIPPED: real downcast — the C example uses MyDataModelRefMut_create +
    ' MyDataModel_downcastMut. The FreeBASIC binding does not yet wrap
    ' those helpers, so we pull the raw payload pointer out by hand.
    modelPtr = CPtr(MyDataModel Ptr, @data)
    If modelPtr <> 0 Then
        modelPtr->counter += 1
        result.tag = AzUpdateTag_RefreshDom
    Else
        result.tag = AzUpdateTag_DoNothing
    End If
    Return result
End Function

' ---- Callback: layout ------------------------------------------------------

Function layout Cdecl (ByVal data As AzRefAny, ByVal info As AzLayoutCallbackInfo) As AzDom
    Dim modelPtr As MyDataModel Ptr
    Dim labelText As AzString
    Dim labelDom As AzDom
    Dim labelWrapper As AzDom
    Dim button As AzButton
    Dim buttonDom As AzDom
    Dim body As AzDom
    Dim fontSize As AzCssProperty
    Dim dataClone As AzRefAny
    Dim buf As String

    modelPtr = CPtr(MyDataModel Ptr, @data)
    If modelPtr = 0 Then
        Return AzDom_createBody()
    End If

    ' Counter label, wrapped in a div so the font-size CSS sticks.
    buf = Str(modelPtr->counter)
    labelText  = AzStr(buf)
    labelDom   = AzDom_createText(labelText)
    labelWrapper = AzDom_createDiv()

    fontSize = AzCssProperty_fontSize(AzStyleFontSize_px(32.0))
    AzDom_addCssProperty(@labelWrapper, AzCssPropertyWithConditions_simple(fontSize))
    AzDom_addChild(@labelWrapper, labelDom)

    ' Increment button.
    button = AzButton_create(AzStr("Increase counter"))
    AzButton_setButtonType(@button, AzButtonType_Primary)

    ' Clone the RefAny so the button keeps its own reference.
    dataClone = AzRefAny_clone(@data)
    AzButton_setOnClick(@button, dataClone, @on_click)
    buttonDom = AzButton_dom(button)

    ' Body.
    body = AzDom_createBody()
    AzDom_addChild(@body, labelWrapper)
    AzDom_addChild(@body, buttonDom)

    Return AzDom_style(body, AzCss_empty())
End Function

' ---- JSON round-trip stubs (C example uses these for state persistence) ----

Function MyDataModel_toJson Cdecl (ByVal refany As AzRefAny) As AzJson
    ' SKIPPED: real toJson — we'd downcast the RefAny and serialise the
    ' counter. For this example we always return JSON null.
    Return AzJson_null()
End Function

Function MyDataModel_fromJson Cdecl (ByVal json As AzJson) As AzResultRefAnyString
    ' SKIPPED: real fromJson — we'd parse a JSON int, build a MyDataModel,
    ' then upcast it. Return an error string instead.
    Return AzResultRefAnyString_err(AzStr("MyDataModel.fromJson is not implemented in the FreeBASIC example"))
End Function

' ---- Main ------------------------------------------------------------------

Dim model As MyDataModel
Dim data As AzRefAny
Dim window As AzWindowCreateOptions
Dim app As AzApp

model.counter = 5

' SKIPPED: real upcast — the C example uses AZ_REFLECT_JSON which expands
' to MyDataModel_upcast. The FreeBASIC binding does not yet expose that
' macro; build the RefAny by calling AzRefAny_newC directly with our
' destructor pointer.
data = AzRefAny_newC( _
    @model, _
    SizeOf(MyDataModel), _
    0, _
    AzStr("MyDataModel"), _
    @MyDataModel_destructor _
)

window = AzWindowCreateOptions_create(@layout)
window.window_state.title = AzStr("Hello World")
window.window_state.size.dimensions.width  = 400.0
window.window_state.size.dimensions.height = 300.0

' NoTitleAutoInject: OS draws close/min/max buttons,
' framework auto-injects a Titlebar with drag support.
window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

app = AzApp_create(data, AzAppConfig_create())
AzApp_run(@app, window)
AzApp_delete(@app)
