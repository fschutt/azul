;;;; examples/lisp/hello-world.lisp
;;;;
;;;; Minimal Common Lisp smoke test for the Azul host-invoker plumbing.
;;;; Confirms that CFFI loads the bindings, the dylib initialises, and
;;;; the host-invoker init phase (refany-create / refany-get) round-trips
;;;; a managed object.
;;;;
;;;; Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
;;;; the wrapper layer's idiomatic API surface to settle — separate work,
;;;; not host-invoker. The C# / Java / Kotlin / Node / PowerShell / Ruby
;;;; hello-worlds have the same shape; all seven verify the FFI plumbing
;;;; one level above libffi.
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
;;;;   * libazul.so / libazul.dylib / azul.dll on the search path, or in
;;;;     the same directory as this script, or via AZUL_LIB_DIR.

(eval-when (:compile-toplevel :load-toplevel :execute)
  (require 'asdf)
  (asdf:load-system :cffi))

(load (merge-pathnames "../../target/codegen/azul.lisp"
                       (or *load-pathname* *compile-file-pathname*)))

(in-package :azul)

(defparameter *model* (list :counter 5))
(defparameter *data*  (refany-create *model*))
(format t "[azul] refany-create ran; RefAny opaque-handle id stored.~%")

;; refany-get takes a *pointer to* an AzRefAny — the framework hands
;; that to callbacks. Marshal *data* via with-foreign-object so CFFI
;; gets a pointer.
(cffi:with-foreign-object (p '(:struct azul-internal::az-ref-any))
  (setf (cffi:mem-ref p '(:struct azul-internal::az-ref-any)) *data*)
  (let ((recovered (refany-get p)))
    (cond
      ((and (listp recovered) (= (getf recovered :counter) 5))
       (format t "[azul] refany-get round-trip succeeded; counter=~A~%"
               (getf recovered :counter)))
      (t
       (format t "[azul] refany-get round-trip FAILED (recovered=~A)~%" recovered)
       (sb-ext:exit :code 1)))))

(format t "[azul] host-invoker init phase completed successfully.~%")
(format t "[azul] (Full App.run wiring requires wrapper-layer API surface~%")
(format t "[azul]  fixes that are separate from the host-invoker plumbing.)~%")
