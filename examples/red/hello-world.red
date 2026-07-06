Red/System [
    Title:   "Azul full-GUI hello-world (counter)"
    Purpose: {Counter label + "Increase counter" button, driven from Red/System
              (the dialect where Red's #import FFI lives). Build:

                  redc -r hello-world.red      # then run ./hello-world

              ALPHA / unverified — constructed from the Red/System spec without a
              local Red toolchain to compile-check. See scripts/RED_FFI_FINDINGS.md.}
]

;; Generated bindings (azul.reds sits next to this file).
#include %azul.reds

;; App model: a single counter, starting at 5.
model!: alias struct! [
    counter [integer!]
]

the-model: declare model!
the-model/counter: 5

;; Build an AzString from a c-string! (AzString_fromUtf8 copies the bytes).
mk-str: func [s [c-string!] return: [AzString! value]][
    AzString_fromUtf8 as byte-ptr! s length? s
]

;; ButtonOnClick callback: arg0 = AzRefAny*, out = AzUpdate*.
on-click: func [[cdecl] arg0 [byte-ptr!] arg1 [byte-ptr!] out [byte-ptr!]
    /local praw [byte-ptr!] m [model!] up [int-ptr!]
][
    praw: azul-refany-get arg0
    if praw <> null [
        m: as model! praw
        m/counter: m/counter + 1
    ]
    if out <> null [
        up: as int-ptr! out
        up/value: AzUpdate_RefreshDom
    ]
]

;; Layout callback: body > [ div{font-size:32} > text(counter), button ].
;; arg0 = AzRefAny*, out = AzDom*.
on-layout: func [[cdecl] arg0 [byte-ptr!] arg1 [byte-ptr!] out [byte-ptr!]
    /local praw   [byte-ptr!]
           m      [model!]
           body   [AzDom! value]
           label  [AzDom! value]
           btn    [AzButton! value]
           click-cb   [AzButtonOnClickCallback! value]
           click-data [AzRefAny! value]
           dom-out [AzDom!]
           num     [c-string!]
][
    body: AzDom_createBody
    praw: azul-refany-get arg0
    if praw <> null [
        m: as model! praw
        num: integer/to-string m/counter          ;; Red/System stdlib helper

        label: AzDom_createDiv
        label: AzDom_withCss label mk-str "font-size: 32px;"
        label: AzDom_withChild label (AzDom_createText mk-str num)

        click-cb:   azul-register-button-on-click-callback :on-click
        click-data: azul-refany-create as byte-ptr! the-model
        btn: AzButton_create mk-str "Increase counter"
        btn: AzButton_withButtonType btn AzButtonType_Primary
        btn: AzButton_withOnClick btn click-data click-cb

        body: AzDom_withChild body label
        body: AzDom_withChild body AzButton_dom btn
    ]
    if out <> null [
        dom-out: as AzDom! out
        dom-out/value: body                         ;; write the AzDom by value
    ]
]

main: func [
    /local app-data  [AzRefAny! value]
           layout-cb [AzLayoutCallback! value]
           wco       [AzWindowCreateOptions! value]
           the-app   [AzApp! value]
][
    print-line "[azul] Red/System full-GUI hello-world starting."

    ;; Register the releaser + per-kind invokers with libazul.
    azul-host-invoker-init

    app-data:  azul-refany-create as byte-ptr! the-model
    layout-cb: azul-register-layout-callback :on-layout

    wco: AzWindowCreateOptions_default
    ;; wco.window_state.layout_callback = layout-cb
    ;; wco.window_state.title           = mk-str "Hello World"
    ;; (nested-struct field writes; see guide for the field-path spelling)

    the-app: AzApp_create app-data AzAppConfig_create
    AzApp_run as byte-ptr! the-app wco
]

main
