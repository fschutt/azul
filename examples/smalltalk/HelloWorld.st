"=============================================================================
 examples/smalltalk/HelloWorld.st
 Pharo (Smalltalk) port of examples/c/hello-world.c.

 Same data model (a `MyDataModel` struct with a uint32 counter), same callback
 semantics (mouse click increments, layout renders).

 Smalltalk has no `main` — runnable code lives at the bottom of this file as
 a `do-it` block (a Smalltalk fileIn convention: any top-level expression
 ending in `!` is executed at file-in time). Open the Pharo Playground and
 evaluate `'HelloWorld.st' asFileReference fileIn` after first loading the
 generated `Azul.st` (or installing `BaselineOfAzul`).

 Prerequisites:
   - Pharo 8 or newer (UnifiedFFI ships in-image).
   - libazul.so / libazul.dylib / azul.dll on the OS loader path.
   - The generated `Azul.st` already loaded via Iceberg or fileIn.
=============================================================================="

"--- Live storage for FFI callback blocks ---------------------------------------
 UnifiedFFI's `ffiCallback:` produces a libffi trampoline whose lifetime is
 tied to the block. If the block is GC'd, the C side jumps into freed memory
 the next time the callback fires. We therefore keep every casted callback in
 the class-side variable `LiveCallbacks` for the entire image lifetime.
-------------------------------------------------------------------------------"

Smalltalk at: #LiveCallbacks put: OrderedCollection new.

"--- Data model -----------------------------------------------------------------
 A trivial wrapper struct backed by an `FFIExternalStructure` so the counter
 lives in C-allocated memory. `AzRefAny_newC` copies sizeof(MyDataModel) bytes
 from the supplied pointer into a heap-allocated RefAny.
-------------------------------------------------------------------------------"

Class {
    #name : 'MyDataModel',
    #superclass : 'FFIExternalStructure',
    #instVars : [ ],
    #classVars : [ ],
    #category : 'Azul-HelloWorld'
}

{ #category : 'ffi' }
MyDataModel class >> fields [
    ^ #(
        (uint32 counter)
    )
]

"--- Hello-world driver ---------------------------------------------------------"

Class {
    #name : 'AzulHelloWorld',
    #superclass : 'Object',
    #instVars : [ ],
    #classVars : [ 'OnClickCallback' 'LayoutCallback' 'ModelDestructorCallback' ],
    #category : 'Azul-HelloWorld'
}

"--- Constants ------------------------------------------------------------------"

