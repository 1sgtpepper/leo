#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$ROOT/target/dce-purity-repro"
LEO=(cargo run -p leo-lang --bin leo --locked --features only_testnet --)

rm -rf "$WORK"
mkdir -p "$WORK"

run_leo_new() {
  local name="$1"
  (cd "$WORK" && "${LEO[@]}" new "$name")
}

build_project() {
  local name="$1"
  (cd "$WORK/$name" && "${LEO[@]}" build)
}

run_project() {
  local name="$1"
  shift
  (cd "$WORK/$name" && "${LEO[@]}" run "$@")
}

run_leo_new dce_wrapped_div_zero
cat > "$WORK/dce_wrapped_div_zero/src/main.leo" <<'LEO'
program dce_wrapped_div_zero.aleo {
    fn main(x: u8) -> u8 {
        let unused: u8 = x.div_wrapped(0u8);
        return x;
    }

    @noupgrade
    constructor() {}
}
LEO
build_project dce_wrapped_div_zero
echo "Wrapped division generated instructions:"
grep -n 'div.w\|output' "$WORK/dce_wrapped_div_zero/build/dce_wrapped_div_zero/dce_wrapped_div_zero.aleo"
if ! grep -q 'div.w' "$WORK/dce_wrapped_div_zero/build/dce_wrapped_div_zero/dce_wrapped_div_zero.aleo"; then
  echo "Expected div.w to be preserved" >&2
  exit 1
fi
set +e
div_output="$(run_project dce_wrapped_div_zero main 7u8 2>&1)"
div_status=$?
set -e
printf '%s\n' "$div_output"
if [[ "$div_status" -eq 0 ]]; then
  echo "Expected preserved div.w by zero to fail at runtime" >&2
  exit 1
fi

run_leo_new dce_wrapped_rem_zero
cat > "$WORK/dce_wrapped_rem_zero/src/main.leo" <<'LEO'
program dce_wrapped_rem_zero.aleo {
    fn main(x: u8) -> u8 {
        let unused: u8 = x.rem_wrapped(0u8);
        return x;
    }

    @noupgrade
    constructor() {}
}
LEO
build_project dce_wrapped_rem_zero
echo "Wrapped remainder generated instructions:"
grep -n 'rem.w\|output' "$WORK/dce_wrapped_rem_zero/build/dce_wrapped_rem_zero/dce_wrapped_rem_zero.aleo"
if ! grep -q 'rem.w' "$WORK/dce_wrapped_rem_zero/build/dce_wrapped_rem_zero/dce_wrapped_rem_zero.aleo"; then
  echo "Expected rem.w to be preserved" >&2
  exit 1
fi
set +e
rem_output="$(run_project dce_wrapped_rem_zero main 7u8 2>&1)"
rem_status=$?
set -e
printf '%s\n' "$rem_output"
if [[ "$rem_status" -eq 0 ]]; then
  echo "Expected preserved rem.w by zero to fail at runtime" >&2
  exit 1
fi

run_leo_new dyn_record_dce_probe
cat > "$WORK/dyn_record_dce_probe/src/main.leo" <<'LEO'
program dyn_record_dce_probe.aleo {
    record BadToken {
        owner: address,
        balance: field,
    }

    fn probe(t: BadToken) -> bool {
        let r: dyn record = t as dyn record;
        let must_halt: u64 = r.balance;
        return true;
    }

    @noupgrade
    constructor() {}
}
LEO
build_project dyn_record_dce_probe
echo "Dynamic record generated instructions:"
grep -n 'dynamic.record\|get.record.dynamic\|output' "$WORK/dyn_record_dce_probe/build/dyn_record_dce_probe/dyn_record_dce_probe.aleo"
if ! grep -q 'get.record.dynamic' "$WORK/dyn_record_dce_probe/build/dyn_record_dce_probe/dyn_record_dce_probe.aleo"; then
  echo "Expected get.record.dynamic to be preserved" >&2
  exit 1
fi

echo "DCE_PURITY_FIX_CONFIRMED"
