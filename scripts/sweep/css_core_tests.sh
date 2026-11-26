#!/usr/bin/env bash
# The clippy sweep's runner command: the tests of the code generators (azul-css with codegen and parser,
# azul-core with codegen), every test binary run even after a failure.
set +e
cargo test -p azul-css --features codegen,parser --no-fail-fast > css-tests.txt 2>&1
echo "css exit $?"
grep -E "^test result: FAILED|FAILED|panicked|^error" css-tests.txt | head -40
grep -E "^test result" css-tests.txt | awk '{p+=$4; f+=$6} END {print "css: " p " passed, " f " failed"}'
cargo test -p azul-core --features codegen --no-fail-fast > core-tests.txt 2>&1
echo "core exit $?"
grep -E "^test result: FAILED|FAILED|panicked|^error" core-tests.txt | head -40
grep -E "^test result" core-tests.txt | awk '{p+=$4; f+=$6} END {print "core: " p " passed, " f " failed"}'
true
