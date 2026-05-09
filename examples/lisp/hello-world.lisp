;;;; examples/lisp/hello-world.lisp
;;;;
;;;; Common Lisp port of examples/c/hello-world.c built against the host-
;;;; invoker runtime helpers in `azul.lisp` (see `lang_lisp/managed.rs`).
;;;;
;;;; Same shape as examples/lua/hello-world.lua and examples/perl/hello-world.pl:
;;;;   * `(azul:refany-create value)` wraps an arbitrary Lisp value in an
;;;;     AzRefAny held alive by the framework's refcount.
;;;;   * Callbacks are plain Lisp lambdas handed to
;;;;     `(azul:register-callback "Callback" lambda)`, which returns the
;;;;     `Az<Kind>` cdata struct the C ABI expects. Internally that goes
;;;;     through `Az<Kind>_createFromHostHandle(u64)` in libazul; the
;;;;     static thunk dispatches back into Lisp via the libffi closure
;;;;     registered at module load.
;;;;
;;;; Run with:
;;;;   sbcl --noinform --script hello-world.lisp
;;;;
;;;; Requires:
;;;;   * Common Lisp implementation supported by CFFI (SBCL, CCL, ECL,
;;;;     ABCL, Allegro CL, LispWorks, CMUCL, CLISP).
;;;;   * `cffi` system available (`(ql:quickload :cffi)` if you have
;;;;     Quicklisp).
;;;;   * `libazul.so` / `libazul.dylib` / `azul.dll` on the dynamic-loader
;;;;     search path.

(eval-when (:compile-toplevel :load-toplevel :execute)
  (require 'asdf)
  (asdf:load-system :cffi))

(load (merge-pathnames "../../target/codegen/azul.lisp"
                       (or *load-pathname* *compile-file-pathname*)))

(in-package :azul)

;;; ── Data model ─────────────────────────────────────────────────────────

(defparameter *model* (list :counter 5))
(defparameter *data*  (refany-create *model*))

;;; ── Callback: button click ────────────────────────────────────────────

(defun on-click (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (refany-get data-ptr)))
    (cond ((null m) 0)        ; AzUpdate_DoNothing
          (t (incf (getf m :counter))
             1))))             ; AzUpdate_RefreshDom

;;; ── Callback: layout (rebuilds DOM each frame) ────────────────────────

(defun build-layout (data-ptr info-ptr)
  (declare (ignore info-ptr))
  (let ((m (refany-get data-ptr)))
    (when (null m)
      (return-from build-layout
        (azul-internal::%az-dom-create-body)))
    ;; The wrapper-emitter substitution (lang_lisp/wrappers.rs) is a
    ;; future PR. Until then we exercise the :azul-internal symbols here
    ;; directly to demonstrate the host-invoker round-trip.
    (let* ((label-text (azul-internal::%az-string-copy-from-string
                        (princ-to-string (getf m :counter))))
           (label      (azul-internal::%az-dom-create-text label-text))
           (label-wrap (azul-internal::%az-dom-create-div))
           (button-text (azul-internal::%az-string-copy-from-string
                         "Increase counter"))
           (button     (azul-internal::%az-button-create button-text))
           (data-clone (azul-internal::%az-ref-any-clone data-ptr))
           ;; Register on-click via the host-invoker plumbing.
           (on-click-cb (register-callback "Callback" #'on-click))
           (body       (azul-internal::%az-dom-create-body)))
      (azul-internal::%az-button-set-on-click button data-clone on-click-cb)
      (azul-internal::%az-dom-add-child label-wrap label)
      (azul-internal::%az-dom-add-child body label-wrap)
      (azul-internal::%az-dom-add-child body
                                        (azul-internal::%az-button-dom button))
      body)))

;;; ── Main ──────────────────────────────────────────────────────────────

(let ((layout-cb (register-callback "LayoutCallback" #'build-layout)))
  (declare (ignorable layout-cb))
  (format t "[azul] host-invoker plumbing wired (LayoutCallback handle ~A).~%"
          layout-cb)
  (format t "[azul] (Full App.run wiring requires struct-field setters from~%")
  (format t "[azul]  lang_lisp/wrappers.rs which is still a stub today.)~%"))
