#!/usr/bin/env bash

LEO_BIN="${LEO_BIN:-$(pwd)/target/debug/leo}"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

log() {
  printf '\n## %s\n' "$1"
}

result() {
  printf 'RESULT %-28s %-14s %s\n' "$1" "$2" "$3"
}

run_cmd() {
  local outfile="$1"
  shift
  "$@" >"$outfile" 2>&1
  return $?
}

new_project() {
  local name="$1"
  (cd "$ROOT" && "$LEO_BIN" new "$name" >/dev/null)
}

write_main() {
  local name="$1"
  cat > "$ROOT/$name/src/main.leo"
}

show_output() {
  local file="$1"
  sed -n '1,220p' "$file"
}

show_tooling() {
  log "Tooling"
  printf 'LEO_BIN=%s\n' "$LEO_BIN"
  "$LEO_BIN" --version || true
}
