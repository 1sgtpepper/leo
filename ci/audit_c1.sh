#!/usr/bin/env bash
set -uo pipefail

ROOT="${GITHUB_WORKSPACE:?}"
LEO="${ROOT}/target/ci/leo"
PACKAGE="$ROOT/ci/c1-package"
ARTIFACT_DIR="${RUNNER_TEMP:?}/c1-artifacts"
mkdir -p "$ARTIFACT_DIR"

run_case() {
    local name="$1"
    shift
    local output="$ARTIFACT_DIR/${name}.log"
    (cd "$PACKAGE" && "$LEO" --disable-update-check --json-output="$ARTIFACT_DIR/${name}.json" "$@") >"$output" 2>&1
    local status=$?
    echo "$status" >"$ARTIFACT_DIR/${name}.status"
    return "$status"
}

explicit_ok=0
if run_case explicit-target run --build-tests main.aleo::main 7u32; then
    explicit_ok=1
fi
if (( explicit_ok == 0 )) || ! jq -e '.program == "main.aleo" and .function == "main"' "$ARTIFACT_DIR/explicit-target.json" >/dev/null 2>&1; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=C1 reason=explicit-primary-control-failed"
    exit 1
fi

if run_case omitted-target run --build-tests main 7u32; then
    if jq -e '.program == "main.aleo" and .function == "main"' "$ARTIFACT_DIR/omitted-target.json" >/dev/null 2>&1; then
        echo "AUDIT_RESULT=DISPROVED root=C1 downstream=implicit-primary-selected"
        exit 0
    fi
    if jq -e '.program == "test_main.aleo"' "$ARTIFACT_DIR/omitted-target.json" >/dev/null 2>&1; then
        echo "AUDIT_RESULT=CONFIRMED root=C1 downstream=test-unit-selected-as-primary"
        exit 0
    fi
    echo "AUDIT_RESULT=INCONCLUSIVE root=C1 reason=implicit-run-produced-unexpected-json"
    exit 1
fi

if rg -qi 'test_main|function.*main|does not exist' "$ARTIFACT_DIR/omitted-target.log"; then
    echo "AUDIT_RESULT=CONFIRMED root=C1 downstream=implicit-primary-run-rejected"
    exit 0
fi

echo "AUDIT_RESULT=INCONCLUSIVE root=C1 reason=implicit-run-failed-unrelated"
exit 1
