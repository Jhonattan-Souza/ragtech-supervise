#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tests/run-in-container.sh [--unit|--integration|--all]

Runs the shell checks and Bats tests in a pinned Debian-based test container.
--unit is the default and does not require Docker socket access inside the tests.
--integration mounts the active Docker context's Unix socket and starts Docker containers.
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
  docker_endpoint="$(docker context inspect --format '{{.Endpoints.docker.Host}}')"
  if [[ "$docker_endpoint" != unix://* ]]; then
    echo "Integration tests require a Unix-socket Docker context; active endpoint is $docker_endpoint" >&2
    exit 1
  fi

  docker_socket="${docker_endpoint#unix://}"
  if [[ ! -S "$docker_socket" ]]; then
    echo "Docker socket not found at $docker_socket; integration tests require Docker daemon access." >&2
    exit 1
  fi
  docker_args+=(-v "$docker_socket:/var/run/docker.sock")
fi

docker "${docker_args[@]}" "$image" tests/run-tests.sh "$mode"
