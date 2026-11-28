#!/usr/bin/env bash
# E76: the tests of the layout request's provenance (green on the tree that has it), and the neighbours they could disturb.
set +e
sudo apt-get install -y -qq libv4l-dev fonts-dejavu-core fonts-liberation > /dev/null 2>&1
mkdir -p out
echo "=== a_layout_pass_names_who_asked_for_it + the startup timeline + the first frame"
cargo test -p azul-dll --lib -- a_layout_pass_names_who_asked_for_it startup_timeline first_frame 2>&1 | tee out/test.log | grep -aE "^test |test result|panicked|^error|FAILED" | head -60
echo "TEST_EXIT=${PIPESTATUS[0]}"
