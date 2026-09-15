#lang racket/base

(require "azul.rkt")

(define model (box 5))

(define az-str string->azul-string)

(define (on-click data-ptr info-ptr)
  (set-box! model (add1 (unbox model)))
  AzUpdate_RefreshDom)

(define (layout data-ptr info-ptr)
  (define counter (unbox model))
  (define label (dom-create-p-with-text (az-str (number->string counter))))
  (dom-set-css label (az-str "font-size: 32px; margin: 0;"))
  (define btn (button-create (az-str "Increase counter")))
  (button-set-button-type btn AzButtonType_Primary)
  (define click-data (refany-create model))
  (button-set-on-click btn click-data on-click)
  (define btn-dom (button-dom btn))
  (define body (dom-create-body))
  (dom-add-child body label)
  (dom-add-child body btn-dom)
  body)

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

  (app-run app wco))

(run-app)
