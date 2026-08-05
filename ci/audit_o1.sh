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
DEVNODE_ENDPOINT="http://127.0.0.1:3030"
ENDPOINT="http://127.0.0.1:3031"
CONSENSUS_HEIGHTS="0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17"
DEVNODE_PID=""
PROXY_PID=""

start_devnode() {
    "$LEO" --disable-update-check devnode start --socket-addr 127.0.0.1:3030 --private-key "$PRIVATE_KEY" \
        >"$ARTIFACT_DIR/devnode.log" 2>&1 &
    DEVNODE_PID=$!
    for _ in $(seq 1 120); do
        if curl --fail --silent "$DEVNODE_ENDPOINT/testnet/block/height/latest" >/dev/null; then
            return 0
        fi
        sleep 1
    done
    return 1
}

start_proxy() {
    python3 "$ROOT/ci/o1_proxy.py" --listen-port 3031 --target-port 3030 \
        >"$ARTIFACT_DIR/proxy.log" 2>&1 &
    PROXY_PID=$!
    for _ in $(seq 1 30); do
        if curl --fail --silent "$ENDPOINT/testnet/block/height/latest" >/dev/null; then
            return 0
        fi
        sleep 1
    done
    return 1
}

stop_proxy() {
    if [[ -n "$PROXY_PID" ]]; then
        kill "$PROXY_PID" 2>/dev/null || true
        wait "$PROXY_PID" 2>/dev/null || true
        PROXY_PID=""
    fi
}

stop_devnode() {
    if [[ -n "$DEVNODE_PID" ]]; then
        kill "$DEVNODE_PID" 2>/dev/null || true
        wait "$DEVNODE_PID" 2>/dev/null || true
        DEVNODE_PID=""
    fi
}
stop_services() {
    stop_proxy
    stop_devnode
}
trap stop_services EXIT

run_deploy() {
    local name="$1"
    local output="$ARTIFACT_DIR/${name}.log"
    (
        cd "$WORKSPACE" || exit 125
        "$LEO" --disable-update-check --home "$LEO_HOME" --json-output="$ARTIFACT_DIR/${name}.json" deploy --broadcast \
            --network testnet --endpoint "$ENDPOINT" --private-key "$PRIVATE_KEY" \
            --consensus-heights "$CONSENSUS_HEIGHTS" -y
    ) >"$output" 2>&1
    local status=$?
    echo "$status" >"$ARTIFACT_DIR/${name}.status"
    return "$status"
}

if ! start_devnode; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=first-devnode-not-ready"
    exit 1
fi
if ! start_proxy; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=proxy-not-ready"
    exit 1
fi
if ! run_deploy skip-first; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=skip-first-deploy-failed"
    exit 1
fi
stop_devnode

if ! start_devnode; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=control-devnode-not-ready"
    exit 1
fi
if ! run_deploy all-confirmed; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=all-confirmed-control-failed"
    exit 1
fi

TARGET_RESULT="DISPROVED"
if ! jq -e '
    (.deployments | length == 2) and
    (.deployments[0].program_id == "first.aleo") and
    (.deployments[0].broadcast == null) and
    (.deployments[1].program_id == "second.aleo") and
    (.deployments[1].broadcast.confirmed == true)
  ' "$ARTIFACT_DIR/skip-first.json" >/dev/null; then
    if jq -e '
        (.deployments | length == 2) and
        (.deployments[0].program_id == "first.aleo") and
        (.deployments[0].broadcast.confirmed == true) and
        (.deployments[1].program_id == "second.aleo") and
        (.deployments[1].broadcast == null)
      ' "$ARTIFACT_DIR/skip-first.json" >/dev/null; then
        TARGET_RESULT="CONFIRMED"
    else
        echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=skip-first-json-not-decisive"
        exit 1
    fi
fi

if ! jq -e '(.deployments | length == 2) and all(.deployments[]; .broadcast.confirmed == true)' "$ARTIFACT_DIR/all-confirmed.json" >/dev/null; then
    echo "AUDIT_RESULT=INCONCLUSIVE root=O1 reason=all-confirmed-json-control-failed"
    exit 1
fi

if [[ "$TARGET_RESULT" == "CONFIRMED" ]]; then
    echo "AUDIT_RESULT=CONFIRMED root=O1 downstream=first-entry-received-second-broadcast"
    exit 0
fi

echo "AUDIT_RESULT=DISPROVED root=O1 downstream=skip-metadata-preserved"
exit 0
