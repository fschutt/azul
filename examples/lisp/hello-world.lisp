;;;; examples/lisp/hello-world.lisp
;;;;
;;;; Common Lisp port of examples/c/hello-world.c. Same data model (a
;;;; counter), same behaviour (mouse click increments, layout rebuilds
;;;; the DOM). Callbacks go through libazul's host-invoker plumbing
;;;; (`%az-callback-create-from-host-handle`, `%az-app-set-callback-
;;;; invoker`) so the CFFI/libffi closure cast stays legal (pointer
;;;; args throughout).
;;;;
;;;; Run with:
;;;;   sbcl --dynamic-space-size 8192 --noinform --non-interactive \
;;;;        --load ~/quicklisp/setup.lisp \
;;;;        --eval "(ql:quickload '(:cffi :cffi-libffi) :silent t)" \
;;;;        --load hello-world.lisp
;;;;
;;;; Requires:
;;;;   * SBCL (or other CFFI-supported Lisp).
;;;;   * Quicklisp with `cffi` + `cffi-libffi` (struct-by-value calls).
;;;;   * libazul.so / libazul.dylib / azul.dll on the search path or
;;;;     next to this script.

(eval-when (:compile-toplevel :load-toplevel :execute)
  (require 'asdf)
  (asdf:load-system :cffi))

(load (merge-pathnames "azul.lisp"
                       (or *load-pathname* *compile-file-pathname*)))

(in-package :azul)

;; ── Data model ────────────────────────────────────────────────────────
;; A plist is fine for our purposes — RefAny just holds an opaque Lisp
;; value reachable through the host-handle table.
(defparameter *model* (list :counter 5))

;; ── Helpers ──────────────────────────────────────────────────────────
(defun az-str (s)
  "Copy a Lisp string into a fresh `(:struct az-string)` and return the
   wrapper instance (use its `az-string-ptr` slot to get the raw struct)."
  (let* ((bytes  (sb-ext:string-to-octets s :external-format :utf-8))
         (len    (length bytes))
         (buf    (cffi:foreign-alloc :uint8 :count len)))
    (loop for i from 0 below len
          do (setf (cffi:mem-aref buf :uint8 i) (aref bytes i)))
    (let ((s (az-string-from-utf8 buf len)))
      (cffi:foreign-free buf)
      s)))

;; ── Callbacks ────────────────────────────────────────────────────────
;; The codegen-emitted host-invoker thunk dispatches via a uint64
;; handle id; user closures register themselves via
;; `register-callback`. Return values:
;;   * `Callback` (click): integer (Update enum: 0=DoNothing, 1=Refresh).
;;   * `LayoutCallback`: a `dom` CLOS instance whose `dom-ptr` slot
;;     carries the AzDom struct value. The invoker memcpys it through
;;     the out-pointer.

(defun on-click (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (refany-get data-ptr)))
    (cond
      ((listp m)
       (incf (getf m :counter))
       1)               ; Update.RefreshDom
      (t 0))))          ; Update.DoNothing

(defun layout-cb (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (refany-get data-ptr)))
    (cond
      ((not (listp m))
       (make-dom-create-body))
      (t
       ;; Counter label wrapped in a font-size-32 div.
       (let* ((counter-text (princ-to-string (getf m :counter)))
              (counter-dom  (make-dom-create-text (az-string-ptr (az-str counter-text))))
              (label-div    (make-dom-create-div))
              (label-div    (dom-with-css label-div
                                          (az-string-ptr (az-str "font-size: 32px;"))))
              (label-div    (dom-with-child label-div (dom-ptr counter-dom)))

              ;; Increment button. `button-with-on-click` auto-registers
              ;; the closure via the host-invoker pattern.
              (button       (make-button-create (az-string-ptr (az-str "Increase counter"))))
              (button       (button-with-button-type button 1)) ; Primary
              (click-data   (refany-create m))
              (button       (button-with-on-click button click-data #'on-click))
              (button-dom-s (button-dom button))

              ;; Body.
              (body         (make-dom-create-body))
              (body         (dom-with-child body (dom-ptr label-div)))
              (body         (dom-with-child body button-dom-s)))
         body)))))

;; ── Main ─────────────────────────────────────────────────────────────
;; The codegen's `make-window-create-options-create` routes through
;; `AzWindowCreateOptions_create(AzLayoutCallbackType)`, which takes a
;; raw fn pointer and discards the host-invoker ctx. Build a default
;; WCO and rebuild it with the layout_callback field set via foreign
;; slot mutation.

(let* ((data (refany-create *model*))
       (cfg  (make-app-config-create))
       (app  (make-app-create data (app-config-ptr cfg)))
       (wco-ptr (cffi:foreign-alloc '(:struct azul-internal::az-window-create-options)))
       (wco-default (azul-internal::%az-window-create-options-default)))
  ;; Write the default WCO bytes into our heap buffer, then mutate the
  ;; nested layout_callback field through the pointer.
  (setf (cffi:mem-ref wco-ptr '(:struct azul-internal::az-window-create-options))
        wco-default)
  ;; Compute the offset of window_state.layout_callback within the WCO
  ;; struct via CFFI's foreign-slot-offset, then write the registered
  ;; AzLayoutCallback struct at that offset.
  (let* ((layout-cb-struct (register-callback "LayoutCallback" #'layout-cb))
         (ws-offset (cffi:foreign-slot-offset
                     '(:struct azul-internal::az-window-create-options)
                     'azul-internal::window-state))
         (lc-offset (cffi:foreign-slot-offset
                     '(:struct azul-internal::az-full-window-state)
                     'azul-internal::layout-callback))
         (lc-ptr (cffi:inc-pointer wco-ptr (+ ws-offset lc-offset))))
    (setf (cffi:mem-ref lc-ptr '(:struct azul-internal::az-layout-callback))
          layout-cb-struct))
  ;; Read the (now-mutated) WCO struct value back out and pass by value.
  (let ((wco-by-val (cffi:mem-ref wco-ptr
                                  '(:struct azul-internal::az-window-create-options))))
    (cffi:foreign-free wco-ptr)
    (app-run app wco-by-val)))
