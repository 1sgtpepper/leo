#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

show_tooling
log "dynamic array index liveness"

new_project dce_arr_dead
write_main dce_arr_dead <<'LEO'
program dce_arr_dead.aleo {
    @noupgrade
    constructor() {}

    fn main(i: u32) -> u8 {
        let arr: [u8; 2] = [1u8, 2u8];
        let dead: u8 = arr[i];
        return 9u8;
    }
}
LEO

dead_out="$ROOT/dce_arr_dead.out"
(cd "$ROOT/dce_arr_dead" && run_cmd "$dead_out" "$LEO_BIN" build)
dead_status=$?
show_output "$dead_out"

log "live dynamic array index control"
new_project dce_arr_live
write_main dce_arr_live <<'LEO'
program dce_arr_live.aleo {
    @noupgrade
    constructor() {}

    fn main(i: u32) -> u8 {
        let arr: [u8; 2] = [1u8, 2u8];
        let dead: u8 = arr[i];
        return dead;
    }
}
LEO

live_out="$ROOT/dce_arr_live.out"
(cd "$ROOT/dce_arr_live" && run_cmd "$live_out" "$LEO_BIN" build)
live_status=$?
show_output "$live_out"

if [ "$dead_status" -eq 0 ] && [ "$live_status" -ne 0 ]; then
  result dce_dynamic_array_index confirmed "dead dynamic index builds; live dynamic index fails"
elif [ "$dead_status" -ne 0 ] && [ "$live_status" -ne 0 ]; then
  result dce_dynamic_array_index fixed "both variants reject/fail before artifact success"
else
  result dce_dynamic_array_index inconclusive "dead status=$dead_status live status=$live_status"
fi
