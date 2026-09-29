;;;; Put target/codegen/azul.asd + azul.lisp where ASDF finds them and libazul on the
;;;; loader path, then:
;;;;   sbcl --eval '(asdf:load-system :azul-styles)' --eval '(azul-styles::main)' --quit
(in-package #:azul-styles)

(defun main ())
