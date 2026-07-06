#lang racket/base
;;;; hello-world.rkt — Azul counter example (Racket / ffi/unsafe).
;;;;
;;;; Run (matrix invocation):
;;;;   AZ_LIB_DIR=. racket hello-world.rkt
;;;;
;;;; azul.rkt (the generated binding) must sit next to this file, and the
;;;; native library libazul.{so,dylib,dll} must be reachable via the
;;;; dynamic loader or AZ_LIB_DIR.
;;;;
;;;; Under AZ_BACKEND=headless + AZ_E2E=<scenario.json> the DLL's headless
;;;; runner replays the scenario (render, assert "5", click x3 -> 6,7,8...)
;;;; and prints "test result: ok". A real windowed run happens otherwise.
;;;;
;;;; ── GC-RETENTION GOTCHA (the whole reason this file keeps callbacks in
;;;;    module-level `define`s) ──────────────────────────────────────────
;;;; Racket's `_fun` turns a closure into a REAL C function pointer via a
;;;; libffi ffi_closure. That closure is only kept alive while the Racket
;;;; procedure it wraps stays reachable. `on-click` and `layout` are bound
;;;; at module top level (a permanent root); register-callback additionally
;;;; stores each in azul.rkt's `azul-handles` hash keyed by host-handle id.
;;;; A callback stored only in a let that goes out of scope WILL be GC'd and
;;;; the next click crashes — so keep them rooted.

(require "azul.rkt")

;; ── Data model ────────────────────────────────────────────────────────
;; A mutable box; the same object is recovered by refany-get, but the
;; callbacks below also close over it directly for clarity.
(define model (box 5))

;; ── Helpers ───────────────────────────────────────────────────────────
;; Copy a Racket string into a fresh AzString (a byte string is a legal C
;; pointer, so it satisfies the const uint8_t* arg directly).
(define (az-str s)
  (define b (string->bytes/utf-8 s))
  (AzString_copyFromBytes b 0 (bytes-length b)))

;; ── Callbacks (module-level → GC-retained; see header) ─────────────────
;; on-click: return an Update enum (DoNothing = 0, RefreshDom = 1). The
;; framework hands the RefAny by pointer; we mutate the model in place.
(define (on-click data-ptr info-ptr)
  (set-box! model (add1 (unbox model)))
  AzUpdate_RefreshDom)

;; layout: returns an AzDom; the host-invoker memcpys its bytes through the
;; out-pointer using the type's true C size (ctype-sizeof _AzDom = 240).
(define (layout data-ptr info-ptr)
  (define counter (unbox model))
  ;; Counter label inside a font-size:32px div.
  (define label (dom-create-text (az-str (number->string counter))))
  (define wrap (dom-create-div))
  (dom-add-css-property
   wrap
   (css-property-with-conditions-simple
    (css-property-font-size (style-font-size-px 32.0))))
  (dom-add-child wrap label)
  ;; "Increase counter" button; button-set-on-click registers on-click
  ;; through the host-invoker (Struct-variant C ABI carries the ctx).
  (define btn (button-create (az-str "Increase counter")))
  (button-set-button-type btn AzButtonType_Primary)
  (define click-data (refany-create model))
  (button-set-on-click btn click-data on-click)
  (define btn-dom (button-dom btn))
  ;; Body.
  (define body (dom-create-body))
  (dom-add-child body wrap)
  (dom-add-child body btn-dom)
  body)

;; ── Main ──────────────────────────────────────────────────────────────
;; AzWindowCreateOptions_create takes a bare LayoutCallbackType fn-ptr and
;; DISCARDS the host-invoker ctx, so we build a default WCO and splice the
;; registered AzLayoutCallback wrapper struct (which carries the ctx) into
;; window_state.layout_callback via the generated cstruct setter.
(define (run-app)
  (define data (refany-create model))
  (define app (app-create data (app-config-create)))

  (define wco (make-window-create-options))
  (define ws (AzWindowCreateOptions-window-state wco))
  (set-AzFullWindowState-title! ws (az-str "Hello World"))
  (set-AzFullWindowState-layout-callback! ws (register-callback "LayoutCallback" layout))

  ;; Cosmetic window sizing / chrome (matches the C example).
  (define sz (AzFullWindowState-size ws))
  (define dims (AzWindowSize-dimensions sz))
  (set-AzLogicalSize-width! dims 400.0)
  (set-AzLogicalSize-height! dims 300.0)
  (define flags (AzFullWindowState-flags ws))
  (set-AzWindowFlags-decorations! flags AzWindowDecorations_NoTitleAutoInject)
  (set-AzWindowFlags-background-material! flags AzWindowBackgroundMaterial_Sidebar)

  (app-run app wco))

(run-app)
