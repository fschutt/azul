#!/usr/bin/env bash
# The clippy sweep's runner command: what the feature set of the CI's clippy step leaves out. Lints azul-css, azul-core and
# azul-layout with the test step's features PLUS the other cross-platform features of azul-layout (the debug server's
# transport, telemetry, crash mail, sign-in sessions, pdf, icu, fluent, the TLS test authorities, the WPT runner), azul-css io and
# azul-core test-hooks, all as warnings, as JSON for the sweep's tools. A measurement, not a gate.
set +e
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater,azul-layout/e2e-server-http,azul-layout/telemetry,azul-layout/crash-mail,azul-layout/auth_session,azul-layout/pdf,azul-layout/icu_chrono,azul-layout/fluent,azul-layout/tls-testing,azul-layout/wpt_tests,azul-css/io,azul-core/test-hooks
P="-p azul-core -p azul-css -p azul-layout"
cargo clippy $P --all-targets --features $F --message-format=json -- -W warnings --cap-lints warn > more.json 2> more.err
echo "more features, all warnings: exit $?"
tail -6 more.err
grep -c "^error" more.err
gzip -9 more.json
ls -la more.json.gz
true
