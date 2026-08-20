(asdf:defsystem #:azul-example
  :description     "Hello-world example using the Azul Common Lisp bindings"
  :author          "Azul contributors"
  :license         "MPL-2.0 OR MIT OR Apache-2.0"
  :version         "0.1.0"
  :depends-on      (#:azul #:cffi #:babel)
  :serial          t
  :components ((:file "hello-world")))
