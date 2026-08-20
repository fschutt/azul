#include "azul.bi"

Type MyDataModel
    counter As ULong
End Type

Sub MyDataModel_destructor Cdecl (ByVal p As Any Ptr)
End Sub

Declare Function MyDataModel_toJson Cdecl (ByVal refany As AzRefAny) As AzJson
Declare Function MyDataModel_fromJson Cdecl (ByVal json As AzJson) As AzResultRefAnyString

Function AzStr (ByRef s As Const String) As AzString
    Return AzString_copyFromBytes(StrPtr(s), 0, Len(s))
End Function

Function on_click Cdecl (ByVal data As AzRefAny, ByVal info As AzCallbackInfo) As AzUpdate
    Dim modelPtr As MyDataModel Ptr
    Dim result As AzUpdate

    modelPtr = CPtr(MyDataModel Ptr, AzRefAny_getDataPtr(@data))
    If modelPtr <> 0 Then
        modelPtr->counter += 1
        result = AzUpdate_RefreshDom
    Else
        result = AzUpdate_DoNothing
    End If
    Return result
End Function

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

    modelPtr = CPtr(MyDataModel Ptr, AzRefAny_getDataPtr(@data))
    If modelPtr = 0 Then
        Return AzDom_createBody()
    End If

    buf = Str(modelPtr->counter)
    labelText  = AzStr(buf)
    labelDom   = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(labelText)
    labelWrapper = AzDom_createDiv()

    fontSize = AzCssProperty_fontSize(AzStyleFontSize_px(32.0))
    AzDom_addCssProperty(@labelWrapper, AzCssPropertyWithConditions_simple(fontSize))
    AzDom_addChild(@labelWrapper, labelDom)

    button = AzButton_create(AzStr("Increase counter"))
    AzButton_setButtonType(@button, AzButtonType_Primary)

    ' Clone the RefAny so the button keeps its own reference.
    dataClone = AzRefAny_clone(@data)
    AzButton_setOnClick(@button, dataClone, @on_click)
    buttonDom = AzButton_dom(button)

    body = AzDom_createBody()
    AzDom_addChild(@body, labelWrapper)
    AzDom_addChild(@body, buttonDom)

    Return body
End Function

Function MyDataModel_toJson Cdecl (ByVal refany As AzRefAny) As AzJson
    Return AzJson_null()
End Function

Function MyDataModel_fromJson Cdecl (ByVal json As AzJson) As AzResultRefAnyString
    Return AzResultRefAnyString_err(AzStr("MyDataModel.fromJson is not implemented in the FreeBASIC example"))
End Function

Dim model As MyDataModel
Dim modelWrapper As AzGlVoidPtrConst
Dim data As AzRefAny
Dim window As AzWindowCreateOptions
Dim app As AzApp

model.counter = 5

modelWrapper.ptr_ = @model
modelWrapper.run_destructor = 0

data = AzRefAny_newC( _
    modelWrapper, _
    SizeOf(MyDataModel), _
    SizeOf(ULong), _
    0, _
    AzStr("MyDataModel"), _
    @MyDataModel_destructor, _
    0, _
    0 _
)

window = AzWindowCreateOptions_create(@layout)
window.window_state.title = AzStr("Hello World")
window.window_state.size.dimensions.width  = 400.0
window.window_state.size.dimensions.height = 300.0

window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

app = AzApp_create(data, AzAppConfig_create())
AzApp_run(@app, window)
AzApp_delete(@app)
