#!/usr/bin/env bash
set -uo pipefail

ROOT="${GITHUB_WORKSPACE:?}"
LEO="${ROOT}/target/ci/leo"
ARTIFACT_DIR="${RUNNER_TEMP:?}/b1-artifacts"
mkdir -p "$ARTIFACT_DIR"
LEO_HOME="$ARTIFACT_DIR/leo-home"
mkdir -p "$LEO_HOME"

PRIVATE_KEY="APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"
ENDPOINT="http://127.0.0.1:3030"
CONSENSUS_HEIGHTS="0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17"

DEVNODE_LOG="$ARTIFACT_DIR/devnode.log"
"$LEO" --disable-update-check devnode start --socket-addr 127.0.0.1:3030 --private-key "$PRIVATE_KEY" >"$DEVNODE_LOG" 2>&1 &
DEVNODE_PID=$!
cleanup() {
    kill "$DEVNODE_PID" 2>/dev/null || true
    wait "$DEVNODE_PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 120); do
    if curl --fail --silent "$ENDPOINT/testnet/block/height/latest" >/dev/null; then
        break
    fi
    sleep 1
done
if ! curl --fail --silent "$ENDPOINT/testnet/block/height/latest" >/dev/null; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=B1 reason=devnode-not-ready"
    exit 1
fi

run_case() {
    local name="$1"
    local directory="$2"
    shift 2
    local output="$ARTIFACT_DIR/${name}.log"
    (
        cd "$directory" || exit 125
        "$LEO" --disable-update-check --home "$LEO_HOME" "$@"
    ) >"$output" 2>&1
    local status=$?
    echo "$status" >"$ARTIFACT_DIR/${name}.status"
    return "$status"
}

LIBRARY_PACKAGE="$ROOT/tests/tests/cli/test_library/contents/program_using_lib"
RENAME_CONTROL="$ROOT/tests/tests/cli/test_deploy_rename/contents"

if ! run_case baseline-build "$LIBRARY_PACKAGE" build; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=B1 reason=baseline-library-build-failed"
    exit 1
fi

if ! run_case no-rename-deploy "$LIBRARY_PACKAGE" deploy -y --print --skip-deploy-certificate \
    --network testnet --endpoint "$ENDPOINT" --private-key "$PRIVATE_KEY" \
    --consensus-heights "$CONSENSUS_HEIGHTS"; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=B1 reason=no-rename-deploy-failed"
    exit 1
fi

if ! run_case dependency-free-rename "$RENAME_CONTROL" deploy -y --print --skip-deploy-certificate \
    --rename renamed_prog --network testnet --endpoint "$ENDPOINT" --private-key "$PRIVATE_KEY" \
    --consensus-heights "$CONSENSUS_HEIGHTS"; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=B1 reason=dependency-free-rename-control-failed"
    exit 1
fi

if run_case renamed-library "$LIBRARY_PACKAGE" deploy -y --print --skip-deploy-certificate \
    --rename renamed_prog --network testnet --endpoint "$ENDPOINT" --private-key "$PRIVATE_KEY" \
    --consensus-heights "$CONSENSUS_HEIGHTS"; then
    echo "AUDIT_RESULT=DISPROVED root=B1 downstream=renamed-library-deploy-prepared"
    exit 0
fi

if rg -qi 'base_lib|unresolved|not found|missing' "$ARTIFACT_DIR/renamed-library.log"; then
    echo "AUDIT_RESULT=CONFIRMED root=B1 downstream=renamed-library-deploy-rejected"
    exit 0
fi

echo "AUDIT_RESULT=INCONCLUSIVE root=B1 reason=renamed-library-failed-with-unrelated-output"
exit 1
