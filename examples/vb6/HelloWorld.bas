Attribute VB_Name = "HelloWorld"
' ============================================================================
' VB6 port of examples/c/hello-world.c.
'
' Same data model (a 32-bit unsigned counter), same callback semantics
' (clicking the button increments the counter and asks for a redraw),
' same visual output (a centred label + a primary button).
'
' === 32-BIT ONLY ===
' This program will ONLY work against a 32-bit azul.dll. Building or
' running it against a 64-bit azul.dll triggers "Bad DLL Calling
' Convention" or "File not found" errors. The Rust target is
' i686-pc-windows-msvc.
'
' Runtime: msvbvm60.dll (ships with every Windows since Win98).
'
' Build:
'
'     Open HelloWorld.vbp in the VB6 IDE and press F5 (run), or:
'     vbc.exe /out:HelloWorld.exe HelloWorld.bas
'
' Required next to the .exe at runtime:
'
'     azul.dll  (32-bit i686-pc-windows-msvc build)
' ============================================================================

Option Explicit

' ---- External declarations ----
'
' These mirror the relevant subset of Azul.bas — we redeclare them inline
' so this example file is self-contained. In a real project you'd just
' `Option Explicit` and reference Azul.bas via the .vbp project file.

Public Declare Sub CopyMemory Lib "kernel32" Alias "RtlMoveMemory" _
    (ByRef Destination As Any, ByRef Source As Any, ByVal Length As Long)

' AzString / AzRefAny / AzApp / AzDom / AzWindowCreateOptions / AzCallbackInfo
' / AzLayoutCallbackInfo / AzAppConfig / AzButton / AzCss / AzCssProperty
' / AzCssPropertyWithConditions / AzStyleFontSize / AzJson / AzResultRefAnyString
' / AzOptionI64 are all `Long`-as-pointer in the simplified shape used here.
' Real generated bindings emit Public Type records for the POD ones and a
' tagged-union shim for the enum ones.

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
Public Declare Function AzDom_createText Lib "azul" Alias "AzDom_createText" _
    (ByVal s As Long) As Long
Public Declare Sub AzDom_addChild Lib "azul" Alias "AzDom_addChild" _
    (ByVal parent As Long, ByVal child As Long)
Public Declare Sub AzDom_addCssProperty Lib "azul" Alias "AzDom_addCssProperty" _
    (ByVal d As Long, ByVal prop As Long)
Public Declare Function AzDom_style Lib "azul" Alias "AzDom_style" _
    (ByVal d As Long, ByVal css As Long) As Long

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
Public Declare Function AzRefAny_newC Lib "azul" Alias "AzRefAny_newC" _
    (ByVal ptr_ As Long, ByVal sz As Long, ByVal type_id As Long, _
     ByVal type_name As Long, ByVal destructor As Long) As Long

' Update / ButtonType discriminator tags (ints).
Public Const az_Update_DoNothing As Long = 0
Public Const az_Update_RefreshDom As Long = 1
Public Const az_ButtonType_Primary As Long = 0
Public Const az_WindowDecorations_NoTitleAutoInject As Long = 2
Public Const az_WindowBackgroundMaterial_Sidebar As Long = 4

' ---- Data model ------------------------------------------------------------

Public Type MyDataModel
    counter As Long
End Type

' Destructor stub: MyDataModel owns no heap memory, so do nothing.
Public Sub MyDataModel_destructor(ByVal p As Long)
    ' intentionally empty
End Sub

' ---- Helpers ---------------------------------------------------------------

' Build an AzString from a VB6 String. We pass StrPtr(s) which gives the
' BSTR data pointer; copyFromBytes then duplicates the bytes into an
' azul-owned string. NOTE: this passes UTF-16 bytes; for non-ASCII
' content use a UTF-8 conversion helper. For the example it is fine.
Public Function AzStr(ByRef s As String) As Long
    AzStr = AzString_copyFromBytes(StrPtr(s), 0, LenB(s))
End Function

' ---- Callback: button click ------------------------------------------------

Public Function on_click(ByVal data As Long, ByVal info As Long) As Long
    ' SKIPPED: real downcast — the C example uses MyDataModelRefMut_create +
    ' MyDataModel_downcastMut to extract a typed pointer. The VB6 binding
    ' does not yet wrap those helpers, so we extract the payload pointer
    ' by hand via CopyMemory.
    Dim modelPtr As Long
    CopyMemory modelPtr, ByVal data, 4
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

' ---- Callback: layout ------------------------------------------------------

Public Function layout(ByVal data As Long, ByVal info As Long) As Long
    Dim modelPtr As Long
    Dim labelText As Long, labelDom As Long, labelWrapper As Long
    Dim btn As Long, buttonDom As Long, body As Long
    Dim fontSize As Long, dataClone As Long
    Dim buf As String

    CopyMemory modelPtr, ByVal data, 4
    If modelPtr = 0 Then
        layout = AzDom_createBody()
        Exit Function
    End If

    Dim m As MyDataModel
    CopyMemory m, ByVal modelPtr, LenB(m)

    ' Counter label, wrapped in a div so the font-size CSS sticks.
    buf = CStr(m.counter)
    labelText = AzStr(buf)
    labelDom = AzDom_createText(labelText)
    labelWrapper = AzDom_createDiv()

    fontSize = AzCssProperty_fontSize(AzStyleFontSize_px(32!))
    AzDom_addCssProperty labelWrapper, AzCssPropertyWithConditions_simple(fontSize)
    AzDom_addChild labelWrapper, labelDom

    ' Increment button.
    btn = AzButton_create(AzStr("Increase counter"))
    AzButton_setButtonType btn, az_ButtonType_Primary

    ' Clone the RefAny so the button keeps its own reference.
    dataClone = AzRefAny_clone(data)
    AzButton_setOnClick btn, dataClone, AddressOf on_click
    buttonDom = AzButton_dom(btn)

    ' Body.
    body = AzDom_createBody()
    AzDom_addChild body, labelWrapper
    AzDom_addChild body, buttonDom

    layout = AzDom_style(body, AzCss_empty())
End Function

' ---- Main ------------------------------------------------------------------

Public Sub Main()
    Dim m As MyDataModel
    Dim data As Long
    Dim window As Long
    Dim app As Long

    m.counter = 5

    ' SKIPPED: real upcast — the C example uses AZ_REFLECT_JSON which
    ' expands to MyDataModel_upcast. The VB6 binding does not yet expose
    ' that macro; build the RefAny by calling AzRefAny_newC directly.
    data = AzRefAny_newC( _
        VarPtr(m), _
        LenB(m), _
        0, _
        AzStr("MyDataModel"), _
        AddressOf MyDataModel_destructor)

    window = AzWindowCreateOptions_create(AddressOf layout)

    ' SKIPPED: window_state.title / size / flags assignment — these are
    ' nested fields inside AzWindowCreateOptions and require either
    ' generated POD types (Public Type AzWindowCreateOptions ...) or
    ' a C-side setter shim. The C example mutates them in-place; the
    ' VB6 example uses defaults.

    app = AzApp_create(data, AzAppConfig_create())
    AzApp_run app, window
    AzApp_delete app
End Sub
