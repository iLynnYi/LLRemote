#!/usr/bin/env bash

set -u

usage() {
  cat <<'EOF'
Usage: scripts/test-win.sh --checks <rust,rust-check,service-build,frontend,tauri-build|all>
       [--rust-tests <lib,test_target,...>]

Runs selected checks in the Windows workspace through SSH.
rust-check compiles all Rust targets; service-build builds the service executable without packaging.
all preserves the rust,frontend,tauri-build checks.
Set WIN_TEST_HOST to the SSH destination. Set WIN_TEST_WORKSPACE to override the Windows source directory.
EOF
}

json_error() {
  printf 'VNC_WINDOWS_RESULT={"schemaVersion":1,"platform":"windows","status":"blocked","requestedChecks":[],"checks":[],"manual":{"status":"not-reached","items":[]},"error":{"code":"%s","message":"%s"}}\n' "$1" "$2"
}

checks=''
rust_tests=''
rust_tests_set=false
format='json'
while (($# > 0)); do
  case "$1" in
    --checks)
      if (($# < 2)); then json_error 'invalid_args' 'missing value for --checks'; exit 30; fi
      checks="$2"; shift 2 ;;
    --format)
      if (($# < 2)); then json_error 'invalid_args' 'missing value for --format'; exit 30; fi
      format="$2"; shift 2 ;;
    --rust-tests)
      if (($# < 2)); then json_error 'invalid_args' 'missing value for --rust-tests'; exit 30; fi
      rust_tests="$2"; rust_tests_set=true; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) json_error 'invalid_args' 'unknown argument'; exit 30 ;;
  esac
done

if [[ "$format" != 'json' ]]; then json_error 'invalid_format' 'only json format is supported'; exit 30; fi
if [[ -z "$checks" ]]; then json_error 'invalid_args' '--checks is required'; exit 30; fi
if [[ "$checks" == 'all' ]]; then checks='rust,frontend,tauri-build'; fi

if [[ "$rust_tests_set" == true ]]; then
  if [[ ",$checks," != *,rust,* || ! "$rust_tests" =~ ^[A-Za-z0-9_][A-Za-z0-9_-]*(,[A-Za-z0-9_][A-Za-z0-9_-]*)*$ ]]; then
    json_error 'invalid_rust_tests' 'Rust targets require rust and valid comma-separated names'; exit 30
  fi
  IFS=',' read -r -a rust_targets <<< "$rust_tests"
  if ((${#rust_targets[@]} > 32)); then json_error 'invalid_rust_tests' 'too many Rust targets'; exit 30; fi
  for rust_target in "${rust_targets[@]}"; do
    if ((${#rust_target} > 64)); then json_error 'invalid_rust_tests' 'Rust target name too long'; exit 30; fi
  done
fi

IFS=',' read -r -a check_items <<< "$checks"
for check in "${check_items[@]}"; do
  case "$check" in rust|rust-check|service-build|frontend|tauri-build) ;; *) json_error 'invalid_check' 'unsupported check'; exit 30 ;; esac
done

host="${WIN_TEST_HOST:-}"
if [[ -z "$host" ]]; then json_error 'ssh_host_required' 'Set WIN_TEST_HOST to the Windows SSH destination'; exit 30; fi
workspace="${WIN_TEST_WORKSPACE:-C:\\src\\LLRemote}"
remote_checks="$(IFS=,; printf '%s' "${check_items[*]}")"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if ! command -v node >/dev/null 2>&1; then json_error 'node_missing' 'Node.js is required by the Windows runner'; exit 30; fi
encoded="$(node "$script_dir/windows-runner.mjs" encode "$workspace" "$remote_checks" "$rust_tests")" || exit 30
remote_output="$(ssh -o BatchMode=yes -o ConnectTimeout=10 -- "$host" powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "$encoded" 2>&1)"
ssh_code=$?
if [[ "$ssh_code" == '255' ]]; then json_error 'ssh_auth_failed' 'Windows SSH connection or authentication failed'; exit 20; fi
if result="$(printf '%s\n' "$remote_output" | node "$script_dir/windows-runner.mjs" result "$remote_checks" "$ssh_code")"; then
  printf 'VNC_WINDOWS_RESULT=%s\n' "$result"
  exit "$ssh_code"
fi
json_error 'runner_protocol_error' 'Windows runner result is missing or inconsistent'
exit 40
