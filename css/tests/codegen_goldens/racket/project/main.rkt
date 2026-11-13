#lang racket/base
;; Copy target/codegen/azul.rkt and libazul here, then:
;;   AZ_LIB_DIR=. racket main.rkt
(require "azul.rkt" "styles.rkt")

(define style-btn-value (style-btn))
(printf "style-btn: ~a properties~n" (AzCssPropertyWithConditionsVec-len style-btn-value))
