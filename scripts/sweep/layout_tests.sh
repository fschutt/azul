#!/usr/bin/env bash
# The clippy sweep's runner command: the layout tests the CI's test_lib job runs, with its feature set
# (the lib tests with the e2e code, the integration tests, layout/tests/all.rs among them).
set +e
export AZ_REQUIRE_TEST_FONTS=1
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater
cargo test -p azul-layout --lib --tests --features $F --no-fail-fast > layout-tests.txt 2>&1
echo "layout exit $?"
grep -E "^test result: FAILED|FAILED|panicked|^error" layout-tests.txt | head -60
grep -E "^test result" layout-tests.txt | awk '{p+=$4; f+=$6} END {print "layout: " p " passed, " f " failed"}'
true
