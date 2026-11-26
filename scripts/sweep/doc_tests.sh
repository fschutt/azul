#!/usr/bin/env bash
# The clippy sweep's runner command: the CI's azul_doc_tests job (the 33-language golden tests among them).
# Run it with --codegen: the tests read the generated bindings.
set +e
cargo test -r -p azul-doc --bins --no-fail-fast > doc-tests.txt 2>&1
echo "azul-doc tests exit $?"
grep -E "^test result: FAILED|FAILED|panicked|^error" doc-tests.txt | head -40
grep -E "^test result" doc-tests.txt | awk '{p+=$4; f+=$6} END {print "azul-doc: " p " passed, " f " failed"}'
cargo run -r -p azul-doc check 2>&1 | tail -15
true
