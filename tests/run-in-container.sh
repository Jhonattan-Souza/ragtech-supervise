#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tests/run-in-container.sh [--unit|--integration|--all]

Runs the shell checks and Bats tests in a pinned Debian-based test container.
--unit is the default and does not require Docker socket access inside the tests.
--integration requires /var/run/docker.sock and starts Docker containers.
EOF
}

mode="--unit"
case "${1:-}" in
  ""|--unit)
    mode="--unit"
    ;;
  --integration|--all)
    mode="$1"
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
image="${RAGTECH_TEST_IMAGE:-ragtech-supervise-tests:local}"

docker build -f "$repo_root/tests/Dockerfile" -t "$image" "$repo_root"

docker_args=(
  run
  --rm
  -t
  -e RAGTECH_TEST_CONTAINER=1
  -e RAGTECH_TEST_IMAGE="$image"
  -v "$repo_root:$repo_root"
  -w "$repo_root"
)

if [[ "$mode" == "--integration" || "$mode" == "--all" ]]; then
  if [[ ! -S /var/run/docker.sock ]]; then
    echo "Docker socket not found at /var/run/docker.sock; integration tests require Docker daemon access." >&2
    exit 1
  fi
  docker_args+=(-v /var/run/docker.sock:/var/run/docker.sock)
fi

docker "${docker_args[@]}" "$image" tests/run-tests.sh "$mode"
