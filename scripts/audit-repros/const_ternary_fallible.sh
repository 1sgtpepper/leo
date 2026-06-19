#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

show_tooling
log "constant ternary fallible arm"

new_project const_ternary_bug
write_main const_ternary_bug <<'LEO'
program const_ternary_bug.aleo {
    @noupgrade
    constructor() {}

    fn main(x: u8) -> u8 {
        let y: u8 = true ? x : x.div_wrapped(0u8);
        return y;
    }
}
LEO

out="$ROOT/const_ternary_bug.out"
(cd "$ROOT/const_ternary_bug" && run_cmd "$out" "$LEO_BIN" run main 7u8)
const_status=$?
show_output "$out"

log "dynamic ternary control"
new_project dynamic_ternary_probe
write_main dynamic_ternary_probe <<'LEO'
program dynamic_ternary_probe.aleo {
    @noupgrade
    constructor() {}

    fn main(cond: bool, x: u8) -> u8 {
        let y: u8 = cond ? x : x.div_wrapped(0u8);
        return y;
    }
}
LEO

dyn_out="$ROOT/dynamic_ternary_probe.out"
(cd "$ROOT/dynamic_ternary_probe" && run_cmd "$dyn_out" "$LEO_BIN" run main true 7u8)
dyn_status=$?
show_output "$dyn_out"

if [ "$const_status" -eq 0 ] && grep -q '7u8' "$out" && [ "$dyn_status" -ne 0 ]; then
  result const_ternary_fallible confirmed "constant fold returns while equivalent dynamic ternary halts"
elif [ "$const_status" -eq 0 ] && grep -q '7u8' "$out"; then
  result const_ternary_fallible partial "constant fold returns 7u8; dynamic control did not fail"
else
  result const_ternary_fallible inconclusive "constant case status=$const_status dynamic status=$dyn_status"
fi
