#!/usr/bin/env bash
set -euo pipefail

mode="${1:---unit}"

case "$mode" in
  --unit|--integration|--all)
    ;;
  *)
    echo "Unknown test mode: $mode" >&2
    exit 2
    ;;
esac

syntax_targets=(init.sh healthcheck.sh nut-bridge/*.sh)

bash -n "${syntax_targets[@]}"
shellcheck "${syntax_targets[@]}"

case "$mode" in
  --unit)
    bats tests/unit
    ;;
  --integration)
    bats tests/integration
    ;;
  --all)
    bats tests/unit tests/integration
    ;;
esac
