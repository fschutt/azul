;;;; hello-world.lisp — Azul counter example (Common Lisp / CFFI).
;;;; Loaded as the :azul-example ASDF system's component; run via
;;;; (ql:quickload :azul-example) then (azul-hello:run-app).
;;;; Do NOT (load "azul.lisp") here — ASDF already loaded it via the :azul
;;;; dependency; a manual load resolves into the fasl-cache dir and fails.

(defpackage #:azul-hello
  (:use #:cl)
  (:export #:run-app))

(in-package #:azul-hello)

;; A cons cell so the counter is mutable in place through the RefAny
;; host-handle table (the same Lisp object is recovered by REFANY-GET).
(defun make-model () (cons :counter 5))
(defun model-counter (m) (cdr m))
(defun (setf model-counter) (v m) (setf (cdr m) v))

(defun az-str (s)
  "Copy a Lisp string into a fresh AzString and return the CLOS wrapper
   (use AZUL:AZ-STRING-PTR to get the raw by-value struct)."
  (let* ((bytes (sb-ext:string-to-octets s :external-format :utf-8))
         (len   (length bytes))
         (buf   (cffi:foreign-alloc :uint8 :count len)))
    (dotimes (i len) (setf (cffi:mem-aref buf :uint8 i) (aref bytes i)))
    (prog1 (azul:az-string-from-utf8 buf len)
      (cffi:foreign-free buf))))

;; on-click: return value is an Update enum (0 = DoNothing, 1 = RefreshDom).
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
           (counter-dom  (azul:make-dom-create-text (azul:az-string-ptr (az-str counter-text))))
           (label-div    (azul:make-dom-create-div))
           (label-div    (azul:dom-with-css label-div
                                            (azul:az-string-ptr (az-str "font-size: 32px;"))))
           (label-div    (azul:dom-with-child label-div (azul:dom-ptr counter-dom)))
           (button       (azul:make-button-create (azul:az-string-ptr (az-str "Increase counter"))))
           (button       (azul:button-with-button-type button 1)) ; Primary
           (click-data   (azul:refany-create m))
           (button       (azul:button-with-on-click button click-data #'on-click))
           (button-dom-s (azul:button-dom button))
           (body         (azul:make-dom-create-body))
           (body         (azul:dom-with-child body (azul:dom-ptr label-div)))
           ;; BUTTON-DOM returns a raw AzDom struct value (not a CLOS
           ;; wrapper), so pass it straight through — no DOM-PTR.
           (body         (azul:dom-with-child body button-dom-s)))
      body)))

;; Opaque WCO transport: AzWindowCreateOptions (1336 bytes) is full of nested
;; tagged unions whose inactive-variant bytes aren't valid enum values, so
;; CFFI's by-value struct translation errors validating them. We never read
;; those fields, so we transport the WCO as a same-sized struct of plain
;; :uint64 words — ABI-identical (a >16-byte struct is passed by hidden
;; pointer regardless of field types) but with no enum walking.
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

;; AzWindowCreateOptions_create takes a bare fn-ptr and DISCARDS the host-invoker
;; ctx, so we build a default WCO and splice the registered AzLayoutCallback
;; wrapper struct (which carries the ctx) into window_state.layout_callback.
(defun run-app ()
  (assert (zerop (mod (cffi:foreign-type-size '(:struct azul-internal::az-window-create-options)) 8)))
  (let* ((model (make-model))
         (data  (azul:refany-create model))
         (cfg   (azul:make-app-config-create))
         (app   (azul:make-app-create data (azul:app-config-ptr cfg)))
         (wco-ptr (cffi:foreign-alloc '(:struct wco-words))))
    (setf (cffi:mem-ref wco-ptr '(:struct wco-words)) (%wco-default-words))
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
    ;; AzApp_run takes `const AzApp*`. The app wrapper's `app-ptr` slot holds
    ;; the CFFI-translated AzApp *value*, not a pointer, so materialize it in
    ;; foreign memory and pass its address.
    (let ((app-buf (cffi:foreign-alloc '(:struct azul-internal::az-app))))
      (setf (cffi:mem-ref app-buf '(:struct azul-internal::az-app)) (azul:app-ptr app))
      (%app-run-words app-buf (cffi:mem-ref wco-ptr '(:struct wco-words))))))
