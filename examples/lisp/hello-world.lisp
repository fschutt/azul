;;;; examples/lisp/hello-world.lisp
;;;;
;;;; Common Lisp port of examples/c/hello-world.c.
;;;;
;;;; Same data model (a `MyDataModel` struct with a uint32 counter),
;;;; same callback semantics (mouse click increments, layout renders).
;;;;
;;;; Usage (assuming Quicklisp is set up and the worktree's `azul.asd`
;;;; is reachable, e.g. symlinked into ~/quicklisp/local-projects/):
;;;;
;;;;   (ql:quickload :azul)
;;;;   (load "hello-world.lisp")
;;;;   (run-app)
;;;;
;;;; --------------------------------------------------------------------
;;;; CFFI callback lifetime caveat (READ THIS):
;;;;
;;;; `cffi:defcallback` allocates a stable trampoline whose lifetime is
;;;; bound to the *symbol* it is named after. We therefore define every
;;;; callback at the top level via `defcallback` (not as anonymous
;;;; closures); this guarantees the C side never jumps into freed
;;;; memory.
;;;; --------------------------------------------------------------------

(in-package :cl-user)

(defpackage :azul-hello
  (:use :cl :cffi)
  (:export #:run-app))

(in-package :azul-hello)

;; ── Data model ───────────────────────────────────────────────────────
;;
;; Declare a CFFI struct so the counter lives in C-allocated memory.
;; This is required because AzRefAny_newC copies sizeof(MyDataModel)
;; bytes from the supplied pointer into a heap-allocated RefAny.

(defcstruct my-data-model
  (counter :uint32))

;; A unique 64-bit RTTI id for this type. Any stable value works as
;; long as no other RefAny in the process uses the same id.
(defparameter +my-data-model-rtti-id+ #xA2010001)

;; Destructor stub -- MyDataModel owns no heap memory.
(defcallback model-destructor :void ((ptr :pointer))
  (declare (ignore ptr)))

(defun upcast-model (ptr len)
  "Upcast a (pointer to my-data-model) into an AzRefAny."
  (let ((name (foreign-string-alloc "MyDataModel")))
    (unwind-protect
         ;; AzString_copyFromBytes(bytes, start, len)
         (let ((name-str (foreign-funcall "AzString_copyFromBytes"
                                          :pointer name
                                          :size 0
                                          :size (length "MyDataModel")
                                          :pointer)))
           ;; AzRefAny_newC(ptr, len, align, type_id, type_name, destructor)
           (foreign-funcall "AzRefAny_newC"
                            :pointer ptr
                            :size len
                            :size (foreign-type-alignment '(:struct my-data-model))
                            :uint64 +my-data-model-rtti-id+
                            :pointer name-str
                            :pointer (callback model-destructor)
                            :pointer))
      (foreign-string-free name))))

(defun downcast-mut (refany)
  "Borrow a *mut my-data-model from a RefAny pointer; nil on RTTI mismatch."
  (when (foreign-funcall "AzRefAny_isType"
                         :pointer refany
                         :uint64 +my-data-model-rtti-id+
                         :bool)
    (foreign-funcall "AzRefAny_getDataPtr" :pointer refany :pointer)))

;; ── Callback: increment the counter on click ─────────────────────────

(defcallback on-click :uint32 ((data :pointer) (info :pointer))
  (declare (ignore info))
  (let ((m (downcast-mut data)))
    (cond
      ((null m)
       ;; AzUpdate_DoNothing
       0)
      (t
       (incf (foreign-slot-value m '(:struct my-data-model) 'counter))
       ;; AzUpdate_RefreshDom
       1))))

;; ── Layout callback ──────────────────────────────────────────────────

(defun make-az-string (s)
  "Build an AzString from a Lisp string via AzString_copyFromBytes."
  (let* ((octets (babel:string-to-octets s :encoding :utf-8))
         (len    (length octets)))
    (with-foreign-object (buf :uint8 len)
      (loop for i below len do
        (setf (mem-aref buf :uint8 i) (aref octets i)))
      (foreign-funcall "AzString_copyFromBytes"
                       :pointer buf
                       :size 0
                       :size len
                       :pointer))))

(defcallback layout :pointer ((data :pointer) (info :pointer))
  (declare (ignore info))
  (let ((m (downcast-mut data)))
    (if (null m)
        ;; SKIPPED: returning a stack value is awkward in CFFI; punt to
        ;; AzDom_createBody and let the framework render an empty body.
        (foreign-funcall "AzDom_createBody" :pointer)
        (let* ((counter-str (write-to-string
                             (foreign-slot-value m '(:struct my-data-model) 'counter)))
               (label-text  (make-az-string counter-str))
               (label       (foreign-funcall "AzDom_createText"
                                             :pointer label-text :pointer))
               (label-wrap  (foreign-funcall "AzDom_createDiv" :pointer)))
          ;; Apply font-size: 32px to the wrapper.
          ;; SKIPPED: full CSS-property fluent builder is verbose; the
          ;; minimal port omits the style and lets the default apply.
          (foreign-funcall "AzDom_addChild"
                           :pointer label-wrap
                           :pointer label
                           :void)
          ;; Button.
          (let* ((btn-text (make-az-string "Increase counter"))
                 (button   (foreign-funcall "AzButton_create"
                                            :pointer btn-text :pointer))
                 (data-clone (foreign-funcall "AzRefAny_clone"
                                              :pointer data :pointer)))
            ;; AzButton_setButtonType(button, AzButtonType_Primary=0)
            (foreign-funcall "AzButton_setButtonType"
                             :pointer button
                             :uint32 0
                             :void)
            (foreign-funcall "AzButton_setOnClick"
                             :pointer button
                             :pointer data-clone
                             :pointer (callback on-click)
                             :void)
            (let* ((button-dom (foreign-funcall "AzButton_dom"
                                                :pointer button :pointer))
                   (body       (foreign-funcall "AzDom_createBody" :pointer)))
              (foreign-funcall "AzDom_addChild"
                               :pointer body :pointer label-wrap :void)
              (foreign-funcall "AzDom_addChild"
                               :pointer body :pointer button-dom :void)
              ;; Style the body with the empty CSS.
              (foreign-funcall "AzDom_style"
                               :pointer body
                               :pointer (foreign-funcall "AzCss_empty" :pointer)
                               :pointer)))))))

;; ── Main ─────────────────────────────────────────────────────────────

(defun run-app ()
  "Construct and run the hello-world app."
  (with-foreign-object (model '(:struct my-data-model))
    (setf (foreign-slot-value model '(:struct my-data-model) 'counter) 5)
    (let* ((data   (upcast-model model
                                 (foreign-type-size '(:struct my-data-model))))
           (window (foreign-funcall "AzWindowCreateOptions_create"
                                    :pointer (callback layout)
                                    :pointer))
           (app    (foreign-funcall "AzApp_create"
                                    :pointer data
                                    :pointer (foreign-funcall "AzAppConfig_create"
                                                              :pointer)
                                    :pointer)))
      (unwind-protect
           (foreign-funcall "AzApp_run"
                            :pointer app
                            :pointer window
                            :void)
        (foreign-funcall "AzApp_delete" :pointer app :void)))))

;; To run: (azul-hello:run-app)
