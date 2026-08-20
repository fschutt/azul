#lang racket/base
;;;; GC-RETENTION GOTCHA: a Racket procedure passed as a callback becomes a
;;;; libffi closure that lives only while the procedure stays reachable. Keep
;;;; `on-click` / `layout` as module-level `define`s (a permanent root) — a
;;;; callback stored only in an escaping let gets GC'd and the next click crashes.

(require "azul.rkt")

(define model (box 5))

(define az-str string->azul-string)

(define (on-click data-ptr info-ptr)
  (set-box! model (add1 (unbox model)))
  AzUpdate_RefreshDom)

(define (layout data-ptr info-ptr)
  (define counter (unbox model))
  (define label
    (dom-create-text-do-not-use-without-block-level-wrapper
     (az-str (number->string counter))))
  (define wrap (dom-create-div))
  (dom-add-css-property
   wrap
   (css-property-with-conditions-simple
    (css-property-font-size (style-font-size-px 32.0))))
  (dom-add-child wrap label)
  (define btn (button-create (az-str "Increase counter")))
  (button-set-button-type btn AzButtonType_Primary)
  (define click-data (refany-create model))
  (button-set-on-click btn click-data on-click)
  (define btn-dom (button-dom btn))
  (define body (dom-create-body))
  (dom-add-child body wrap)
  (dom-add-child body btn-dom)
  body)

;; AzWindowCreateOptions_create takes a bare fn-ptr and discards the ctx, so we
;; splice the registered layout wrapper (which carries it) into the window state.
(define (run-app)
  (define data (refany-create model))
  (define app (app-create data (app-config-create)))

  (define wco (make-window-create-options))
  (define ws (AzWindowCreateOptions-window-state wco))
  (set-AzFullWindowState-title! ws (az-str "Hello World"))
  (set-AzFullWindowState-layout-callback! ws (register-callback "LayoutCallback" layout))

  (define sz (AzFullWindowState-size ws))
  (define dims (AzWindowSize-dimensions sz))
  (set-AzLogicalSize-width! dims 400.0)
  (set-AzLogicalSize-height! dims 300.0)
  (define flags (AzFullWindowState-flags ws))
  (set-AzWindowFlags-decorations! flags AzWindowDecorations_NoTitleAutoInject)
  (set-AzWindowFlags-background-material! flags AzWindowBackgroundMaterial_Sidebar)

  (app-run app wco))

(run-app)
