Attribute VB_Name = "HelloWorld"

Option Explicit

Public Declare Sub CopyMemory Lib "kernel32" Alias "RtlMoveMemory" _
    (ByRef Destination As Any, ByRef Source As Any, ByVal Length As Long)

' Every Az* struct below is declared `Long`-as-pointer; the real generated
' bindings emit Public Type records instead.

Public Declare Function AzString_copyFromBytes Lib "azul" Alias "AzString_copyFromBytes" _
    (ByVal ptr_ As Long, ByVal start_ As Long, ByVal len_ As Long) As Long

Public Declare Function AzApp_create Lib "azul" Alias "AzApp_create" _
    (ByVal data As Long, ByVal config As Long) As Long
Public Declare Sub AzApp_run Lib "azul" Alias "AzApp_run" _
    (ByVal app As Long, ByVal opts As Long)
Public Declare Sub AzApp_delete Lib "azul" Alias "AzApp_delete" _
    (ByVal app As Long)

Public Declare Function AzAppConfig_create Lib "azul" Alias "AzAppConfig_create" _
    () As Long

Public Declare Function AzWindowCreateOptions_create Lib "azul" Alias "AzWindowCreateOptions_create" _
    (ByVal layout_cb As Long) As Long

Public Declare Function AzDom_createBody Lib "azul" Alias "AzDom_createBody" _
    () As Long
Public Declare Function AzDom_createDiv Lib "azul" Alias "AzDom_createDiv" _
    () As Long
Public Declare Function AzDom_createTextDoNotUseWithoutBlockLevelWrapper Lib "azul" Alias "AzDom_createTextDoNotUseWithoutBlockLevelWrapper" _
    (ByVal s As Long) As Long
Public Declare Sub AzDom_addChild Lib "azul" Alias "AzDom_addChild" _
    (ByVal parent As Long, ByVal child As Long)
Public Declare Sub AzDom_addCssProperty Lib "azul" Alias "AzDom_addCssProperty" _
    (ByVal d As Long, ByVal prop As Long)

Public Declare Function AzButton_create Lib "azul" Alias "AzButton_create" _
    (ByVal s As Long) As Long
Public Declare Sub AzButton_setButtonType Lib "azul" Alias "AzButton_setButtonType" _
    (ByVal btn As Long, ByVal kind As Long)
Public Declare Sub AzButton_setOnClick Lib "azul" Alias "AzButton_setOnClick" _
    (ByVal btn As Long, ByVal data As Long, ByVal cb As Long)
Public Declare Function AzButton_dom Lib "azul" Alias "AzButton_dom" _
    (ByVal btn As Long) As Long

Public Declare Function AzCss_empty Lib "azul" Alias "AzCss_empty" _
    () As Long
Public Declare Function AzCssProperty_fontSize Lib "azul" Alias "AzCssProperty_fontSize" _
    (ByVal sz As Long) As Long
Public Declare Function AzCssPropertyWithConditions_simple Lib "azul" Alias "AzCssPropertyWithConditions_simple" _
    (ByVal prop As Long) As Long
Public Declare Function AzStyleFontSize_px Lib "azul" Alias "AzStyleFontSize_px" _
    (ByVal v As Single) As Long

Public Declare Function AzRefAny_clone Lib "azul" Alias "AzRefAny_clone" _
    (ByVal data As Long) As Long
Public Declare Function AzRefAny_getDataPtr Lib "azul" Alias "AzRefAny_getDataPtr" _
    (ByVal ref_any As Long) As Long
Public Declare Function AzRefAny_newC Lib "azul" Alias "AzRefAny_newC" _
    (ByVal ptr_ As Long, ByVal sz As Long, ByVal type_id As Long, _
     ByVal type_name As Long, ByVal destructor As Long) As Long

Public Const az_Update_DoNothing As Long = 0
Public Const az_Update_RefreshDom As Long = 1
Public Const az_ButtonType_Primary As Long = 0
Public Const az_WindowDecorations_NoTitleAutoInject As Long = 2
Public Const az_WindowBackgroundMaterial_Sidebar As Long = 4

Public Type MyDataModel
    counter As Long
End Type

Public Sub MyDataModel_destructor(ByVal p As Long)
End Sub

' Build an AzString from a VB6 String. We pass StrPtr(s) which gives the
' BSTR data pointer; copyFromBytes then duplicates the bytes into an
' azul-owned string. NOTE: this passes UTF-16 bytes; for non-ASCII
' content use a UTF-8 conversion helper. For the example it is fine.
Public Function AzStr(ByRef s As String) As Long
    AzStr = AzString_copyFromBytes(StrPtr(s), 0, LenB(s))
End Function

Public Function on_click(ByVal data As Long, ByVal info As Long) As Long
    Dim modelPtr As Long
    modelPtr = AzRefAny_getDataPtr(data)
    If modelPtr <> 0 Then
        Dim m As MyDataModel
        CopyMemory m, ByVal modelPtr, LenB(m)
        m.counter = m.counter + 1
        CopyMemory ByVal modelPtr, m, LenB(m)
        on_click = az_Update_RefreshDom
    Else
        on_click = az_Update_DoNothing
    End If
End Function

Public Function layout(ByVal data As Long, ByVal info As Long) As Long
    Dim modelPtr As Long
    Dim labelText As Long, labelDom As Long, labelWrapper As Long
    Dim btn As Long, buttonDom As Long, body As Long
    Dim fontSize As Long, dataClone As Long
    Dim buf As String

    modelPtr = AzRefAny_getDataPtr(data)
    If modelPtr = 0 Then
        layout = AzDom_createBody()
        Exit Function
    End If

    Dim m As MyDataModel
    CopyMemory m, ByVal modelPtr, LenB(m)

    buf = CStr(m.counter)
    labelText = AzStr(buf)
    labelDom = AzDom_createTextDoNotUseWithoutBlockLevelWrapper(labelText)
    labelWrapper = AzDom_createDiv()

    fontSize = AzCssProperty_fontSize(AzStyleFontSize_px(32!))
    AzDom_addCssProperty labelWrapper, AzCssPropertyWithConditions_simple(fontSize)
    AzDom_addChild labelWrapper, labelDom

    btn = AzButton_create(AzStr("Increase counter"))
    AzButton_setButtonType btn, az_ButtonType_Primary

    ' Clone the RefAny so the button keeps its own reference.
    dataClone = AzRefAny_clone(data)
    AzButton_setOnClick btn, dataClone, AddressOf on_click
    buttonDom = AzButton_dom(btn)

    body = AzDom_createBody()
    AzDom_addChild body, labelWrapper
    AzDom_addChild body, buttonDom

    layout = body
End Function

Public Sub Main()
    Dim m As MyDataModel
    Dim data As Long
    Dim window As Long
    Dim app As Long

    m.counter = 5

    data = AzRefAny_newC( _
        VarPtr(m), _
        LenB(m), _
        0, _
        AzStr("MyDataModel"), _
        AddressOf MyDataModel_destructor)

    window = AzWindowCreateOptions_create(AddressOf layout)

    app = AzApp_create(data, AzAppConfig_create())
    AzApp_run app, window
    AzApp_delete app
End Sub
