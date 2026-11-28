#!/usr/bin/env bash
# files_gate.sh: the gate of the files / sharing / open-with work (ledger WP-F) on one Ubuntu runner (or the box), every step with
# its REAL exit code.
#
#   scripts/remote.sh run <full sha or ci-wip ref> ubuntu-22.04 'FILES_ONLY=<step>,<step> bash scripts/ci/files_gate.sh' [--codegen] \
#     --artifacts 'api.json
# api.diff'
#
# Steps (FILES_ONLY names them; none named = all of them):
#   tests      the unit tests of the work by name, in azul-core and azul-layout, with the CI's feature list
#   clippy     the CI's clippy line of the three crates, the test targets included
#   autofix    api.json through azul-doc: `autofix add` for the new functions, patches until there are none, normalize; the
#              result is api.json (upload it) and api.diff (what it changed); then a SECOND round that must change nothing
#   doc_check  azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   dll_check  codegen of the bindings from the api.json as it stands, cargo check of azul-dll, the srcgate tests
#   contracts  the dll/tests source contracts of the work (they read the Java and Rust sources as text; cargo builds the crate first)
#   python     scripts/test_documents_manifest.py (the filters and the plist generated from the app declarations) and the demo manifest checks
# The workspace tests of css / core / layout / webrender in full (the gate before a push) and the Android / iOS compile checks
# (scripts/ci/android_check.sh, scripts/ci/ios_cross_check.sh) are their own runs: `bash scripts/ci/tabs_gate.sh` names the first
# (TABS_ONLY=workspace), the second two are the scripts themselves.
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the
# script is 0 only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${FILES_ONLY:-}"

run() {
  local name="$1"
  shift
  case ",$only," in
    ",,") ;;
    *",$name,"*) ;;
    *) return 0 ;;
  esac
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
}

features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"

tests() {
  local code=0
  # the tests of this work by name: the module names carry them
  cargo test -p azul-css -p azul-core -p azul-layout --lib --features "$features" --no-fail-fast -- \
    dialogs_mobile share_ open_files open_intent open_inbox documents_ mime_ 2>&1 | tee files-tests.txt
  code=${PIPESTATUS[0]}
  return "$code"
}
run tests tests

clippy() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings 2>&1 | tee clippy.txt
  return "${PIPESTATUS[0]}"
}
run clippy clippy

# api.json: `autofix add` stages the functions (a plain `autofix` syncs TYPES only), the patches are applied until the scan finds
# none, and the tree is normalized. The functions are the ones the work added to the API.
api_functions=(
  ShareItem.file
  ShareItem.text
  ShareItem.url
  ShareItem.with_mime
  ShareItem.with_name
  ShareRequest.new
  ShareRequest.file
  ShareRequest.files
  ShareRequest.text
  ShareRequest.url
  ShareRequest.with_item
  ShareRequest.with_title
  ShareRequest.with_anchor
  ShareRequest.with_open_with
  CallbackInfo.share
  CallbackInfo.can_share
  CallbackInfo.set_open_handler
  CallbackInfo.get_open_request
  CallbackInfo.documents
  FileDialog.open_folder
  FolderPickResult.downcast
  FileDialog.export_bytes
  ExportResult.downcast
  Documents.places
  Documents.forget
  Documents.stat
  Documents.children
  Documents.read
  Documents.write
  Documents.create_file
  Documents.create_dir
  Documents.rename
  Documents.delete
  Documents.all_files_access
  Documents.request_all_files_access
  Documents.backend_name
  Documents.origin
  Documents.write_back
)
autofix_round() {
  # a critical FFI error does not stop the round: the patches were written anyway ("fix the errors before proceeding with
  # codegen"), so they are applied, the error is printed in full and the exit code of the LAST round carries it
  local out code=0
  out=$(cargo run -r -p azul-doc -- autofix 2>&1) || code=$?
  echo "$out" | grep -E "Types to add|Type modifications|Path fixes|Generated|^  [+~] " | head -40
  if [ "$code" -ne 0 ]; then
    echo "$out" | grep -A 40 "critical FFI safety errors" | head -80
    echo "$out" | tail -8
  fi
  if echo "$out" | grep -q "Generated 0 patches"; then
    return "$code"
  fi
  cargo run -r -p azul-doc -- autofix explain >/dev/null 2>&1
  cargo run -r -p azul-doc -- patch safe target/autofix/patches || return 1
  cargo run -r -p azul-doc -- patch target/autofix/patches || return 1
  return 0
}
autofix() {
  # all the functions in one run; if that fails (one spec the index cannot place), one by one, so the types and the rest still land
  if [ "${#api_functions[@]}" -gt 0 ]; then
    if ! cargo run -r -p azul-doc -- autofix add "${api_functions[@]}"; then
      echo "autofix add of the whole list failed: one function at a time"
      local f
      for f in "${api_functions[@]}"; do
        cargo run -r -p azul-doc -- autofix add "$f" || echo "ADD FAILED: $f"
      done
    fi
    cargo run -r -p azul-doc -- autofix apply || return 1
  fi
  local i
  for i in 1 2 3 4 5 6; do
    autofix_round || return 1
  done
  cargo run -r -p azul-doc -- normalize || return 1
  git diff api.json > api.diff
  echo "api.json: $(git diff --stat api.json | tail -1)"
  cp api.json api.round1.json
  # round 2 on the result: nothing may change
  autofix_round || return 1
  cargo run -r -p azul-doc -- normalize || return 1
  if ! cmp -s api.json api.round1.json; then
    echo "autofix round 2 changed api.json"
    diff api.round1.json api.json | head -40
    return 1
  fi
  echo "autofix round 2 changed nothing"
  return 0
}
run autofix autofix

doc_check() {
  local code=0
  # the examples lint reads target/codegen: the bindings of THIS api.json, not the cache's older ones
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo run -r -p azul-doc -- check || code=1
  cargo run -r -p azul-doc -- check layout || code=1
  return "$code"
}
run doc_check doc_check

dll_check() {
  local code=0
  # the bindings of the api.json as it stands NOW (after the autofix step): azul-dll does not build without them
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo check -p azul-dll || code=1
  cargo test -p azul-srcgate --no-fail-fast || code=1
  return "$code"
}
run dll_check dll_check

contracts() {
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo test -p azul-dll --features build-dll --no-fail-fast \
    --test file_picker_tells_the_truth_contract \
    --test android_share_open_contract \
    --test android_documents_contract \
    --test android_picker_contract \
    --test file_picker_export_contract \
    --test android_extras_java_glue_contract \
    --test ios_share_open_contract \
    --test android_jni_symbols_match_the_java_glue \
    --test android_manifest_contract \
    --test android_extras_java_glue_contract
}
run contracts contracts

python() {
  local code=0
  python3 -m pip install --quiet --user tomli 2>/dev/null || true
  python3 scripts/test_demo_apps.py || code=1
  if [ -f scripts/test_documents_manifest.py ]; then
    python3 scripts/test_documents_manifest.py || code=1
  fi
  python3 scripts/demo_apps.py check || code=1
  # the pure parts of the two emulator / Simulator proofs (the bytes the self-test rows write, the lines the apps log)
  python3 -I scripts/ci/test_files_proofs.py || code=1
  return "$code"
}
run python python

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: FILES_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"
