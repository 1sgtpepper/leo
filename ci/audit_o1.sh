#!/usr/bin/env bash
set -uo pipefail

ROOT="${GITHUB_WORKSPACE:?}"
LEO="${ROOT}/target/ci/leo"
WORKSPACE="$ROOT/ci/o1-workspace"
ARTIFACT_DIR="${RUNNER_TEMP:?}/o1-artifacts"
mkdir -p "$ARTIFACT_DIR"
LEO_HOME="$ARTIFACT_DIR/leo-home"
mkdir -p "$LEO_HOME"

PRIVATE_KEY="APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"
ENDPOINT="http://127.0.0.1:3030"
CONSENSUS_HEIGHTS="0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17"
DEVNODE_PID=""

start_devnode() {
    "$LEO" --disable-update-check devnode start --socket-addr 127.0.0.1:3030 --private-key "$PRIVATE_KEY" \
        >"$ARTIFACT_DIR/devnode.log" 2>&1 &
    DEVNODE_PID=$!
    for _ in $(seq 1 120); do
        if curl --fail --silent "$ENDPOINT/testnet/block/height/latest" >/dev/null; then
            return 0
        fi
        sleep 1
    done
    return 1
}

stop_devnode() {
    if [[ -n "$DEVNODE_PID" ]]; then
        kill "$DEVNODE_PID" 2>/dev/null || true
        wait "$DEVNODE_PID" 2>/dev/null || true
        DEVNODE_PID=""
    fi
}
trap stop_devnode EXIT

run_deploy() {
    local name="$1"
    local answers="$2"
    local output="$ARTIFACT_DIR/${name}.log"
    local command
    printf -v command 'cd %q && %q --disable-update-check --home %q --json-output=%q deploy --broadcast --network testnet --endpoint %q --private-key %q --consensus-heights %q' \
        "$WORKSPACE" "$LEO" "$LEO_HOME" "$ARTIFACT_DIR/${name}.json" "$ENDPOINT" "$PRIVATE_KEY" "$CONSENSUS_HEIGHTS"
    printf '%b' "$answers" | script -q -e -c "$command" /dev/null >"$output" 2>&1
    local status=$?
    echo "$status" >"$ARTIFACT_DIR/${name}.status"
    return "$status"
}

if ! start_devnode; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=first-devnode-not-ready"
    exit 1
fi
if ! run_deploy skip-first $'y\ny\ny\nn\ny\n'; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=skip-first-deploy-failed"
    exit 1
fi
stop_devnode

if ! start_devnode; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=control-devnode-not-ready"
    exit 1
fi
if ! run_deploy all-confirmed $'y\ny\ny\ny\ny\n'; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=all-confirmed-control-failed"
    exit 1
fi

if ! jq -e '
    (.deployments | length == 2) and
    (.[0].program_id == "first.aleo") and
    (.[0].broadcast == null) and
    (.[1].program_id == "second.aleo") and
    (.[1].broadcast.confirmed == true)
  ' "$ARTIFACT_DIR/skip-first.json" >/dev/null; then
    if jq -e '
        (.deployments | length == 2) and
        (.[0].program_id == "first.aleo") and
        (.[0].broadcast.confirmed == true) and
        (.[1].program_id == "second.aleo") and
        (.[1].broadcast == null)
      ' "$ARTIFACT_DIR/skip-first.json" >/dev/null; then
        echo "AUDIT_RESULT=CONFIRMED root=O1 downstream=first-entry-received-second-broadcast"
        exit 0
    fi
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=skip-first-json-not-decisive"
    exit 1
fi

if ! jq -e '(.deployments | length == 2) and all(.deployments[]; .broadcast.confirmed == true)' "$ARTIFACT_DIR/all-confirmed.json" >/dev/null; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=all-confirmed-json-control-failed"
    exit 1
fi

echo "AUDIT_RESULT=DISPROVED root=O1 downstream=skip-metadata-preserved"
exit 0
