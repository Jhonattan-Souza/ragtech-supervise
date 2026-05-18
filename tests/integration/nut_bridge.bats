#!/usr/bin/env bats

load "../helpers/test_helper"
load "../fixtures/ragtech_db"

setup() {
  if [[ ! -S /var/run/docker.sock ]]; then
    skip "integration tests require /var/run/docker.sock"
  fi

  suffix="${BATS_TEST_NUMBER:-$$}-$RANDOM"
  bridge_image="ragtech-nut-bridge:local"
  test_image="${RAGTECH_TEST_IMAGE:-ragtech-supervise-tests:local}"
  network="ragtech-nut-test-$suffix"
  bridge_name="ragtech-nut-bridge-test-$suffix"
  data_dir="$(mktemp -d "$REPO_ROOT/.test-tmp.integration.XXXXXX")"
  db="$data_dir/monit.db"
}

teardown() {
  docker rm -f "$bridge_name" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -rf "${data_dir:-}"
}

upsc_from_network() {
  docker run --rm --network "$network" "$test_image" \
    upsc "ragtech@$bridge_name:3493" "$1"
}

wait_for_upsc_value() {
  local key="$1"
  local expected="$2"

  for _ in $(seq 1 60); do
    if value="$(upsc_from_network "$key" 2>/dev/null)" && [[ "$value" == "$expected" ]]; then
      return 0
    fi
    sleep 1
  done

  printf 'timed out waiting for %s=%s\n' "$key" "$expected" >&2
  docker logs "$bridge_name" >&2 || true
  return 1
}

wait_for_upsc_contains() {
  local key="$1"
  local expected="$2"

  for _ in $(seq 1 60); do
    if value="$(upsc_from_network "$key" 2>/dev/null)" && [[ " $value " == *" $expected "* ]]; then
      return 0
    fi
    sleep 1
  done

  printf 'timed out waiting for %s to contain %s\n' "$key" "$expected" >&2
  docker logs "$bridge_name" >&2 || true
  return 1
}

@test "NUT bridge container exposes generated SQLite telemetry without UPS hardware" {
  create_ragtech_schema "$db"
  insert_device "$db" ups-integration 1000 "Ragtech Integration UPS" "9.9"
  SAMPLE_ID=ups-integration SAMPLE_DT=1000 SAMPLE_EVENT=7 SAMPLE_C_BATTERY=91 SAMPLE_V_INPUT=126.5 SAMPLE_P_OUTPUT=25 insert_sample "$db" EVENTLOG

  docker build -f "$REPO_ROOT/nut-bridge/Dockerfile" -t "$bridge_image" "$REPO_ROOT"
  docker network create "$network"
  docker run -d \
    --name "$bridge_name" \
    --network "$network" \
    -v "$data_dir:/data" \
    -e NUT_MONITOR_PASSWORD=integration-secret \
    -e REQUIRE_FRESH_SAMPLE=0 \
    -e MAX_SAMPLE_AGE=0 \
    "$bridge_image"

  wait_for_upsc_value ups.status OL
  [[ "$(upsc_from_network experimental.ragtech.sample.valid)" == "1" ]]
  [[ "$(upsc_from_network experimental.ragtech.connection.status)" == "connected" ]]
  [[ "$(upsc_from_network battery.charge)" == "91" ]]
  [[ "$(upsc_from_network input.voltage)" == "126.5" ]]
  [[ "$(upsc_from_network ups.load)" == "25" ]]

  for _ in $(seq 1 30); do
    health="$(docker inspect -f '{{.State.Health.Status}}' "$bridge_name")"
    [[ "$health" == "healthy" ]] && return 0
    sleep 1
  done

  docker inspect "$bridge_name" >&2
  return 1
}

@test "NUT bridge container fails fast when password is missing" {
  docker build -f "$REPO_ROOT/nut-bridge/Dockerfile" -t "$bridge_image" "$REPO_ROOT"

  run docker run --rm "$bridge_image"

  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must be set explicitly"
}

@test "NUT bridge exposes alarm, low battery, and stale-source semantics through upsc" {
  create_ragtech_schema "$db"
  insert_device "$db" ups-integration 1000 "Ragtech Integration UPS" "9.9"
  SAMPLE_ID=ups-integration SAMPLE_DT=1000 SAMPLE_EVENT=7 SAMPLE_CONNECTED=0 insert_sample "$db" EVENTLOG

  docker build -f "$REPO_ROOT/nut-bridge/Dockerfile" -t "$bridge_image" "$REPO_ROOT"
  docker network create "$network"
  docker run -d \
    --name "$bridge_name" \
    --network "$network" \
    -v "$data_dir:/data" \
    -e NUT_MONITOR_PASSWORD=integration-secret \
    -e REQUIRE_FRESH_SAMPLE=0 \
    -e MAX_SAMPLE_AGE=2 \
    -e POLL_INTERVAL=1 \
    "$bridge_image"

  wait_for_upsc_contains ups.status OFF
  [[ "$(upsc_from_network ups.alarm)" == "Ragtech Supervise reports UPS disconnected" ]]
  [[ "$(upsc_from_network experimental.ragtech.connection.status)" == "disconnected" ]]

  SAMPLE_ID=ups-integration SAMPLE_DT=1001 SAMPLE_EVENT=8 SAMPLE_OP_BATTERY=1 SAMPLE_LO_BATTERY=1 insert_sample "$db" EVENTLOG
  wait_for_upsc_value ups.status "OB DISCHRG LB"
  [[ "$(upsc_from_network experimental.ragtech.connection.status)" == "connected" ]]

  wait_for_upsc_value experimental.ragtech.sample.valid 0
  wait_for_upsc_contains ups.status OFF
  [[ "$(upsc_from_network ups.alarm)" == "Ragtech telemetry unavailable: stale-source-sample" ]]
}
