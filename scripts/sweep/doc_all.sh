#!/usr/bin/env bash
# The clippy sweep's runner command: the autofix fixpoint, then the azul-doc tests (run it with --codegen).
set +e
bash scripts/sweep/autofix.sh
bash scripts/sweep/doc_tests.sh
true