{ #category : 'constants' }
AzulHelloWorld class >> rttiId [
    "A unique 64-bit RTTI id for MyDataModel. Pick any stable value that no
     other RefAny in the process uses; collisions cause spurious downcast
     successes."
    ^ 16#A2010001
]

"--- Helpers --------------------------------------------------------------------"

{ #category : 'private' }
AzulHelloWorld class >> azStringFromString: aSmalltalkString [
    "Allocate an AzString by going through a UTF-8 byte buffer and the public
     `AzString_copyFromBytes` constructor."
    | bytes |
    bytes := aSmalltalkString utf8Encoded.
    ^ AzulNative
        azStringCopyFromBytes: bytes
        startByte: 0
        endByte: bytes size
]

{ #category : 'private' }
AzulHelloWorld class >> upcastModel: aModel [
    "Wrap a MyDataModel handle in an AzRefAny."
    ^ AzulNative
        azRefAnyNewC: aModel getHandle
        len: MyDataModel byteSize
        align: MyDataModel alignment
        typeId: self rttiId
        typeName: (self azStringFromString: 'MyDataModel')
        destructor: ModelDestructorCallback
]

{ #category : 'private' }
AzulHelloWorld class >> downcastMut: aRefAny [
    "Borrow a *mut MyDataModel from an AzRefAny. Returns nil on type mismatch."
    (AzulNative azRefAnyIsType: aRefAny typeId: self rttiId)
        ifFalse: [ ^ nil ].
    ^ MyDataModel fromHandle: (AzulNative azRefAnyGetDataPtr: aRefAny)
]

"--- Callbacks ------------------------------------------------------------------"

{ #category : 'callbacks' }
AzulHelloWorld class >> installCallbacks [
    "Allocate libffi trampolines for our three Smalltalk callback blocks and
     pin them in `LiveCallbacks` so the GC never reclaims the trampoline
     while native code still holds the function pointer."

    ModelDestructorCallback := [ :ptr | "MyDataModel owns no heap memory" ]
        ffiCallback: #( void (void* ptr) ).
    LiveCallbacks add: ModelDestructorCallback.

    OnClickCallback := [ :data :info |
        | model |
        model := self downcastMut: data.
        model isNil
            ifTrue: [ AzulNative azUpdateDoNothing ]
            ifFalse: [
                model counter: model counter + 1.
                AzulNative azUpdateRefreshDom ] ]
        ffiCallback: #( int32 (void* data, void* info) ).
    LiveCallbacks add: OnClickCallback.

    LayoutCallback := [ :data :info |
        self layoutFor: data ]
        ffiCallback: #( AzDom (void* data, void* info) ).
    LiveCallbacks add: LayoutCallback
]

"--- Layout ---------------------------------------------------------------------"

{ #category : 'layout' }
AzulHelloWorld class >> layoutFor: data [
    | model labelText label labelWrapper buttonText button buttonDom body dataClone |

    model := self downcastMut: data.
    model isNil ifTrue: [ ^ AzulNative azDomCreateBody ].

    "Counter label, wrapped in a div so it lays out as block."
    labelText := self azStringFromString: model counter printString.
    label := AzulNative azDomCreateText: labelText.
    labelWrapper := AzulNative azDomCreateDiv.
    AzulNative
        azDomAddCssProperty: labelWrapper
        property: (AzulNative azCssPropertyWithConditionsSimple:
            (AzulNative azCssPropertyFontSize:
                (AzulNative azStyleFontSizePx: 32.0))).
    AzulNative azDomAddChild: labelWrapper child: label.

    "Button."
    buttonText := self azStringFromString: 'Increase counter'.
    button := AzulNative azButtonCreate: buttonText.
    AzulNative azButtonSetButtonType: button buttonType: AzButtonType Primary.
    dataClone := AzulNative azRefAnyClone: data.
    AzulNative
        azButtonSetOnClick: button
        data: dataClone
        callback: OnClickCallback.
    buttonDom := AzulNative azButtonDom: button.

    "Body."
    body := AzulNative azDomCreateBody.
    AzulNative azDomAddChild: body child: labelWrapper.
    AzulNative azDomAddChild: body child: buttonDom.

    ^ AzulNative azDomStyle: body css: AzulNative azCssEmpty
]

"--- Main -----------------------------------------------------------------------"

{ #category : 'main' }
AzulHelloWorld class >> run [
    | model data window app |

    self installCallbacks.

    "Build a MyDataModel in C-allocated memory."
    model := MyDataModel new.
    model counter: 5.
    data := self upcastModel: model.

    "Window options."
    window := AzulNative azWindowCreateOptionsCreate: LayoutCallback.
    window window_state title: (self azStringFromString: 'Hello World').
    window window_state size dimensions width: 400.0.
    window window_state size dimensions height: 300.0.
    "NoTitleAutoInject: OS draws close/min/max buttons,
     framework auto-injects a Titlebar with drag support."
    window window_state flags
        decorations: AzWindowDecorations NoTitleAutoInject.
    window window_state flags
        background_material: AzWindowBackgroundMaterial Sidebar.

    "Run."
    app := AzulApp create: data appConfig: AzulAppConfig create.
    app run: window.

    "AzulApp's #finalize will call AzApp_delete the next time GC reclaims the
     wrapper; we let Pharo's WeakRegistry handle it. If you want deterministic
     teardown, send #finalize explicitly here."
]

"=============================================================================
 Do-it block — runs on file-in.
============================================================================="

AzulHelloWorld run.
