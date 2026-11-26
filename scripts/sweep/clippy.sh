#!/usr/bin/env bash
# The clippy sweep's runner command: the feature set the tests compile (all findings as JSON, for the
# sweep's tools) and the default features under -D warnings (the first step of the CI's clippy job).
# Writes clip.json.gz, clip.err and def.txt in the repository root.
set +e
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater,azul-layout/http
P="-p azul-core -p azul-css -p azul-layout"
cargo clippy $P --all-targets --features $F --message-format=json -- -W warnings --cap-lints warn > clip.json 2> clip.err
echo "feature set, all warnings: exit $?"
tail -4 clip.err
grep -c "^error" clip.err
cargo clippy $P --all-targets -- -D warnings > def.txt 2>&1
echo "default features, -D warnings (the first CI step): exit $?"
tail -6 def.txt
gzip -9 clip.json
ls -la clip.json.gz def.txt
true
