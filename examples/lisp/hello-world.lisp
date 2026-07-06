;;;; hello-world.lisp — Azul counter example (Common Lisp / CFFI).
;;;;
;;;; Loaded as the sole component of the :azul-example ASDF system, which
;;;; depends on :azul (the generated bindings) + :cffi + :cffi-libffi.
;;;; Do NOT (load "azul.lisp") here — ASDF already compiled/loaded it via
;;;; the :azul dependency; a manual load would resolve the path into the
;;;; fasl-cache directory and fail.
;;;;
;;;; Run (matrix invocation):
;;;;   (ql:quickload :azul-example)
;;;;   (azul-hello:run-app)
;;;;
;;;; Under AZ_BACKEND=headless + AZ_E2E=<scenario.json> the DLL's headless
;;;; runner replays the scenario (render, assert "5", click x3 -> 6,7,8...)
;;;; and prints "test result: ok". A real windowed run happens otherwise.

(defpackage #:azul-hello
  (:use #:cl)
  (:export #:run-app))

(in-package #:azul-hello)

;; ── Data model ────────────────────────────────────────────────────────
;; A cons cell so the counter is mutable in place through the RefAny
;; host-handle table (the same Lisp object is recovered by REFANY-GET).
(defun make-model () (cons :counter 5))
(defun model-counter (m) (cdr m))
(defun (setf model-counter) (v m) (setf (cdr m) v))

;; ── Helpers ──────────────────────────────────────────────────────────
(defun az-str (s)
  "Copy a Lisp string into a fresh AzString and return the CLOS wrapper
   (use AZUL:AZ-STRING-PTR to get the raw by-value struct)."
  (let* ((bytes (sb-ext:string-to-octets s :external-format :utf-8))
         (len   (length bytes))
         (buf   (cffi:foreign-alloc :uint8 :count len)))
    (dotimes (i len) (setf (cffi:mem-aref buf :uint8 i) (aref bytes i)))
    (prog1 (azul:az-string-from-utf8 buf len)
      (cffi:foreign-free buf))))

;; ── Callbacks ────────────────────────────────────────────────────────
;; on-click: return value is an Update enum (0 = DoNothing, 1 = RefreshDom).
;; The framework hands the callback the RefAny by pointer; recover the
;; model and mutate the counter in place.
(defun on-click (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (azul:refany-get data-ptr)))
    (cond ((consp m) (incf (model-counter m)) 1)   ; Update.RefreshDom
          (t 0))))                                 ; Update.DoNothing

;; layout-cb: returns a DOM CLOS instance; the host-invoker memcpys its
;; AzDom struct bytes through the out-pointer.
(defun layout-cb (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (azul:refany-get data-ptr)))
    (let* ((counter-text (princ-to-string (if (consp m) (model-counter m) 5)))
           ;; Counter label inside a font-size:32px div.
           (counter-dom  (azul:make-dom-create-text (azul:az-string-ptr (az-str counter-text))))
           (label-div    (azul:make-dom-create-div))
           (label-div    (azul:dom-with-css label-div
                                            (azul:az-string-ptr (az-str "font-size: 32px;"))))
           (label-div    (azul:dom-with-child label-div (azul:dom-ptr counter-dom)))
           ;; "Increase counter" button; with-on-click registers ON-CLICK
           ;; through the host-invoker (Struct-variant C ABI).
           (button       (azul:make-button-create (azul:az-string-ptr (az-str "Increase counter"))))
           (button       (azul:button-with-button-type button 1)) ; Primary
           (click-data   (azul:refany-create m))
           (button       (azul:button-with-on-click button click-data #'on-click))
           (button-dom-s (azul:button-dom button))
           ;; Body.
           (body         (azul:make-dom-create-body))
           (body         (azul:dom-with-child body (azul:dom-ptr label-div)))
           ;; BUTTON-DOM returns a raw AzDom struct value (not a CLOS
           ;; wrapper), so pass it straight through — no DOM-PTR.
           (body         (azul:dom-with-child body button-dom-s)))
      body)))

;; ── Opaque WCO transport (word struct) ────────────────────────────────
;; AzWindowCreateOptions is a 1336-byte struct densely populated with
;; nested tagged unions and enum fields (Vsync, Vec destructors, …).
;; CFFI's default by-value struct translation walks EVERY nested slot and
;; validates every enum-typed field — but a live union's inactive variants
;; (and padding) hold bytes that aren't valid enum values ("112 is not a
;; value for AZ-VSYNC"). We never read those fields in Lisp, so we
;; transport the WCO as a struct of plain :uint64 words of exactly the
;; same size. Scalar slots translate losslessly (no enum validation), and
;; a same-sized aggregate is ABI-identical: on arm64/x86-64 a >16-byte
;; struct is passed indirectly (by hidden pointer) regardless of its field
;; types, so `AzApp_run`/`AzWindowCreateOptions_default` see the same
;; bytes either way.
(eval-when (:compile-toplevel :load-toplevel :execute)
  (defmacro def-word-struct (name n)
    `(cffi:defcstruct ,name
       ,@(loop for i below n
               collect (list (intern (format nil "W~D" i) :azul-hello) :uint64)))))

(def-word-struct wco-words
  #.(/ (cffi:foreign-type-size '(:struct azul-internal::az-window-create-options)) 8))

(cffi:defcfun ("AzWindowCreateOptions_default" %wco-default-words)
    (:struct wco-words))

(cffi:defcfun ("AzApp_run" %app-run-words) :void
  (app  :pointer)
  (root (:struct wco-words)))

;; ── Main ─────────────────────────────────────────────────────────────
;; AzWindowCreateOptions_create takes a bare LayoutCallbackType fn-ptr and
;; DISCARDS the host-invoker ctx, so we build a default WCO and splice the
;; registered AzLayoutCallback wrapper struct (which carries the ctx) into
;; window_state.layout_callback by foreign-slot offset.
(defun run-app ()
  (assert (zerop (mod (cffi:foreign-type-size '(:struct azul-internal::az-window-create-options)) 8)))
  (let* ((model (make-model))
         (data  (azul:refany-create model))
         (cfg   (azul:make-app-config-create))
         (app   (azul:make-app-create data (azul:app-config-ptr cfg)))
         (wco-ptr (cffi:foreign-alloc '(:struct wco-words))))
    ;; Default WCO bytes into our heap buffer (word-wise, no enum walking).
    (setf (cffi:mem-ref wco-ptr '(:struct wco-words)) (%wco-default-words))
    ;; Splice the registered layout callback into window_state.layout_callback
    ;; using the REAL AzLayoutCallback type (small, translates cleanly).
    (let* ((layout-cb-struct (azul:register-callback "LayoutCallback" #'layout-cb))
           (ws-offset (cffi:foreign-slot-offset
                       '(:struct azul-internal::az-window-create-options)
                       'azul-internal::window-state))
           (lc-offset (cffi:foreign-slot-offset
                       '(:struct azul-internal::az-full-window-state)
                       'azul-internal::layout-callback))
           (lc-ptr (cffi:inc-pointer wco-ptr (+ ws-offset lc-offset))))
      (setf (cffi:mem-ref lc-ptr '(:struct azul-internal::az-layout-callback))
            layout-cb-struct))
    ;; AzApp_run takes `const AzApp*` — a POINTER to the AzApp struct. The
    ;; app wrapper's `app-ptr` slot holds the CFFI-translated AzApp *value*
    ;; (a plist {ptr, run-destructor}), not a pointer, so materialize it in
    ;; foreign memory and pass its address.
    (let ((app-buf (cffi:foreign-alloc '(:struct azul-internal::az-app))))
      (setf (cffi:mem-ref app-buf '(:struct azul-internal::az-app)) (azul:app-ptr app))
      ;; Pass the mutated WCO by value (word struct) and run.
      (%app-run-words app-buf (cffi:mem-ref wco-ptr '(:struct wco-words))))))
