;;;; azul-example.asd
;;;;
;;;; ASDF system definition for the Azul Common Lisp example program.
;;;; Drop this directory into ~/quicklisp/local-projects/ (or any other
;;;; ASDF source-registry-visible location), then:
;;;;
;;;;   (ql:quickload :azul-example)
;;;;   (azul-hello:run-app)
;;;;
;;;; Depends on the generated `:azul` system, which itself depends on
;;;; `:cffi`. The native `libazul` shared library must be on the
;;;; dynamic loader's search path at load time.

(asdf:defsystem #:azul-example
  :description     "Hello-world example using the Azul Common Lisp bindings"
  :author          "Azul contributors"
  :license         "MPL-2.0 OR MIT OR Apache-2.0"
  :version         "0.1.0"
  :depends-on      (#:azul #:cffi #:babel)
  :serial          t
  :components ((:file "hello-world")))
