REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd -P)"

assert_success() {
  if [[ "$status" -ne 0 ]]; then
    printf 'expected success, got status %s\noutput:\n%s\n' "$status" "$output" >&2
    return 1
  fi
}

assert_failure() {
  if [[ "$status" -eq 0 ]]; then
    printf 'expected failure, got success\noutput:\n%s\n' "$output" >&2
    return 1
  fi
}

assert_output_contains() {
  local needle="$1"
  if [[ "$output" != *"$needle"* ]]; then
    printf 'expected output to contain %q\noutput:\n%s\n' "$needle" "$output" >&2
    return 1
  fi
}

assert_file_contains() {
  local file="$1"
  local needle="$2"
  if ! grep -Fq -- "$needle" "$file"; then
    printf 'expected %s to contain %q\ncontents:\n' "$file" "$needle" >&2
    sed -n '1,220p' "$file" >&2 || true
    return 1
  fi
}

refute_file_contains() {
  local file="$1"
  local needle="$2"
  if grep -Fq -- "$needle" "$file"; then
    printf 'expected %s not to contain %q\ncontents:\n' "$file" "$needle" >&2
    sed -n '1,220p' "$file" >&2 || true
    return 1
  fi
}

nut_value() {
  local file="$1"
  local key="$2"

  awk -v key="$key" '
    index($0, key ":") == 1 {
      value = substr($0, length(key) + 2)
      sub(/^ /, "", value)
      print value
      found = 1
      exit
    }
    END { exit found ? 0 : 1 }
  ' "$file"
}

assert_nut_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local actual

  actual="$(nut_value "$file" "$key")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected %s=%q, got %q\ncontents:\n' "$key" "$expected" "$actual" >&2
    sed -n '1,220p' "$file" >&2 || true
    return 1
  fi
}

wait_for_file_contains() {
  local file="$1"
  local needle="$2"
  local attempts="${3:-30}"

  for _ in $(seq 1 "$attempts"); do
    if [[ -f "$file" ]] && grep -Fq -- "$needle" "$file"; then
      return 0
    fi
    sleep 0.2
  done

  printf 'timed out waiting for %s to contain %q\n' "$file" "$needle" >&2
  [[ -f "$file" ]] && sed -n '1,220p' "$file" >&2
  return 1
}

make_fake_command() {
  local dir="$1"
  local name="$2"
  local body="$3"
  local path="$dir/$name"

  mkdir -p "$dir"
  {
    printf '#!/usr/bin/env bash\n'
    printf 'set -euo pipefail\n'
    printf '%s\n' "$body"
  } >"$path"
  chmod +x "$path"
}
